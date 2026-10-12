//! A frame is the same bytes however many threads draw it.
//!
//! The renderer shares a frame out among threads (`pf-render`'s `render.rs`). These draw the
//! benchmark's sequences, and the one with a little of everything in it, on one thread and on
//! several, with the show cut into tiles of different sizes, and compare every byte of every
//! frame. Nothing here looks at a clock: how long a frame takes is the benchmark's business.

use pf_bench::sequence::{Variant, synthetic_media, synthetic_sequence};
use pf_bench::show::synthetic_show;
use pf_bench::varied::varied;
use pf_render::{RenderPool, Renderer, default_threads};
use pf_sequence::{RowBlending, Sequence};
use std::sync::Arc;

/// Small enough to draw quickly unoptimized, large enough for every kind of prop and group.
const PIXELS: u32 = 3_000;
const SECONDS: u32 = 4;

/// The frames each check draws: played from the start, then jumped about in (forward, back, and
/// back to the start), so the effects that carry on from frame to frame are started again.
fn frames() -> Vec<u64> {
    let mut frames: Vec<u64> = (0..10).collect();
    frames.extend(90..95);
    frames.extend(27..31);
    frames.extend(0..3);
    frames
}

/// What's drawn: its name, a way to make a renderer for it, and its sequence.
type Drawn = (String, Box<dyn Fn() -> Renderer + Sync>, Sequence);

/// The typical and worst-case sequences, and the varied one with its rows meeting each way.
fn everything() -> Vec<Drawn> {
    let mut all: Vec<Drawn> = Vec::new();
    let show = synthetic_show(PIXELS);
    let (map, _) = pf_mapping::map_show(&show.show);
    let media = synthetic_media(SECONDS);
    for variant in Variant::ALL {
        let sequence = synthetic_sequence(&show, variant, SECONDS).sequence;
        let (show, map, media) = (show.show.clone(), map.clone(), media.clone());
        let renderer = move || {
            let mut renderer = Renderer::new(&show, &map);
            renderer.set_audio(media.audio.clone());
            renderer.set_pictures(media.pictures.clone());
            renderer
        };
        all.push((variant.label().to_string(), Box::new(renderer), sequence));
    }
    for blending in RowBlending::ALL {
        let varied = varied(PIXELS, SECONDS, blending);
        let sequence = varied.sequence.clone();
        let name = format!("varied, rows {blending:?}");
        all.push((name, Box::new(move || varied.renderer()), sequence));
    }
    all
}

/// Every frame of `frames`, drawn in turn.
fn draw(renderer: &mut Renderer, seq: &Sequence, frames: &[u64]) -> Vec<Vec<u8>> {
    frames
        .iter()
        .map(|&index| {
            // Not black to begin with: a pixel the renderer missed would show.
            let mut frame = vec![0xA5; renderer.frame_len()];
            renderer.render_frame(seq, index, &mut frame);
            frame
        })
        .collect()
}

/// Says which frame differs, and where, rather than printing two frames.
fn assert_same(name: &str, how: &str, frames: &[u64], drawn: &[Vec<u8>], reference: &[Vec<u8>]) {
    for ((index, frame), wanted) in frames.iter().zip(drawn).zip(reference) {
        if let Some(at) = frame.iter().zip(wanted).position(|(a, b)| a != b) {
            panic!(
                "{name}, {how}: frame {index} differs from the reference, first at byte {at} ({} for {})",
                frame[at], wanted[at]
            );
        }
        assert_eq!(frame.len(), wanted.len(), "{name}, {how}: frame {index}");
    }
}

