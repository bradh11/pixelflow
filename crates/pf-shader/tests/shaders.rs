//! Shaders written for these tests, run and held against the same arithmetic done in Rust.

use pf_shader::{Limits, Machine, Program, RunStats, Uniforms};

fn compile(source: &str) -> Program {
    Program::compile(source, &Limits::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// Every cell of a `columns` × `rows` picture, `width` pixels at a time.
fn picture(
    program: &Program,
    uniforms: &Uniforms,
    columns: u32,
    rows: u32,
    width: usize,
) -> (Vec<[f32; 4]>, RunStats) {
    let mut uniforms = uniforms.clone();
    program.set_size(&mut uniforms, columns as f32, rows as f32);
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for y in 0..rows {
        for x in 0..columns {
            xs.push(x as f32 + 0.5);
            ys.push(y as f32 + 0.5);
        }
    }
    let mut out = vec![[0.0; 4]; xs.len()];
    let mut machine = Machine::new(program, width);
    let work = program.work_for(xs.len());
    let stats = machine.run(program, &uniforms, work, &xs, &ys, &mut out);
    (out, stats)
}

fn one(source: &str) -> [f32; 4] {
    let program = compile(source);
    picture(&program, &program.uniforms(), 1, 1, 8).0[0]
}

fn bits(pixels: &[[f32; 4]]) -> Vec<[u32; 4]> {
    pixels.iter().map(|p| p.map(f32::to_bits)).collect()
}

#[test]
fn a_color_and_a_gradient_come_out_as_written() {
    assert_eq!(
        one("void main() { gl_FragColor = vec4(0.25, 0.5, 0.75, 1.0); }"),
        [0.25, 0.5, 0.75, 1.0]
    );
    let program = compile(
        "void main() { vec2 uv = gl_FragCoord.xy / RENDERSIZE; gl_FragColor = vec4(uv, isf_FragNormCoord.x, 1.0); }",
    );
    let (out, _) = picture(&program, &program.uniforms(), 4, 2, 3);
    assert_eq!(out[0], [0.125, 0.25, 0.125, 1.0]);
    assert_eq!(out[3], [0.875, 0.25, 0.875, 1.0]);
    assert_eq!(out[7], [0.875, 0.75, 0.875, 1.0]);
}

#[test]
fn inputs_take_their_defaults_and_what_they_are_set_to() {
    let source = r#"/*{ "INPUTS": [
        { "NAME": "level", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.5 },
        { "NAME": "flip", "TYPE": "bool", "DEFAULT": 1 },
        { "NAME": "mode", "TYPE": "long", "VALUES": [0, 1, 2], "LABELS": ["a", "b", "c"], "DEFAULT": 2 },
        { "NAME": "at", "TYPE": "point2D", "DEFAULT": [0.25, 0.75] },
        { "NAME": "tint", "TYPE": "color", "DEFAULT": [1.0, 0.5, 0.25, 1.0] }
    ] }*/
    void main() {
        float a = flip ? level : -level;
        if (mode == 2) a += 1.0;
        gl_FragColor = vec4(a, at.x + at.y * 10.0, tint.g + tint.b, TIME);
    }"#;
    let program = compile(source);
    let mut uniforms = program.uniforms();
    assert_eq!(picture(&program, &uniforms, 1, 1, 4).0[0], [1.5, 7.75, 0.75, 0.0]);
    assert!(program.set_input(&mut uniforms, "level", &[1.25]));
    assert!(program.set_input(&mut uniforms, "flip", &[0.0]));
    assert!(program.set_input(&mut uniforms, "mode", &[1.0]));
    assert!(program.set_input(&mut uniforms, "at", &[0.5, 0.5]));
    assert!(program.set_input(&mut uniforms, "tint", &[0.0, 0.25, 0.25, 1.0]));
    assert!(!program.set_input(&mut uniforms, "nothing", &[1.0]));
    program.set_time(&mut uniforms, 3.0, 0.025, 120);
    assert_eq!(picture(&program, &uniforms, 1, 1, 4).0[0], [-1.25, 5.5, 0.5, 3.0]);
    // Out of its range, not a number, or not one of its choices: brought back inside.
    program.set_input(&mut uniforms, "level", &[99.0]);
    program.set_input(&mut uniforms, "mode", &[7.0]);
    program.set_input(&mut uniforms, "at", &[f32::NAN, 0.5]);
    assert_eq!(picture(&program, &uniforms, 1, 1, 4).0[0], [-1.0, 5.25, 0.5, 3.0]);
}

