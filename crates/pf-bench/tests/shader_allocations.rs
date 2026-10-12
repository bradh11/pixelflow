//! Drawing a Shader effect allocates nothing once it has been drawn: the program is compiled
//! once, each thread keeps the machines it runs shaders on, and a frame's inputs are set up in
//! place.
//!
//! Counted with the benchmark's allocator, which only a program can install, so this is a test
//! program of its own with a single test: nothing else is allocating beside it.

use pf_bench::alloc::{Counting, counted};
use pf_model::{Corner, Generator, MatrixWiring, Orientation, Prop, ShapeSource, Show};
use pf_render::Renderer;
use pf_sequence::{Blend, Effect, EffectKind, EffectParams, Layer, Row, Sequence, ShaderParams, Target};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn drawing_shaders_allocates_nothing() {
    let matrix = Prop::new(
        "Matrix",
        ShapeSource::Generator(Generator::Matrix {
            columns: 64,
            rows: 32,
            width: 2.0,
            height: 1.0,
            wiring: MatrixWiring {
                start: Corner::BottomLeft,
                orientation: Orientation::Horizontal,
                serpentine: true,
            },
        }),
    );
    let mut show = Show::new("t");
    show.props.push(matrix);
    let mut seq = Sequence::new("s", 10_000);
    seq.frame_ms = 25;
    let mut row = Row::new(Target::Prop(show.props[0].id));
    // Every one of PixelFlow's own shaders at once, one above another, with an input set
    // (which those that have no input of that name pass over).
    row.layers = pf_render::built_in_shaders()
        .into_iter()
        .enumerate()
        .map(|(n, file)| {
            let mut effect =
                Effect::new(EffectKind::Shader, 0, 10_000).with_params(EffectParams::Shader(ShaderParams {
                    file,
                    inputs: serde_json::from_str(r#"{ "speed": 2.0 }"#).unwrap(),
                    ..ShaderParams::default()
                }));
            if n > 0 {
                effect.blend = Blend::Max;
            }
            Layer {
                effects: vec![effect],
            }
        })
        .collect();
    seq.rows.push(row);
    let mut renderer = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    // On this thread alone: threads of a pool allocate on their own account now and then.
    renderer.set_pool(None);
    let mut frame = vec![0u8; renderer.frame_len()];
    for index in 0..3 {
        renderer.render_frame(&seq, index, &mut frame);
    }
    assert!(
        frame.iter().filter(|&&b| b > 0).count() > 2000,
        "the shaders draw"
    );

    let before = counted().expect("the counting allocator is installed");
    for index in 3..63 {
        renderer.render_frame(&seq, index, &mut frame);
    }
    let after = counted().expect("the counting allocator is installed");
    assert_eq!(after.allocations - before.allocations, 0);
    assert_eq!(after.bytes - before.bytes, 0);
}
