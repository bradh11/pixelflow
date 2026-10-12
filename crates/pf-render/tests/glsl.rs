//! The Shader effect: a shader file run over a prop's grid. That it draws what the file says with
//! the inputs the effect sets, the same a pixel at a time and a run at a time and on any number
//! of threads; that files are read once and compiled once; and that a file that can't be used,
//! or does more than it may, draws nothing and says why. Every shader here is written in the
//! test, or is one of PixelFlow's own.

use pf_model::{Corner, Generator, MatrixWiring, Orientation, Prop, ShapeSource, Show};
use pf_render::{
    Canvas, Colors, EffectTime, Pixel, RenderContext, RenderPool, Renderer, Rgba, Shade, Shader, ShaderInfo,
    ShaderInputInfo, Shaders, shader_input_problem, shader_name,
};
use pf_sequence::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const FRAME_MS: u32 = 25;

/// Every kind of input, each read straight out to a channel so a test can see what the shader
/// was given: the number and the choice in red, the switch in green, the point in blue, and the
/// color's green in alpha. `TIME` moves red on, a hundredth a second.
const INPUTS: &str = r#"/*{
  "DESCRIPTION": "Shows what it was given.",
  "CREDIT": "A test",
  "INPUTS": [
    { "NAME": "level", "LABEL": "Level", "TYPE": "float", "MIN": 0.0, "MAX": 0.5, "DEFAULT": 0.25 },
    { "NAME": "flip", "LABEL": "Flip", "TYPE": "bool", "DEFAULT": false },
    { "NAME": "mode", "LABEL": "Mode", "TYPE": "long", "VALUES": [0, 2, 5], "LABELS": ["None", "Two", "Five"], "DEFAULT": 2 },
    { "NAME": "at", "LABEL": "Where", "TYPE": "point2D", "MIN": [0.0, 0.0], "MAX": [1.0, 1.0], "DEFAULT": [0.25, 0.5] },
    { "NAME": "tint", "LABEL": "Tint", "TYPE": "color", "DEFAULT": [1.0, 0.5, 0.0, 1.0] },
    { "NAME": "go", "TYPE": "event" }
  ]
}*/
void main() {
    float red = level + float(mode) * 0.1 + TIME * 0.01;
    gl_FragColor = vec4(red, flip ? 1.0 : 0.25, at.x * 0.5 + at.y * 0.25, tint.g);
}"#;

/// The cell's own place: red across, green up, and the grid's size in blue.
const PLACE: &str =
    "void main() { gl_FragColor = vec4(gl_FragCoord.xy / 100.0, RENDERSIZE.x / 100.0, 1.0); }";

/// A hundred sines and cosines for every pixel: heavy enough to be shared out among threads.
const HEAVY: &str = r#"void main() {
    vec2 p = gl_FragCoord.xy / RENDERSIZE;
    float sum = 0.0;
    for (int i = 0; i < 100; i++) {
        sum += sin(p.x * float(i) + TIME) * cos(p.y * float(i) * 0.7 - TIME);
    }
    gl_FragColor = vec4(0.5 + sum * 0.02, p.x, p.y, 1.0);
}"#;

/// Goes round until a number that is different for every pixel passes a mark: more turns the
/// further right a pixel is, and no way to count them when the file is read.
const UNEVEN: &str = r#"/*{ "INPUTS": [ { "NAME": "turns", "TYPE": "float", "MIN": 0.0, "MAX": 1000000.0, "DEFAULT": 10.0 } ] }*/
void main() {
    float x = 0.0;
    float mark = turns * gl_FragCoord.x;
    while (x < mark) { x += 1.0; }
    gl_FragColor = vec4(1.0, 0.5, 0.0, 1.0);
}"#;

/// Never finishes on the last column of a grid 64 wide, which is none of the places tried
/// before a frame is drawn; everywhere else it's done at once.
const EDGE: &str = r#"void main() {
    float x = 0.0;
    float mark = gl_FragCoord.x > 63.0 ? 1.0e9 : 4.0;
    while (x < mark) { x += 1.0; }
    gl_FragColor = vec4(0.2, 0.1, 0.0, 1.0);
}"#;