/// A shader whose pixels take different ways through loops, breaks, and returns.
const WANDER: &str = r#"
#define TURNS 12
float wander(vec2 p) {
    float n = 0.0;
    for (int i = 0; i < TURNS; i++) {
        if (length(p) > 2.0) break;
        p = vec2(p.x * p.x - p.y * p.y, 2.0 * p.x * p.y) + vec2(0.3, 0.5);
        if (p.x < 0.0) continue;
        n += 1.0;
    }
    return n;
}
float pick(float n) {
    if (n < 3.0) return 0.25;
    if (n < 6.0) return 0.5;
    return 1.0;
}
void main() {
    vec2 p = gl_FragCoord.xy / RENDERSIZE * 2.0 - 1.0;
    float n = wander(p);
    float v = pick(n);
    if (p.x > 0.0) { v = v * 0.5; } else if (p.y > 0.2) v += 0.125;
    gl_FragColor = vec4(n / 12.0, v, p.x > p.y ? 1.0 : 0.0, 1.0);
}"#;

fn wander_in_rust(x: f32, y: f32, columns: f32, rows: f32) -> [f32; 4] {
    let (px, py) = (x / columns * 2.0 - 1.0, y / rows * 2.0 - 1.0);
    let (mut a, mut b) = (px, py);
    let mut n = 0.0f32;
    for _ in 0..12 {
        if (a * a + b * b).sqrt() > 2.0 {
            break;
        }
        (a, b) = (a * a - b * b + 0.3, 2.0 * a * b + 0.5);
        if a < 0.0 {
            continue;
        }
        n += 1.0;
    }
    let mut v = if n < 3.0 {
        0.25
    } else if n < 6.0 {
        0.5
    } else {
        1.0
    };
    if px > 0.0 {
        v *= 0.5;
    } else if py > 0.2 {
        v += 0.125;
    }
    [n / 12.0, v, if px > py { 1.0 } else { 0.0 }, 1.0]
}

#[test]
fn pixels_that_take_different_ways_each_get_their_own_answer() {
    let program = compile(WANDER);
    let (columns, rows) = (37u32, 23u32);
    let (out, stats) = picture(&program, &program.uniforms(), columns, rows, 64);
    assert_eq!(stats.stopped, 0);
    let mut seen = std::collections::HashSet::new();
    for y in 0..rows {
        for x in 0..columns {
            let want = wander_in_rust(x as f32 + 0.5, y as f32 + 0.5, columns as f32, rows as f32);
            let got = out[(y * columns + x) as usize];
            assert_eq!(got.map(f32::to_bits), want.map(f32::to_bits), "at {x}, {y}");
            seen.insert(got[0].to_bits());
        }
    }
    // The picture does exercise different turn counts.
    assert!(seen.len() >= 6, "{}", seen.len());
}

#[test]
fn a_pixel_is_the_same_whichever_pixels_it_is_run_beside() {
    let program = compile(WANDER);
    let (wide, wide_stats) = picture(&program, &program.uniforms(), 37, 23, 1024);
    for width in [1, 2, 3, 7, 64, 100] {
        let (out, stats) = picture(&program, &program.uniforms(), 37, 23, width);
        assert_eq!(bits(&out), bits(&wide), "{width} at a time");
        assert_eq!(stats, wide_stats, "{width} at a time");
    }
}

