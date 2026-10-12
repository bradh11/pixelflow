//! Spike: effects drawn by shader files, through the renderer.
//!
//! One test, because the table of shader effects is one for the whole program.

use pf_model::{Generator, Group, MatrixWiring, Prop, ShapeSource, Show, Transform, Vec3};
use pf_render::glsl::{ShaderSetup, set_shader_batch, set_shader_effects, take_shader_counts};
use pf_render::{RenderPool, Renderer};
use pf_sequence::*;
use pf_shader::{Limits, Program};
use std::collections::HashMap;
use std::sync::Arc;

/// Rings that drift and a loop each pixel leaves at its own turn.
const RINGS: &str = r#"/*{ "INPUTS": [
    { "NAME": "rings", "TYPE": "float", "MIN": 1.0, "MAX": 20.0, "DEFAULT": 6.0 },
    { "NAME": "tint", "TYPE": "color", "DEFAULT": [1.0, 0.5, 0.25, 1.0] }
] }*/
void main() {
    vec2 p = gl_FragCoord.xy / RENDERSIZE - 0.5;
    p.x *= RENDERSIZE.x / RENDERSIZE.y;
    float r = length(p);
    float wave = 0.5 + 0.5 * sin(r * rings * 6.2831853 - TIME * 3.0);
    float steps = 0.0;
    for (int i = 0; i < 12; i++) {
        if (r * float(i) > 1.0) break;
        steps += 1.0;
    }
    gl_FragColor = vec4(tint.rgb * wave * (steps / 12.0), 1.0);
}"#;

/// Never done, wherever it is asked.
const FOREVER: &str =
    "void main() { float x = gl_FragCoord.x; while (true) { x = x * 0.5 + 1.0; } gl_FragColor = vec4(x); }";

/// Six matrices of 30 by 20 side by side, in a group: 3,600 pixels.
fn show() -> Show {
    let mut show = Show::new("t");
    for k in 0..6 {
        let mut prop = Prop::new(
            format!("M{k}"),
            ShapeSource::Generator(Generator::Matrix {
                columns: 30,
                rows: 20,
                width: 3.0,
                height: 2.0,
                wiring: MatrixWiring::default(),
            }),
        );
        prop.transform = Transform {
            position: Vec3::new(k as f32 * 3.5, 0.0, 0.0),
            ..Transform::default()
        };
        show.props.push(prop);
    }
    let mut group = Group::new("All");
    group.members = show.props.iter().map(|p| p.id.into()).collect();
    show.groups.push(group);
    show
}

fn frames(show: &Show, seq: &Sequence, pool: Option<Arc<RenderPool>>, tile: Option<usize>) -> Vec<u8> {
    let mut renderer = Renderer::new(show, &pf_mapping::map_show(show).0);
    renderer.set_pool(pool);
    if let Some(tile) = tile {
        renderer.set_tile_pixels(tile);
    }
    let mut frame = vec![0u8; renderer.frame_len()];
    let mut all = Vec::new();
    for t_ms in (0..2000).step_by(250) {
        renderer.render(seq, t_ms, &mut frame);
        all.extend_from_slice(&frame);
    }
    all
}

fn setup(source: &str, limits: &Limits, inputs: Vec<(String, Vec<f32>)>) -> Arc<ShaderSetup> {
    Arc::new(ShaderSetup {
        program: Arc::new(Program::compile(source, limits).expect("the shader compiles")),
        inputs,
        speed: 1.0,
        lead_in_frames: 0,
    })
}

#[test]
fn a_shader_effect_is_the_same_bytes_however_it_is_drawn_and_a_runaway_one_is_stopped() {
    let show = show();
    let group = Target::Group(show.groups[0].id);
    // The group's row has the shader with a blur over it on the layer above, so both ways an
    // effect is shaded (on its pixels, and on the cells a blur reads) are drawn.
    let plain = Effect::new(EffectKind::On, 0, 2000);
    let mut blurred = Effect::new(EffectKind::On, 1000, 2000);
    blurred.blur = 3;
    let mut row = Row::new(group);
    row.layers = vec![
        Layer {
            effects: vec![plain.clone()],
        },
        Layer {
            effects: vec![blurred.clone()],
        },
    ];
    let mut seq = Sequence::new("s", 2000);
    seq.rows.push(row);
    let rings = setup(RINGS, &Limits::default(), vec![("rings".into(), vec![9.0])]);
    set_shader_effects(HashMap::from([
        (plain.id.seed(), Arc::clone(&rings)),
        (blurred.id.seed(), rings),
    ]));

    take_shader_counts();
    let one = frames(&show, &seq, None, None);
    let (pixels, work, stopped) = take_shader_counts();
    assert!(pixels >= 3600 * 8, "{pixels}");
    assert_eq!(stopped, 0);
    assert!(work > pixels * 20);
    // It is a picture: lit, and not one flat color.
    let lit = one.iter().filter(|&&b| b > 0).count();
    assert!(lit > one.len() / 10, "{lit} of {}", one.len());
    assert!(one.iter().collect::<std::collections::HashSet<_>>().len() > 50);
    // The same on two threads and on eight, with the show cut into tiles of other sizes, and
    // with the shader run for other numbers of pixels at a time.
    for threads in [2, 8] {
        let pool = RenderPool::new(threads);
        assert_eq!(frames(&show, &seq, pool.clone(), None), one, "{threads} threads");
        assert_eq!(
            frames(&show, &seq, pool, Some(100)),
            one,
            "{threads} threads, small tiles"
        );
    }
    for batch in [1, 7, 64, 1000] {
        set_shader_batch(batch);
        assert_eq!(
            frames(&show, &seq, RenderPool::new(4), Some(333)),
            one,
            "{batch} at a time"
        );
    }
    set_shader_batch(pf_render::glsl::USUAL_BATCH);

    // A shader that never finishes: every pixel is stopped at its limit, and comes out dark.
    // (A low limit, to keep an unoptimized test quick.)
    let tight = Limits {
        work: 20_000,
        ..Limits::default()
    };
    let forever = setup(FOREVER, &tight, Vec::new());
    set_shader_effects(HashMap::from([(plain.id.seed(), forever)]));
    let mut alone = seq.clone();
    alone.rows[0].layers.truncate(1);
    take_shader_counts();
    let started = std::time::Instant::now();
    let dark = frames(&show, &alone, RenderPool::new(4), None);
    assert!(started.elapsed().as_secs() < 120);
    let (pixels, _, stopped) = take_shader_counts();
    assert_eq!((pixels, stopped), (3600 * 8, 3600 * 8));
    assert!(dark.iter().all(|&b| b == 0));

    // With no table, the effects are what their settings say again.
    set_shader_effects(HashMap::new());
    let on = frames(&show, &alone, None, None);
    assert!(on.iter().all(|&b| b == 255));
}
