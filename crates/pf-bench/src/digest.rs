//! What the renderer draws, as numbers to compare.
//!
//! A change to how frames are drawn (on more threads, in another order, with something kept
//! from frame to frame) must not change a byte of them. A digest is a hash of every byte of
//! every frame of a sequence: two builds, or one build on different numbers of threads, drew the
//! same frames when their digests are the same.
//!
//! Each sequence is played from its first frame, then jumped about in: back, forward, and back to
//! the start, so the effects that carry on from frame to frame are started again from cold, as
//! scrubbing does.
//!
//! A digest belongs to the machine it was made on: effects use the system's sines and cosines,
//! which differ in their last bit from one system to another.

use crate::pipeline::RenderThreads;
use crate::sequence::{Variant, synthetic_media, synthetic_sequence};
use crate::show::{synthetic_show, tier_label};
use crate::varied::varied;
use pf_render::Renderer;
use pf_sequence::{RowBlending, Sequence};

/// Frames played after each jump.
const AFTER_A_JUMP: u64 = 12;

/// The hash of one sequence's frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    pub tier: String,
    pub sequence: String,
    pub frames: u64,
    pub hash: u64,
}

/// FNV-1a, carried on from `hash`: the same on every machine and in every build.
fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// The frames a digest draws of a sequence `total` frames long: the first `played` in order,
/// then a few after each jump.
pub fn frames_drawn(played: u64, total: u64) -> Vec<u64> {
    let played = played.min(total);
    let mut frames: Vec<u64> = (0..played).collect();
    for jump in [played / 3, total.saturating_sub(AFTER_A_JUMP), played / 2, 0] {
        frames.extend((jump..total).take(AFTER_A_JUMP as usize));
    }
    frames
}

/// Draws `frames` of `seq` in turn and hashes every byte of every frame.
pub fn hash_frames(renderer: &mut Renderer, seq: &Sequence, frames: &[u64]) -> u64 {
    let mut frame = vec![0u8; renderer.frame_len()];
    let mut hash = 0xCBF2_9CE4_8422_2325;
    for &index in frames {
        renderer.render_frame(seq, index, &mut frame);
        hash = fnv(hash, &frame);
    }
    hash
}

/// The digests of the show with `pixels` pixels: the typical and worst-case sequences, and the
/// varied one with its rows meeting each of the three ways, over each one's first `seconds`
/// seconds, drawn on `threads`.
pub fn digests(pixels: u32, seconds: u32, threads: &RenderThreads) -> Vec<Digest> {
    let length = seconds.max(1) + 2;
    let mut out = Vec::new();
    let mut add = |name: String, mut renderer: Renderer, seq: &Sequence| {
        threads.apply(&mut renderer);
        let per_second = u64::from(1000 / seq.frame_ms.max(1));
        let frames = frames_drawn(u64::from(seconds.max(1)) * per_second, seq.frame_count());
        out.push(Digest {
            tier: tier_label(pixels),
            sequence: name,
            frames: frames.len() as u64,
            hash: hash_frames(&mut renderer, seq, &frames),
        });
    };
    let show = synthetic_show(pixels);
    let (map, _) = pf_mapping::map_show(&show.show);
    let media = synthetic_media(length);
    for variant in Variant::ALL {
        let seq = synthetic_sequence(&show, variant, length).sequence;
        let mut renderer = Renderer::new(&show.show, &map);
        renderer.set_audio(media.audio.clone());
        renderer.set_pictures(media.pictures.clone());
        add(variant.label().to_string(), renderer, &seq);
    }
    for blending in RowBlending::ALL {
        let varied = varied(pixels, length, blending);
        let name = format!("varied, rows {blending:?}").to_lowercase();
        add(name, varied.renderer(), &varied.sequence);
    }
    out
}

/// The digests as a Markdown table.
pub fn markdown(digests: &[Digest]) -> String {
    let mut out = String::from("| Tier | Sequence | Frames | Digest |\n| --- | --- | --- | --- |\n");
    for digest in digests {
        out.push_str(&format!(
            "| {} | {} | {} | `{:016x}` |\n",
            digest.tier, digest.sequence, digest.frames, digest.hash
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_plays_then_jumps_back_forward_and_to_the_start() {
        let frames = frames_drawn(30, 100);
        assert_eq!(frames[..30], (0..30).collect::<Vec<u64>>());
        assert_eq!(frames[30..42], (10..22).collect::<Vec<u64>>());
        assert_eq!(frames[42..54], (88..100).collect::<Vec<u64>>());
        assert_eq!(frames[54..66], (15..27).collect::<Vec<u64>>());
        assert_eq!(frames[66..], (0..12).collect::<Vec<u64>>());
        // A sequence shorter than what's asked for is played to its end.
        assert_eq!(frames_drawn(30, 5)[..5], [0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_hash_follows_every_byte_of_every_frame() {
        assert_eq!(fnv(0xCBF2_9CE4_8422_2325, b""), 0xCBF2_9CE4_8422_2325);
        assert_eq!(fnv(0xCBF2_9CE4_8422_2325, b"a"), 0xAF63_DC4C_8601_EC8C);
        assert_ne!(fnv(fnv(1, b"ab"), b"c"), fnv(fnv(1, b"ac"), b"b"));
    }

    #[test]
    fn a_small_shows_digests_are_the_same_every_time() {
        let first = digests(1_500, 1, &RenderThreads::engines());
        assert_eq!(first.len(), 5);
        assert!(first.iter().all(|d| d.frames == 40 + 4 * AFTER_A_JUMP));
        assert_eq!(first, digests(1_500, 1, &RenderThreads::exactly(1).unwrap()));
        assert_eq!(first, digests(1_500, 1, &RenderThreads::exactly(3).unwrap()));
        // Different sequences draw different frames.
        assert!(first[..3].windows(2).all(|pair| pair[0].hash != pair[1].hash));
        assert!(markdown(&first).contains("| 1500 | varied, rows replace |"));
    }
}