#[test]
fn the_language_does_what_glsl_says() {
    // Arguments passed out and both ways, and components stored in another order.
    assert_eq!(
        one(
            "void split(in float a, out float b, inout vec2 c) { b = a * 2.0; c = c.yx + a; }
             void main() { float b; vec2 c = vec2(1.0, 2.0); split(3.0, b, c); vec3 v = vec3(1.0, 2.0, 3.0);
             v.zx = v.xy; v.xy = v.yx; gl_FragColor = vec4(b, c, v.x * 100.0 + v.y * 10.0 + v.z); }"
        ),
        [6.0, 5.0, 4.0, 221.0]
    );
    // Matrices: columns first, times a vector either side, times each other.
    assert_eq!(
        one("void main() { mat2 m = mat2(1.0, 2.0, 3.0, 4.0); vec2 v = vec2(1.0, 10.0);
             vec2 a = m * v; vec2 b = v * m; mat2 mm = m * m; mat3 i = mat3(2.0);
             gl_FragColor = vec4(a.x * 100.0 + a.y, b.x * 100.0 + b.y, mm[1][0] * 100.0 + mm[0][1], i[1][1] + i[2][1] + m[1].y); }"),
        [3142.0, 2143.0, 1510.0, 6.0]
    );
    // Whole numbers, and floats made from them.
    assert_eq!(
        one(
            "const int N = 7; void main() { int a = N / 2; int b = N - a * 2; float c = float(N) / 2.0;
             int d = int(-2.7); ivec2 e = ivec2(5, 3) % 4; int z = 0;
             gl_FragColor = vec4(a * 10 + b, c, d * 10 + e.x * 2 + e.y, N / z + 5 % z); }"
        ),
        [31.0, 3.5, -15.0, 0.0]
    );
    // Arrays, picked by a number worked out as it runs; one outside the array is the nearest in.
    assert_eq!(
        one(
            "void main() { float a[4]; for (int i = 0; i < 4; i++) a[i] = float(i * i);
             int k = int(gl_FragCoord.x + 2.0); vec3 v = vec3(7.0, 8.0, 9.0); v[k - 1] = 1.0;
             gl_FragColor = vec4(a[k], a[k + 40], a[k - 40] + v.y, v[k]); }"
        ),
        [4.0, 9.0, 1.0, 9.0]
    );
    // The right of `&&` and of `? :` runs only when it is the one wanted.
    assert_eq!(
        one("float n = 0.0; bool bump() { n += 1.0; return true; }
             void main() { bool a = false && bump(); bool b = true || bump(); bool c = true && bump();
             float d = a ? (n += 10.0) : (n += 100.0); gl_FragColor = vec4(n, d, float(b) + float(c), float(a)); }"),
        [101.0, 101.0, 2.0, 0.0]
    );
    // Counting up and down, before and after; do-while; a loop on a float.
    assert_eq!(
        one(
            "void main() { int i = 5; int a = i++; int b = ++i; float f = 1.0; f--; float s = 0.0;
             do { s += 1.0; } while (s < 3.5); float t = 0.0; for (float x = 0.0; x < 1.0; x += 0.25) t += x;
             gl_FragColor = vec4(a * 10 + b, f + s, t, i); }"
        ),
        [57.0, 4.0, 1.5, 7.0]
    );
    // The built-in functions, on vectors with a single number alongside.
    let got = one(
        "void main() { vec3 v = vec3(-1.5, 0.25, 2.5);
         vec3 a = clamp(v, 0.0, 1.0) + step(0.0, v) + mod(v, 1.0) + fract(v) + abs(v) + sign(v) + floor(v) + ceil(v);
         float b = mix(2.0, 4.0, 0.25) + smoothstep(0.0, 2.0, 1.0) + min(3.0, 1.0) + max(3.0, 1.0);
         float c = length(vec2(3.0, 4.0)) + dot(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0)) + distance(vec2(1.0), vec2(4.0, 5.0));
         vec3 d = cross(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0)) + normalize(vec3(0.0, 3.0, 4.0)) + reflect(vec3(1.0, -1.0, 0.0), vec3(0.0, 1.0, 0.0));
         gl_FragColor = vec4(a.x * 100.0 + a.y * 10.0 + a.z, b, c, d.x + d.y * 10.0 + d.z * 100.0); }",
    );
    // a.x: 0 + 0 + 0.5 + 0.5 + 1.5 − 1 − 2 − 1 = −1.5; a.y: 0.25 + 1 + 0.25 + 0.25 + 0.25 + 1 + 0 + 1 = 4;
    // a.z: 1 + 1 + 0.5 + 0.5 + 2.5 + 1 + 2 + 3 = 11.5.
    assert_eq!(got[0], -1.5 * 100.0 + 4.0 * 10.0 + 11.5);
    assert_eq!(got[1], 2.5 + 0.5 + 1.0 + 3.0);
    assert_eq!(got[2], 5.0 + 32.0 + 5.0);
    assert!(
        (got[3] - (1.0 + (0.6 + 1.0) * 10.0 + (1.0 + 0.8) * 100.0)).abs() < 1e-4,
        "{}",
        got[3]
    );
}