/// Shader files by name, and how many times each was read.
fn library(files: &[(&str, &str)]) -> (Shaders, Arc<AtomicUsize>) {
    let files: HashMap<String, String> = files
        .iter()
        .map(|(n, s)| (n.to_string(), s.to_string()))
        .collect();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&reads);
    let shaders = Shaders::new(move |file| {
        count.fetch_add(1, Ordering::SeqCst);
        files.get(file).cloned().ok_or("it isn't there".to_string())
    });
    (shaders, reads)
}

fn of(file: &str) -> ShaderParams {
    ShaderParams {
        file: file.to_string(),
        ..ShaderParams::default()
    }
}

fn with(file: &str, inputs: &str) -> ShaderParams {
    ShaderParams {
        inputs: serde_json::from_str(inputs).unwrap(),
        ..of(file)
    }
}

fn cells(columns: u32, rows: u32) -> Vec<Pixel> {
    let at = |i: u32, n: u32| if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
    (0..rows)
        .flat_map(|y| (0..columns).map(move |x| (x, y)))
        .map(|(x, y)| Pixel {
            u: at(x, columns),
            v: at(y, rows),
            index: y * columns + x,
            count: columns * rows,
        })
        .collect()
}

/// The effect for one frame on a `columns` × `rows` grid, `t_ms` into an effect a second long.
fn effect(shaders: &Shaders, p: &ShaderParams, (columns, rows): (u32, u32), t_ms: u64) -> Shader {
    let time = EffectTime::within(0, 1000, t_ms).with_frame_ms(FRAME_MS);
    let cx = RenderContext::default().with_shaders(shaders, 0);
    Shader::in_context(
        &EffectParams::Shader(p.clone()),
        &time,
        Colors::new(&[]),
        7,
        Canvas { columns, rows },
        &cx,
    )
}

/// Every cell's color, bottom row first, each cell drawn on its own.
fn draw(shaders: &Shaders, p: &ShaderParams, grid: (u32, u32), t_ms: u64) -> Vec<Rgba> {
    let shader = effect(shaders, p, grid, t_ms);
    cells(grid.0, grid.1).iter().map(|px| shader.shade(px)).collect()
}

fn one(shaders: &Shaders, p: &ShaderParams, t_ms: u64) -> [f32; 4] {
    let c = draw(shaders, p, (1, 1), t_ms)[0];
    [c.r, c.g, c.b, c.a]
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-6)
}

#[test]
fn a_shader_sees_the_grid_as_its_picture_and_the_time_in_whole_frames() {
    let (shaders, _) = library(&[("place.fs", PLACE), ("in.fs", INPUTS)]);
    let shaders = shaders.waiting();
    let drawn = draw(&shaders, &of("place.fs"), (4, 3), 0);
    // Each cell's middle, counted from the bottom left, and the grid's columns.
    assert_eq!(drawn[0], Rgba::new(0.005, 0.005, 0.04, 1.0));
    assert_eq!(drawn[3], Rgba::new(0.035, 0.005, 0.04, 1.0));
    assert_eq!(drawn[11], Rgba::new(0.035, 0.025, 0.04, 1.0));
    // Time goes by the effect's frames: within a frame it stands still.
    let at = |t_ms| one(&shaders, &of("in.fs"), t_ms)[0];
    assert!((at(0) - 0.45).abs() < 1e-6);
    assert_eq!(at(500), at(524));
    assert!((at(500) - 0.455).abs() < 1e-6 && (at(525) - 0.45525).abs() < 1e-6);
    // Speed runs it faster, slower, or backwards; a lead-in starts it part-way in, to the frame.
    let timed = |speed: f32, lead_in: f32, t_ms| {
        let p = ShaderParams {
            speed,
            lead_in,
            ..of("in.fs")
        };
        one(&shaders, &p, t_ms)[0]
    };
    assert!((timed(2.0, 0.0, 500) - 0.46).abs() < 1e-6);
    assert!((timed(-1.0, 0.0, 500) - 0.445).abs() < 1e-6);
    assert!((timed(0.0, 0.0, 500) - 0.45).abs() < 1e-6);
    assert!((timed(1.0, 2.0, 500) - 0.475).abs() < 1e-6);
    assert_eq!(timed(1.0, 2.01, 500), timed(1.0, 2.0, 500));
}