#[test]
fn frames_are_the_same_on_one_thread_and_on_many() {
    let frames = frames();
    // Two, four, eight, and as many as the engine would use on this machine.
    let mut counts = vec![2, 4, 8, default_threads().max(3)];
    counts.dedup();
    let pools: Vec<Arc<RenderPool>> = counts
        .into_iter()
        .map(|threads| RenderPool::new(threads).expect("threads to draw on"))
        .collect();
    for (name, renderer, seq) in everything() {
        let mut one = renderer();
        one.set_pool(None);
        let reference = draw(&mut one, &seq, &frames);
        assert!(
            reference.iter().any(|frame| frame.iter().any(|&b| b != 0)),
            "{name}: nothing is lit"
        );
        for pool in &pools {
            let mut many = renderer();
            many.set_pool(Some(pool.clone()));
            let drawn = draw(&mut many, &seq, &frames);
            let how = format!("{} threads", pool.threads());
            assert_same(&name, &how, &frames, &drawn, &reference);
        }
    }
}

#[test]
fn frames_are_the_same_however_the_show_is_cut_into_tiles() {
    let frames = frames();
    let pool = RenderPool::new(4).expect("threads to draw on");
    for (name, renderer, seq) in everything() {
        let mut one = renderer();
        one.set_pool(None);
        let reference = draw(&mut one, &seq, &frames);
        // A few pixels a tile, a size no prop is a multiple of, and the whole show in one.
        let cuts = [
            (3, Some(pool.clone())),
            (61, None),
            (61, Some(pool.clone())),
            (1_000_000, Some(pool.clone())),
        ];
        for (tile, pool) in cuts {
            let how = format!(
                "tiles of {tile} on {} threads",
                pool.as_ref().map_or(1, |pool| pool.threads())
            );
            let mut cut = renderer();
            cut.set_tile_pixels(tile);
            cut.set_pool(pool);
            let drawn = draw(&mut cut, &seq, &frames);
            assert_same(&name, &how, &frames, &drawn, &reference);
        }
    }
}

#[test]
fn the_same_frames_drawn_at_once_are_the_same() {
    let frames = frames();
    let pool = RenderPool::new(4).expect("threads to draw on");
    for (name, renderer, seq) in everything() {
        let mut one = renderer();
        one.set_pool(None);
        let reference = draw(&mut one, &seq, &frames);
        // Three renderers drawing the same frames at the same moment: two on one pool between
        // them, one on the pool every renderer shares unless it's told otherwise.
        let drawn: Vec<Vec<Vec<u8>>> = std::thread::scope(|scope| {
            let drawing: Vec<_> = [Some(pool.clone()), Some(pool.clone()), RenderPool::shared()]
                .into_iter()
                .map(|pool| {
                    let (renderer, seq, frames) = (&renderer, &seq, &frames);
                    scope.spawn(move || {
                        let mut renderer = renderer();
                        renderer.set_pool(pool);
                        draw(&mut renderer, seq, frames)
                    })
                })
                .collect();
            drawing
                .into_iter()
                .map(|thread| thread.join().expect("a frame was drawn"))
                .collect()
        });
        for (n, drawn) in drawn.iter().enumerate() {
            let how = format!("drawn at once ({n})");
            assert_same(&name, &how, &frames, drawn, &reference);
        }
    }
}

#[test]
fn a_frame_jumped_to_is_the_frame_played_up_to() {
    // Played from the start, then every fifth frame drawn by a renderer that has drawn nothing
    // before it: with blurs on every kind of grid among them, the same bytes.
    for (name, renderer, seq) in everything() {
        let played: Vec<u64> = (0..seq.frame_count()).collect();
        let mut one = renderer();
        let reference = draw(&mut one, &seq, &played);
        for jump in played.iter().copied().step_by(5) {
            let mut fresh = renderer();
            let drawn = draw(&mut fresh, &seq, &[jump]);
            let wanted = &reference[jump as usize..=jump as usize];
            assert_same(&name, "jumped to", &[jump], &drawn, wanted);
        }
    }
}

#[test]
fn a_renderer_draws_the_same_frames_whether_or_not_it_laid_the_show_out_first() {
    let frames = frames();
    for (name, renderer, seq) in everything() {
        let mut one = renderer();
        one.set_pool(None);
        let reference = draw(&mut one, &seq, &frames);
        let mut ahead = renderer();
        ahead.lay_out_ahead(&seq);
        let drawn = draw(&mut ahead, &seq, &frames);
        assert_same(&name, "laid out ahead", &frames, &drawn, &reference);
    }
}