#[test]
fn macros_globals_and_a_missing_return_are_taken_in_stride() {
    // Globals worked out from inputs before `main`; a function that doesn't always return.
    assert_eq!(
        one(
            "#define half(x) ((x) * 0.5)\n#define FOUR 4.0\nfloat scale = RENDERSIZE.x * FOUR;
             float maybe(float x) { if (x < 1.0) return 9.0; }
             void main() { gl_FragColor = vec4(half(scale), maybe(0.5), maybe(2.0), 1.); }"
        ),
        [2.0, 9.0, 0.0, 1.0]
    );
    // discard leaves the pixel clear.
    let program = compile("void main() { gl_FragColor = vec4(1.0); if (gl_FragCoord.x > 2.0) discard; }");
    let (out, _) = picture(&program, &program.uniforms(), 4, 1, 4);
    assert_eq!(out, vec![[1.0; 4], [1.0; 4], [0.0; 4], [0.0; 4]]);
}

#[test]
fn what_cant_be_run_is_refused_in_plain_words() {
    let refused = |source: &str| Program::compile(source, &Limits::default()).unwrap_err().message;
    assert!(refused("void main() { gl_FragColor = vec3(1.0); }").contains("vec4"));
    assert!(refused("void main() { float x = y; }").contains("no `y`"));
    assert!(refused("void main() { TIME = 1.0; }").contains("can't be changed"));
    assert!(refused("void main() { gl_FragColor = vec4(noise(1.0)); }").contains("no function"));
    assert!(refused("float f() { return 1.0; }").contains("no `main`"));
    assert!(refused("void main() { break; }").contains("outside a loop"));
    assert!(refused("void main() { if (1.0) {} }").contains("truth"));
    assert!(refused("void main() { vec2 v = vec2(1.0); float x = v.z; }").contains("no `.z`"));
    assert!(refused("void main() { gl_FragColor = texture2D(a, vec2(0.0)); }").contains("pictures"));
    assert!(
        refused(r#"/*{"INPUTS":[{"NAME":"inputImage","TYPE":"image"}]}*/ void main() {}"#)
            .contains("\"inputImage\" is a picture")
    );
    // What a shader needs that isn't given yet is named, whatever the shader then does.
    let needs = [
        (
            r#"/*{"PASSES":[{"TARGET":"last","PERSISTENT":true},{}]}*/"#,
            "from one frame to the next",
        ),
        (r#"/*{"PASSES":[{"TARGET":"blur"},{}]}*/"#, "several passes"),
        (
            r#"/*{"IMPORTED":{"mask":{"PATH":"mask.png"}}}*/"#,
            "picture files of its own",
        ),
        (r#"/*{"INPUTS":[{"NAME":"sound","TYPE":"audio"}]}*/"#, "the music"),
    ];
    for (header, need) in needs {
        let why = refused(&format!("{header} void main() {{ gl_FragColor = vec4(1.0); }}"));
        assert!(why.contains(need) && why.contains("isn't supported yet"), "{why}");
    }
}

// ---- shaders that mean harm ------------------------------------------------------------------

#[test]
fn a_loop_without_end_stops_its_own_pixels_and_no_others() {
    // Forever on the left two columns, and a real answer on the rest.
    let program = compile(
        "void main() { float x = 0.0; while (gl_FragCoord.x < 2.0) { x += 1.0; }
         while (gl_FragCoord.x < 1.0) {}
         gl_FragColor = vec4(gl_FragCoord.x, 0.0, 0.0, 1.0); }",
    );
    let started = std::time::Instant::now();
    let (out, stats) = picture(&program, &program.uniforms(), 8, 4, 64);
    assert!(started.elapsed().as_secs() < 20);
    assert_eq!(stats.stopped, 8);
    for (k, px) in out.iter().enumerate() {
        let x = (k % 8) as f32 + 0.5;
        assert_eq!(*px, if x < 2.0 { [0.0; 4] } else { [x, 0.0, 0.0, 1.0] });
    }
    // The same however the pixels are batched.
    for width in [1, 5, 8] {
        let (again, again_stats) = picture(&program, &program.uniforms(), 8, 4, width);
        assert_eq!((bits(&again), again_stats.stopped), (bits(&out), 8));
    }
}

/// A loop of two billion turns that can't be counted from how it's written (its counter is a
/// float), so it is read, and cut when it's run.
const TWO_BILLION: &str = "void main() { float s = 0.0; for (float i = 0.0; i < 2.0e9; i += 1.0) { s += 0.5; } gl_FragColor = vec4(s); }";

#[test]
fn a_loop_of_two_billion_turns_is_cut_at_the_work_limit() {
    let program = compile(TWO_BILLION);
    let limits = Limits::default();
    let (out, stats) = picture(&program, &program.uniforms(), 16, 16, 64);
    assert_eq!(stats.stopped, 256);
    assert!(out.iter().all(|px| *px == [0.0; 4]));
    // Each pixel was charged the work it is allowed and less than a turn's more: the turn that
    // would have taken it past was never run.
    let each = stats.work / 256;
    assert!(
        each > u64::from(limits.work) && each < u64::from(limits.work) + 64,
        "{each}"
    );
    // A lower limit stops it sooner.
    let tight = Limits { work: 1000, ..limits };
    let program = Program::compile(TWO_BILLION, &tight).unwrap();
    let (_, stats) = picture(&program, &program.uniforms(), 4, 4, 16);
    assert!(stats.work / 16 < 1100);
}

#[test]
fn a_frame_of_many_pixels_gives_each_less_to_do() {
    let limits = Limits {
        work: 10_000,
        frame_work: 1_000_000,
        ..Limits::default()
    };
    let program = Program::compile(TWO_BILLION, &limits).unwrap();
    // Up to a hundred pixels each may do all a pixel may; past that, a share of the frame's.
    assert_eq!(program.work_for(0), 10_000);
    assert_eq!(program.work_for(100), 10_000);
    assert_eq!(program.work_for(400), 2_500);
    assert_eq!(program.work_for(usize::MAX), 0);
    for (columns, rows) in [(10, 10), (40, 25), (100, 100)] {
        let pixels = (columns * rows) as u64;
        let (_, stats) = picture(&program, &program.uniforms(), columns, rows, 256);
        assert_eq!(stats.stopped, pixels);
        // The frame's whole work stays near what a frame may do: over by a turn a pixel at most.
        assert!(
            stats.work <= limits.frame_work + pixels * 64,
            "{columns} by {rows}: {}",
            stats.work
        );
    }
    // The work a pixel is charged is the same however the pixels are batched.
    let (_, wide) = picture(&program, &program.uniforms(), 40, 25, 256);
    let (_, narrow) = picture(&program, &program.uniforms(), 40, 25, 7);
    assert_eq!(wide, narrow);
    // A shader with no loop at all is held to it too: a pixel is stopped before the lines that
    // would take it past, on whichever side of an `if` they are.
    let mut lines = String::from("void main() { float x = gl_FragCoord.x; if (x > 2.0) {\n");
    for _ in 0..200 {
        lines.push_str("x = sin(x) * 1.0001 + 0.5;\n");
    }
    lines.push_str("} gl_FragColor = vec4(x, 0.0, 0.0, 1.0); }");
    let program = Program::compile(&lines, &limits).unwrap();
    assert!(program.least_work() < 100, "{}", program.least_work());
    let (out, stats) = picture(&program, &program.uniforms(), 4, 1000, 256);
    assert_eq!(program.work_for(4000), 250);
    assert_eq!(stats.stopped, 2000);
    for (k, px) in out.iter().enumerate() {
        let x = (k % 4) as f32 + 0.5;
        assert_eq!(*px, if x > 2.0 { [0.0; 4] } else { [x, 0.0, 0.0, 1.0] });
    }
}

#[test]
fn what_can_never_finish_is_refused_when_it_is_read() {
    let refused = |source: &str| Program::compile(source, &Limits::default()).unwrap_err();
    // A loop nothing asks to stop and nothing leaves, wherever it stands.
    let endless = [
        "void main() {\n float x = 0.0;\n for (;;) { x += 1.0; }\n gl_FragColor = vec4(x); }",
        "void main() {\n float x = 0.0;\n\n while (true) { x += 1.0; if (x > 3.0) continue; }\n }",
        "float f() { float x = 0.0;\n\n for (;;) x += 1.0;\n return x; }\n void main() { if (gl_FragCoord.x < 2.0) gl_FragColor = vec4(f()); }",
    ];
    for (source, line) in endless.into_iter().zip([3, 4, 3]) {
        let error = refused(source);
        assert_eq!(
            (error.message.as_str(), error.line),
            ("a loop that never ends", line),
            "{source}"
        );
    }
    // One with a way out is read: a `break`, a `return`, or a `discard`.
    for source in [
        "void main() { float x = 0.0; for (;;) { x += 1.0; if (x > 3.0) break; } gl_FragColor = vec4(x); }",
        "float f() { float x = 0.0; while (true) { x += 1.0; if (x > 3.0) return x; } } void main() { gl_FragColor = vec4(f()); }",
        "void main() { float x = 0.0; for (;;) { x += 1.0; if (x > 3.0) discard; } }",
    ] {
        compile(source);
    }
    // Loops whose turns can be counted, and come to more than a pixel may do.
    let counted = [
        "void main() { float s = 0.0; for (int i = 0; i < 2000000000; i++) { s += 0.5; } gl_FragColor = vec4(s); }",
        "void main() { float s = 0.0; for (int i = 0; i < 1000; i++) for (int j = 0; j < 1000; j++) for (int k = 0; k < 1000; k++) s += gl_FragCoord.x; gl_FragColor = vec4(s); }",
        // Counting the wrong way: it would go round until the counter wrapped.
        "void main() { float s = 0.0; for (int i = 0; i < 10; i--) { s += 0.5; } gl_FragColor = vec4(s); }",
        "const int N = 700; void main() { float s = 0.0; for (int i = N; i >= -N; i -= 1) { for (int j = 0; j != N; j += 2) { s += sin(float(j)); } } gl_FragColor = vec4(s); }",
    ];
    for source in counted {
        let error = refused(source);
        assert!(
            error.message.contains("more work for every pixel"),
            "{source}: {error}"
        );
    }
    // Line after line, with no loop at all.
    let mut long = String::from("void main() { float x = gl_FragCoord.x;\n");
    for _ in 0..5_000 {
        long.push_str("x = sin(x) * 1.0001 + 0.5;\n");
    }
    long.push_str("gl_FragColor = vec4(x); }");
    let tight = Limits {
        work: 50_000,
        ..Limits::default()
    };
    let error = Program::compile(&long, &tight).unwrap_err();
    assert!(error.message.contains("more work for every pixel"), "{error}");
    assert!(Program::compile(&long, &Limits::default()).is_ok());
    // The same loops are read where a pixel can leave them early, the counter is changed inside,
    // or the turns are few: what they cost is then found out as they run.
    for source in [
        "void main() { float s = 0.0; for (int i = 0; i < 2000000000; i++) { s += 0.5; if (s > 4.0) break; } gl_FragColor = vec4(s); }",
        "void main() { float s = 0.0; for (int i = 0; i < 2000000000; i++) { s += 0.5; i += 1000000000; } gl_FragColor = vec4(s); }",
        "void bump(inout int n) { n += 1000000000; } void main() { float s = 0.0; for (int i = 0; i < 2000000000; i++) { s += 0.5; bump(i); } gl_FragColor = vec4(s); }",
        "void main() { float s = 0.0; for (int i = 0; i < 2000000000; i++) { if (gl_FragCoord.x > 0.0) return; s += 0.5; } gl_FragColor = vec4(s); }",
        "void main() { float s = 0.0; for (int i = 0; i < 100; i++) for (int j = 0; j < 10; j++) s += 0.5; gl_FragColor = vec4(s); }",
    ] {
        let program = compile(source);
        let (_, stats) = picture(&program, &program.uniforms(), 2, 2, 4);
        assert_eq!(stats.stopped, 0, "{source}");
    }
    // The least a pixel does is a lower bound on what it is then charged.
    let program = compile(
        "void main() { float s = 0.0; for (int i = 0; i < 100; i++) { s += sin(gl_FragCoord.x); if (s > 1e9) continue; s *= 0.5; } gl_FragColor = vec4(s); }",
    );
    let (_, stats) = picture(&program, &program.uniforms(), 3, 3, 4);
    assert!(program.least_work() > 1_000, "{}", program.least_work());
    assert!(
        stats.work / 9 >= program.least_work(),
        "{} against {}",
        stats.work / 9,
        program.least_work()
    );
}

#[test]
fn recursion_is_refused_however_it_is_dressed() {
    let refused = |source: &str| Program::compile(source, &Limits::default()).unwrap_err().message;
    assert!(
        refused("float f(float x) { return f(x - 1.0); } void main() { gl_FragColor = vec4(f(1.0)); }")
            .contains("calls itself")
    );
    assert!(
        refused(
            "float g(float x); float f(float x) { return g(x); } float g(float x) { return f(x); }
         void main() { gl_FragColor = vec4(f(1.0)); }"
        )
        .contains("calls itself")
    );
    // A chain of calls, each to the next, too long to follow.
    let mut chain = String::from("float f0(float x) { return x; }\n");
    for i in 1..200 {
        chain.push_str(&format!(
            "float f{i}(float x) {{ return f{}(x) + 1.0; }}\n",
            i - 1
        ));
    }
    chain.push_str("void main() { gl_FragColor = vec4(f199(0.0)); }");
    assert!(refused(&chain).contains("nest too deeply"));
}

#[test]
fn a_shader_that_swells_when_its_calls_are_written_out_is_refused_quickly() {
    // Each function calls the one before it twice: 2^40 calls from forty lines.
    let mut source = String::from("float f0(float x) { return x * 0.5; }\n");
    for i in 1..30 {
        source.push_str(&format!(
            "float f{i}(float x) {{ return f{}(x) + f{}(x + 1.0); }}\n",
            i - 1,
            i - 1
        ));
    }
    source.push_str("void main() { gl_FragColor = vec4(f29(0.0)); }");
    let started = std::time::Instant::now();
    let error = Program::compile(&source, &Limits::default()).unwrap_err();
    assert!(error.message.contains("too large"), "{error}");
    assert!(started.elapsed().as_secs() < 10);
}

#[test]
fn absurd_sizes_are_refused() {
    let refused = |source: &str| Program::compile(source, &Limits::default()).unwrap_err().message;
    assert!(refused("void main() { float a[2000000000]; }").contains("elements"));
    assert!(refused("void main() { float a[0]; }").contains("elements"));
    assert!(refused("void main() { float a[-1]; }").contains("elements"));
    assert!(refused("float n = 4.0; void main() { float a[int(n)]; }").contains("constant"));
    // Arrays that each fit, and together don't.
    let mut many = String::from("void main() {\n");
    for i in 0..40 {
        many.push_str(&format!("vec4 a{i}[1024];\n"));
    }
    many.push('}');
    assert!(refused(&many).contains("too many values"));
    // A file too large to be a shader.
    let huge = format!("void main() {{}}\n// {}", "x".repeat(300 * 1024));
    assert!(refused(&huge).contains("too large"));
    // More tokens than a shader has.
    let long = format!("void main() {{ {} }}", ";".repeat(250_000));
    assert!(refused(&long).contains("too long"));
}

#[test]
fn dividing_by_zero_and_not_a_number_come_out_as_numbers_do() {
    let got = one("void main() { float z = 0.0; int i = 0; float inf = 1.0 / z; float nan = z / z;
         gl_FragColor = vec4(inf, nan, float(7 / i) + float(7 % i), sqrt(-1.0) + log(-1.0) + pow(-2.0, 0.5)); }");
    assert_eq!(got[0], f32::INFINITY);
    assert!(got[1].is_nan());
    assert_eq!(got[2], 0.0);
    assert!(got[3].is_nan());
    // Comparisons with it are false, so it takes the `else`; the smaller and larger of it and a
    // number are settled too.
    let got = one(
        "void main() { float z = 0.0; float nan = z / z; float a = nan > 0.0 ? 1.0 : 2.0;
         gl_FragColor = vec4(a, min(nan, 1.0), max(1.0, nan), clamp(nan, 0.0, 1.0)); }",
    );
    assert_eq!(got[0], 2.0);
    assert!(got[1].is_nan() && got[3].is_nan());
    assert_eq!(got[2], 1.0);
    // A whole number from it, or from more than a whole number holds: settled, not a crash.
    let got = one(
        "void main() { float z = 0.0; int a = int(z / z); int b = int(1e30); int c = int(-1e30);
         int big = 2147483647; gl_FragColor = vec4(a, b, c, big + 1); }",
    );
    assert_eq!(got, [0.0, 2147483647.0, -2147483648.0, -2147483648.0]);
}