#[test]
fn the_effect_sets_the_inputs_its_file_declares() {
    let (shaders, _) = library(&[("in.fs", INPUTS)]);
    let shaders = shaders.waiting();
    // Left alone, each is as the file has it.
    assert!(close(one(&shaders, &of("in.fs"), 0), [0.45, 0.25, 0.25, 0.5]));
    let set = r#"{ "level": 0.5, "flip": true, "mode": 5, "at": [1, 0], "tint": [0, 0.25, 0, 1] }"#;
    assert!(close(
        one(&shaders, &with("in.fs", set), 0),
        [1.0, 1.0, 0.5, 0.25]
    ));
    // Outside its range a value is brought inside; a choice the file doesn't offer, a value of
    // the wrong kind, and a name the file doesn't have leave the file's own.
    let odd =
        r#"{ "level": 9, "mode": 3, "at": [5, -5], "tint": [0, 7, 0, 1], "flip": [0.5, 0.5], "nothing": 1 }"#;
    assert!(close(one(&shaders, &with("in.fs", odd), 0), [0.7, 1.0, 0.5, 1.0]));
    // A curve moves one number of them over the effect, a point's coordinates each on its own.
    let mut curved = Effect::new(EffectKind::Shader, 0, 1000).with_params(EffectParams::Shader(of("in.fs")));
    curved.curves.insert("inputs.level".into(), Curve::ramp(0.0, 0.5));
    curved.curves.insert("inputs.at.y".into(), Curve::ramp(0.0, 1.0));
    let now = curved.at(500);
    let EffectParams::Shader(p) = &now.params else {
        unreachable!()
    };
    let halfway = one(&shaders, p, 500);
    // Level 0.25 (and 0.2 for the choice, 0.005 for the time); the point's x is still the
    // file's own 0.25, and its y is 0.5.
    assert!(close(halfway, [0.455, 0.25, 0.25, 0.5]), "{halfway:?}");
    let end = curved.at(999);
    let EffectParams::Shader(p) = &end.params else {
        unreachable!()
    };
    assert!((one(&shaders, p, 0)[2] - (0.125 + 0.999 * 0.25)).abs() < 1e-4);
}

#[test]
fn a_run_of_pixels_comes_out_as_each_would_on_its_own() {
    let (shaders, _) = library(&[("heavy.fs", HEAVY)]);
    let shaders = shaders.waiting();
    let files = [
        "heavy.fs",
        "pixelflow:aurora",
        "pixelflow:stained-glass",
        "pixelflow:lava-lamp",
    ];
    for file in files {
        let shader = effect(&shaders, &of(file), (40, 13), 475);
        let pixels = cells(40, 13);
        let alone: Vec<Rgba> = pixels.iter().map(|px| shader.shade(px)).collect();
        assert!(
            alone.iter().any(|c| c.a > 0.0 && c != &alone[0]),
            "{file} draws a pattern"
        );
        for run in [1, 7, 64, 256] {
            let mut together = vec![Rgba::CLEAR; pixels.len()];
            for (run, colors) in pixels.chunks(run).zip(together.chunks_mut(run)) {
                shader.shade_run(run, colors);
            }
            assert_eq!(together, alone, "{file}, {run} at a time");
        }
    }
}

fn matrix_show(columns: u32, rows: u32) -> Show {
    let matrix = Prop::new(
        "Matrix",
        ShapeSource::Generator(Generator::Matrix {
            columns,
            rows,
            width: 2.0,
            height: 1.0,
            wiring: MatrixWiring {
                start: Corner::BottomLeft,
                orientation: Orientation::Horizontal,
                serpentine: false,
            },
        }),
    );
    let mut show = Show::new("t");
    show.props.push(matrix);
    show
}

fn sequence_of(show: &Show, layers: Vec<Vec<Effect>>) -> Sequence {
    let mut seq = Sequence::new("s", 4_000);
    seq.frame_ms = FRAME_MS;
    let mut row = Row::new(Target::Prop(show.props[0].id));
    row.layers = layers.into_iter().map(|effects| Layer { effects }).collect();
    seq.rows.push(row);
    assert_eq!(validate_sequence(&seq, show), vec![]);
    seq
}

