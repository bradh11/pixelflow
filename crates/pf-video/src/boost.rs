//! Boost dim: the previews' viewing curve that lifts dim levels so fades show on a screen, for a
//! video drawn as the viewer's preview looks (`boostTable` and `boostFrame` in
//! `app/src/lib/viewOptions.ts`, number for number: the scenes in `app/src/api/contracts/pixels/`
//! hold the two to each other). Only what's drawn is lifted: the show's colors, and what goes to
//! the lights, are left as they are.

use pf_engine::PreviewProp;

/// The steps of Boost dim, after off.
pub const BOOST_STEPS: u8 = 3;

/// What Boost dim makes of a light's brightest channel at each level (0–255) at `step`. Low lifts
/// a level `x` (0–1) half way to √x, Medium to √x, High half way on from there to ∜x: full stays
/// full and off stays off. Only square roots, sums and halves, which every computer works out to
/// the same last bit, so this is the preview's table.
pub fn boost_table(step: u8) -> [u8; 256] {
    let mut table = [0u8; 256];
    for (m, out) in table.iter_mut().enumerate() {
        let x = m as f64 / 255.0;
        let root = x.sqrt();
        let lifted = match step {
            0 => x,
            1 => (x + root) / 2.0,
            2 => root,
            _ => (root + root.sqrt()) / 2.0,
        };
        *out = (lifted * 255.0 + 0.5).floor() as u8;
    }
    table
}

/// Where one prop's pixels are in a frame: from `offset`, `count` of them, `step` channels apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    offset: usize,
    step: usize,
    count: usize,
}

/// The props' pixels lifted by Boost dim at a step.
#[derive(Debug, Clone)]
pub(crate) struct Lift {
    table: [u8; 256],
    spans: Vec<Span>,
}

impl Lift {
    /// Boost dim at `step` for `props`' pixels; `None` when it's off.
    pub(crate) fn new(props: &[PreviewProp], step: u8) -> Option<Self> {
        if step == 0 {
            return None;
        }
        let spans = props
            .iter()
            // A prop whose pixels aren't three colors each has nothing to lift.
            .filter(|p| p.channels_per_pixel >= 3)
            .map(|p| Span {
                offset: p.frame_offset,
                step: usize::from(p.channels_per_pixel),
                count: p.points.len() / 2,
            })
            .collect();
        Some(Self {
            table: boost_table(step.min(BOOST_STEPS)),
            spans,
        })
    }

    /// `frame` with every prop's pixels lifted, into `out`: each keeps its hue, its brightest
    /// channel going where the table says and the others in proportion. Read from `frame` as it
    /// is, so a pixel in two props is lifted once.
    pub(crate) fn apply(&self, frame: &[u8], out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(frame);
        for span in &self.spans {
            let mut at = span.offset;
            for _ in 0..span.count {
                if at + 2 >= frame.len() {
                    break;
                }
                let [r, g, b] = [frame[at], frame[at + 1], frame[at + 2]].map(u32::from);
                let most = r.max(g).max(b);
                if most > 0 {
                    let to = u32::from(self.table[most as usize]);
                    let half = most / 2;
                    for (c, v) in [r, g, b].into_iter().enumerate() {
                        out[at + c] = ((v * to + half) / most) as u8;
                    }
                }
                at += span.step;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::PropId;

    fn prop(frame_offset: usize, count: usize, channels_per_pixel: u8) -> PreviewProp {
        PreviewProp {
            prop: PropId::new(),
            frame_offset,
            channels_per_pixel,
            points: vec![0.0; count * 2],
            flood: false,
            motion: None,
        }
    }

    #[test]
    fn the_curve_is_the_previews() {
        // The same numbers app/src/lib/viewOptions.test.ts checks.
        assert_eq!([1, 2, 3].map(|s| boost_table(s)[26]), [54, 81, 113]);
        assert_eq!([1, 2, 3].map(|s| boost_table(s)[1]), [8, 16, 40]);
        for step in 1..=BOOST_STEPS {
            let table = boost_table(step);
            assert_eq!((table[0], table[255]), (0, 255));
            assert!(table.windows(2).all(|w| w[0] <= w[1]));
        }
        assert!(
            boost_table(0)
                .iter()
                .enumerate()
                .all(|(m, &v)| usize::from(v) == m)
        );
    }

    #[test]
    fn lifts_each_pixel_keeping_its_hue_and_leaves_other_channels() {
        // A pixel, a fixture's motor channel, then an RGBW pixel with its white channel.
        let frame = [26, 13, 0, 99, 10, 10, 10, 10];
        let props = [prop(0, 1, 3), prop(4, 1, 4)];
        let mut out = Vec::new();
        Lift::new(&props, 2).unwrap().apply(&frame, &mut out);
        let ten = boost_table(2)[10];
        assert_eq!(out, [81, 41, 0, 99, ten, ten, ten, 10]);
        assert!(Lift::new(&props, 0).is_none());
    }

    #[test]
    fn stops_at_the_frames_end_and_lifts_a_shared_pixel_once() {
        let mut out = Vec::new();
        Lift::new(&[prop(0, 3, 3), prop(0, 1, 3)], 3)
            .unwrap()
            .apply(&[50, 50, 50, 9], &mut out);
        let once = boost_table(3)[50];
        assert_eq!(out, [once, once, once, 9]);
    }
}