#[test]
fn nonsense_is_refused_and_never_a_crash() {
    // Statements thrown together inside a shader, then half the time damaged: a piece dropped,
    // doubled, or the file cut short. Whatever comes of it is an answer.
    let statements = [
        "x += f(x);",
        "v = vec3(x).zyx * m[1].xyx;",
        "for (int i = 0; i < 3; i++) {",
        "if (x > gl_FragCoord.y) {",
        "} else {",
        "}",
        "a[k] = v[k] + a[k + 1];",
        "k = int(x * 3.0) % 4;",
        "m[1] *= m;",
        "m *= mat2(x, v.xy, 1.0);",
        "if (v.x < x) break;",
        "if (v.y < x) continue;",
        "if (x > 9.0) return;",
        "x = v.x > 0.5 ? f(x) : -x;",
        "bool b = x > 0.5 && f(x) > 0.25 || k == 2;",
        "v.zx = v.xy * A(x);",
        "do { x -= 0.25; } while (x > 1.0);",
        "while (x < gl_FragCoord.x) x += 0.5;",
        "if (x < 0.1) discard;",
        "v[k] = sin(x) + pow(v.x, 2.0) / (x - x);",
        "float x = v.z;",
        "{",
    ];
    let damage = [
        "(",
        ")",
        "{",
        "}",
        ";",
        "+",
        "?",
        ":",
        ",",
        "[",
        "]",
        "=",
        "#if 1\n",
        "\n#endif\n",
        "return",
    ];
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let (mut ran, mut refused) = (0, 0);
    for round in 0..3000 {
        let mut parts: Vec<&str> = vec![
            "#define A(x) (x + x)\n",
            "float f(float x) { if (x > 0.5) return x * 0.5; return x + 0.25; }\n",
            "void main() { float x = gl_FragCoord.x; vec3 v = vec3(0.5); mat2 m = mat2(1.0); float a[5]; int k = 0;\n",
        ];
        for _ in 0..(next() % 10) {
            parts.push(statements[(next() % statements.len() as u64) as usize]);
        }
        parts.push("gl_FragColor = vec4(v, x); }");
        if round % 2 == 1 {
            for _ in 0..=(next() % 3) {
                let at = (next() as usize) % parts.len();
                match next() % 3 {
                    0 => drop(parts.remove(at)),
                    1 => parts.insert(at, parts[at]),
                    _ => parts.insert(at, damage[(next() % damage.len() as u64) as usize]),
                }
                if parts.is_empty() {
                    break;
                }
            }
        }
        let mut source = parts.join(" ");
        if round % 7 == 3 {
            source.truncate((next() as usize) % (source.len() + 1));
        }
        match Program::compile(&source, &Limits::default()) {
            Ok(program) => {
                let (out, _) = picture(&program, &program.uniforms(), 5, 3, 4);
                // And it is the same run another way.
                let (again, _) = picture(&program, &program.uniforms(), 5, 3, 1);
                assert_eq!(bits(&out), bits(&again), "{source}");
                ran += 1;
            }
            // Refused for what it is, never because reading it went wrong.
            Err(e) => {
                assert!(!e.message.contains("fault"), "{e}\n{source}");
                refused += 1;
            }
        }
    }
    // Plenty of it does make a shader, and plenty doesn't.
    assert!(ran > 300 && refused > 300, "{ran} ran, {refused} refused");
    // Bytes that are no text GLSL has.
    assert!(Program::compile("void main() { \u{1F600} }", &Limits::default()).is_err());
    assert!(Program::compile("\0\0\0", &Limits::default()).is_err());
    assert!(Program::compile("", &Limits::default()).is_err());
}