fn shader_effect(p: ShaderParams, start_ms: u64, end_ms: u64) -> Effect {
    Effect::new(EffectKind::Shader, start_ms, end_ms).with_params(EffectParams::Shader(p))
}

#[test]
fn frames_are_the_same_on_any_number_of_threads_and_however_the_show_is_cut() {
    let show = matrix_show(64, 20);
    let (shaders, _) = library(&[("heavy.fs", HEAVY), ("edge.fs", EDGE)]);
    // A heavy shader under a blurred one of PixelFlow's own, and one that stops some pixels.
    let mut blurred = shader_effect(of("pixelflow:nebula"), 0, 4_000);
    blurred.blur = 3;
    blurred.blend = Blend::Max;
    let mut stopping = shader_effect(of("edge.fs"), 0, 4_000);
    stopping.blend = Blend::Add;
    let seq = sequence_of(
        &show,
        vec![
            vec![shader_effect(of("heavy.fs"), 0, 4_000)],
            vec![blurred],
            vec![stopping],
        ],
    );
    let map = pf_mapping::map_show(&show).0;
    let draw = |pool: Option<Arc<RenderPool>>, tile: usize| -> Vec<Vec<u8>> {
        let mut renderer = Renderer::new(&show, &map);
        renderer.set_shaders(shaders.waiting());
        renderer.set_pool(pool);
        renderer.set_tile_pixels(tile);
        [0u64, 41, 3]
            .iter()
            .map(|&index| {
                let mut frame = vec![0u8; renderer.frame_len()];
                renderer.render_frame(&seq, index, &mut frame);
                frame
            })
            .collect()
    };
    let reference = draw(None, 2048);
    let lit = reference[1].iter().filter(|&&b| b > 0).count();
    assert!(lit > 1000, "{lit} bytes lit");
    assert_ne!(reference[0], reference[1], "it moves");
    let pool = RenderPool::new(3);
    for tile in [50, 300, 1_000_000] {
        assert_eq!(draw(pool.clone(), tile), reference, "3 threads, tiles of {tile}");
    }
    assert_eq!(draw(None, 97), reference, "one thread, tiles of 97");
    // The pixels the shader stopped on the last columns are told of: some, not the whole
    // effect, which went on drawing everywhere else.
    let stopped = shaders
        .stopped(seq.rows[0].layers[2].effects[0].id.seed())
        .unwrap();
    assert!(!stopped.whole);
    assert!(
        stopped.message().contains("for some pixels"),
        "{}",
        stopped.message()
    );
}

#[test]
fn a_file_is_read_once_and_compiled_once_for_its_contents() {
    let (shaders, reads) = library(&[("a.fs", INPUTS), ("copy of a.fs", INPUTS), ("place.fs", PLACE)]);
    let waiting = shaders.waiting();
    for t_ms in [0, 25, 500] {
        draw(&waiting, &of("a.fs"), (8, 8), t_ms);
        draw(&waiting, &of("copy of a.fs"), (3, 3), t_ms);
    }
    assert_eq!(reads.load(Ordering::SeqCst), 2, "each file once");
    // The same text under two names is one program; a different text is another.
    let (a, copy, place) = (
        waiting.load("a.fs").unwrap(),
        waiting.load("copy of a.fs").unwrap(),
        waiting.load("place.fs").unwrap(),
    );
    assert!(Arc::ptr_eq(&a, &copy) && !Arc::ptr_eq(&a, &place));
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    // A file that changed is read again once it's forgotten; one that didn't keeps its program.
    waiting.forget("a.fs");
    assert!(Arc::ptr_eq(&waiting.load("a.fs").unwrap(), &a));
    assert_eq!(reads.load(Ordering::SeqCst), 4);
    // PixelFlow's own need no library, and are never read from anywhere.
    let none = Shaders::none();
    assert!(
        draw(&none, &of("pixelflow:prism"), (8, 4), 0)
            .iter()
            .all(|c| c.a == 1.0)
    );
    assert!(
        draw(&none, &of("a.fs"), (8, 4), 0)
            .iter()
            .all(|c| *c == Rgba::CLEAR)
    );
    assert_eq!(reads.load(Ordering::SeqCst), 4);
}

#[test]
fn a_preview_draws_without_a_shader_until_it_has_been_read() {
    let gate = Arc::new(Mutex::new(()));
    let held = gate.lock().unwrap();
    let reader_gate = Arc::clone(&gate);
    let shaders = Shaders::new(move |file| {
        let _open = reader_gate.lock().unwrap();
        match file {
            "place.fs" => Ok(PLACE.to_string()),
            _ => Err("it isn't there".to_string()),
        }
    });
    let arrived = Arc::new(AtomicUsize::new(0));
    let told = Arc::clone(&arrived);
    shaders.on_arrival(Some(Arc::new(move || {
        told.fetch_add(1, Ordering::SeqCst);
    })));
    // While the file is read, on a thread of its own, the effect draws nothing.
    assert!(
        draw(&shaders, &of("place.fs"), (2, 2), 0)
            .iter()
            .all(|c| *c == Rgba::CLEAR)
    );
    assert!(
        draw(&shaders, &of("place.fs"), (2, 2), 0)
            .iter()
            .all(|c| *c == Rgba::CLEAR)
    );
    drop(held);
    for _ in 0..2000 {
        if arrived.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(arrived.load(Ordering::SeqCst), 1, "told once, when it arrived");
    assert_eq!(draw(&shaders, &of("place.fs"), (2, 2), 0)[0].a, 1.0);
}

#[test]
fn a_file_that_cant_be_used_draws_nothing_and_says_why() {
    let broken = "void main() {\n  float x = 1.0;\n  gl_FragColor = vec4(y);\n}";
    let endless = "void main() {\n  float x = 0.0;\n  for (;;) { x += 1.0; }\n}";
    let passes = r#"/*{ "PASSES": [ { "TARGET": "last", "PERSISTENT": true }, {} ] }*/ void main() { gl_FragColor = vec4(1.0); }"#;
    let picture = r#"/*{ "INPUTS": [ { "NAME": "inputImage", "TYPE": "image" } ] }*/ void main() { gl_FragColor = vec4(1.0); }"#;
    let (shaders, _) = library(&[
        ("broken.fs", broken),
        ("endless.fs", endless),
        ("passes.fs", passes),
        ("picture.fs", picture),
    ]);
    let shaders = shaders.waiting();
    let why = |file: &str| {
        assert!(
            draw(&shaders, &of(file), (4, 4), 0)
                .iter()
                .all(|c| *c == Rgba::CLEAR),
            "{file} draws nothing"
        );
        let info = shaders.describe(file);
        (info.problem.unwrap_or_default(), info.line)
    };
    assert_eq!(why("broken.fs"), ("line 3: there is no `y`".to_string(), Some(3)));
    assert_eq!(
        why("endless.fs"),
        ("line 3: a loop that never ends".to_string(), Some(3))
    );
    let (problem, line) = why("passes.fs");
    assert!(
        problem.contains("from one frame to the next") && line.is_none(),
        "{problem}"
    );
    assert!(why("picture.fs").0.contains("\"inputImage\" is a picture"));
    assert_eq!(why("gone.fs"), ("it isn't there".to_string(), None));
    assert_eq!(why("").0, "no shader is chosen");
    assert!(
        why("pixelflow:not-one")
            .0
            .contains("no shader of its own by that name")
    );
}

#[test]
fn a_shader_that_does_more_than_it_may_draws_nothing_and_says_so() {
    let (shaders, _) = library(&[("uneven.fs", UNEVEN)]);
    let shaders = shaders.waiting();
    let seed = 7;
    let lit = |turns: f32, grid: (u32, u32)| {
        let p = with("uneven.fs", &format!(r#"{{ "turns": {turns} }}"#));
        draw(&shaders, &p, grid, 0).iter().filter(|c| c.a > 0.0).count()
    };
    // Light enough everywhere: every pixel is drawn, and nothing is said.
    assert_eq!(lit(10.0, (16, 4)), 64);
    assert_eq!(shaders.stopped(seed), None);
    // Too much everywhere it was tried: the whole effect draws nothing, which is said.
    assert_eq!(lit(1_000_000.0, (16, 4)), 0);
    let stopped = shaders.stopped(seed).unwrap();
    assert!(stopped.whole && stopped.at_ms == 0);
    assert!(
        stopped
            .message()
            .starts_with("Stopped at 0:00.000: it does more work than a shader may")
    );
    // Drawn through again, it isn't stopped any more.
    assert_eq!(lit(10.0, (16, 4)), 64);
    assert_eq!(shaders.stopped(seed), None);
    // The more pixels a frame has, the less each may do: the same settings finish on a small
    // grid and are stopped on a very large one.
    assert_eq!(lit(40.0, (20, 20)), 400);
    assert_eq!(lit(40.0, (1000, 1000)), 0);
    assert!(shaders.stopped(seed).unwrap().whole);
}

#[test]
fn a_file_is_described_by_its_header_for_the_settings() {
    let (shaders, _) = library(&[("shaders/My Rings.fs", INPUTS)]);
    let info = shaders.describe("shaders/My Rings.fs");
    assert_eq!(
        (
            info.name.as_str(),
            info.built_in,
            info.description.as_str(),
            info.credit.as_str()
        ),
        ("My Rings", false, "Shows what it was given.", "A test")
    );
    assert_eq!((&info.problem, info.line), (&None, None));
    // The button isn't something an effect can set, so it isn't listed.
    let names: Vec<&str> = info.inputs.iter().map(ShaderInputInfo::name).collect();
    assert_eq!(names, ["level", "flip", "mode", "at", "tint"]);
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["file"], "shaders/My Rings.fs");
    assert_eq!(json["builtIn"], false);
    assert_eq!(
        json["inputs"][0],
        serde_json::json!({ "type": "number", "name": "level", "label": "Level", "min": 0.0, "max": 0.5, "default": 0.25 })
    );
    assert_eq!(
        json["inputs"][2]["options"],
        serde_json::json!([{ "value": 0, "label": "None" }, { "value": 2, "label": "Two" }, { "value": 5, "label": "Five" }])
    );
    assert_eq!(json["inputs"][3]["type"], "point");
    assert_eq!(
        json["inputs"][4]["default"],
        serde_json::json!([1.0, 0.5, 0.0, 1.0])
    );
    // PixelFlow's own are described too, by their titles.
    for own in pf_shader::BUILT_IN {
        let info = Shaders::none().describe(&format!("pixelflow:{}", own.name));
        assert_eq!(
            (info.name.as_str(), info.built_in, &info.problem),
            (own.title, true, &None)
        );
        assert!(!info.inputs.is_empty() && !info.description.is_empty());
    }
    assert_eq!(shader_name("C:\\Shaders\\Wavy Lines.FS"), "Wavy Lines");
    assert_eq!(shader_name("shaders/no-extension"), "no-extension");
    // The effect's own default names one of them.
    let default = Shaders::none().describe(DEFAULT_SHADER);
    assert!(default.built_in && default.problem.is_none(), "{default:?}");
}

fn problem(info: &ShaderInfo, inputs: &str, curves: &[(&str, Curve)]) -> Option<String> {
    let inputs: ShaderInputs = serde_json::from_str(inputs).unwrap();
    let curves = curves.iter().map(|(k, c)| (k.to_string(), c.clone())).collect();
    shader_input_problem(info, &inputs, &curves)
}

#[test]
fn what_an_effect_sets_is_held_against_the_file_in_plain_words() {
    let (shaders, _) = library(&[("in.fs", INPUTS)]);
    let info = shaders.describe("in.fs");
    let fine = r#"{ "level": 0.5, "flip": true, "mode": 5, "at": [1, 0], "tint": [0, 0.25, 0, 1] }"#;
    assert_eq!(
        problem(&info, fine, &[("inputs.level", Curve::ramp(0.0, 0.5))]),
        None
    );
    assert_eq!(problem(&info, "{}", &[("speed", Curve::ramp(0.0, 9.0))]), None);
    let said = |inputs: &str| problem(&info, inputs, &[]).unwrap();
    assert_eq!(
        said(r#"{ "glow": 1 }"#),
        "\"glow\" isn't one of this shader's inputs (in has: level, flip, mode, at, tint)"
    );
    assert_eq!(said(r#"{ "level": 0.75 }"#), "\"level\" is 0.75; use 0 to 0.5");
    assert_eq!(said(r#"{ "level": true }"#), "\"level\" is a number");
    assert_eq!(said(r#"{ "flip": 1 }"#), "\"flip\" is on or off: true or false");
    assert_eq!(
        said(r#"{ "mode": 3 }"#),
        "\"mode\" is 3; use one of 0 (None), 2 (Two), 5 (Five)"
    );
    assert_eq!(
        said(r#"{ "at": [0.5, 1.5] }"#),
        "\"at\" is 1.5; use 0 to 1 (across, then up)"
    );
    assert_eq!(said(r#"{ "at": 0.5 }"#), "\"at\" is a point: give [across, up]");
    assert_eq!(
        said(r#"{ "tint": [1, 0] }"#),
        "\"tint\" is a color: give [red, green, blue, alpha], each 0 to 1"
    );
    let curved = |key: &str, curve: Curve| problem(&info, "{}", &[(key, curve)]).unwrap();
    assert_eq!(
        curved("inputs.level", Curve::ramp(0.0, 2.0)),
        "the curve on \"level\" is 2; use 0 to 0.5"
    );
    assert_eq!(
        curved("inputs.at", Curve::ramp(0.0, 1.0)),
        "\"at\" is a point: a curve changes \"at.x\" or \"at.y\""
    );
    assert_eq!(
        curved("inputs.flip", Curve::ramp(0.0, 1.0)),
        "\"flip\" isn't a number, so it can't change over the effect"
    );
    assert!(
        curved("inputs.glow", Curve::ramp(0.0, 1.0)).starts_with("a curve changes \"glow\", which isn't")
    );
    // A file that can't be used has its own problem; its inputs aren't known to hold against.
    let gone = shaders.describe("gone.fs");
    assert_eq!(problem(&gone, r#"{ "glow": 1 }"#, &[]), None);
}

#[test]
fn it_draws_on_a_prop_over_the_layers_below_and_leaves_clear_what_it_leaves_clear() {
    let show = matrix_show(8, 4);
    // Lit on the left half, clear on the right.
    let half = "void main() { float on = gl_FragCoord.x < RENDERSIZE.x * 0.5 ? 1.0 : 0.0; gl_FragColor = vec4(1.0, 0.0, 0.0, on); }";
    let (shaders, _) = library(&[("half.fs", half)]);
    let seq = sequence_of(
        &show,
        vec![
            vec![Effect::new(EffectKind::On, 0, 4_000).with_palette([Rgb::new(0, 0, 100)])],
            vec![
                shader_effect(of("half.fs"), 0, 2_000),
                shader_effect(of("gone.fs"), 2_000, 4_000),
            ],
        ],
    );
    let mut renderer = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    renderer.set_shaders(shaders.waiting());
    let mut frame = vec![0u8; renderer.frame_len()];
    renderer.render(&seq, 500, &mut frame);
    let pixel = |frame: &[u8], x: usize, y: usize| -> [u8; 3] {
        let n = y * 8 + x;
        [frame[n * 3], frame[n * 3 + 1], frame[n * 3 + 2]]
    };
    assert_eq!(pixel(&frame, 0, 0), [255, 0, 0]);
    assert_eq!(pixel(&frame, 3, 3), [255, 0, 0]);
    assert_eq!(
        pixel(&frame, 4, 0),
        [0, 0, 100],
        "the layer below shows where it's clear"
    );
    // A file that isn't there draws nothing: the layer below, everywhere.
    renderer.render(&seq, 2_500, &mut frame);
    assert!(frame.chunks(3).all(|px| px == [0, 0, 100]));
    // Without the show's files a renderer still draws PixelFlow's own.
    let own = sequence_of(&show, vec![vec![shader_effect(of("pixelflow:prism"), 0, 4_000)]]);
    let mut bare = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    bare.render(&own, 500, &mut frame);
    assert!(
        frame.chunks(3).all(|px| px.iter().any(|&b| b > 200)),
        "a rainbow, lit everywhere"
    );
}
