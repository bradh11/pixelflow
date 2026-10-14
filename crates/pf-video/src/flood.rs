//! DMX floods, drawn the way the 2D previews draw them (the window's `app/src/lib/floodWash.ts`,
//! from the same numbers): each cell of a fixture lights a soft round wash of its colour around
//! it, added onto what's under it, under the dots. The wash is `FLOOD_STRENGTH` of its colour at
//! the middle, falling as (1 - r²)² to nothing at `FLOOD_RADIUS` layout units (r: the share of
//! the radius out). A wash is never smaller than `SMALLEST` screen pixels across.

use crate::dots::View;
use pf_engine::PreviewProp;

/// How far a flood's wash reaches, in layout units.
pub(crate) const FLOOD_RADIUS: f64 = 0.75;
/// How strong a wash is at its middle, as a share of its colour.
pub(crate) const FLOOD_STRENGTH: f64 = 0.55;
/// The smallest a wash's radius is drawn, in screen pixels.
const SMALLEST: f64 = 12.0;

/// How much of a wash shows at `r` of its radius out.
pub(crate) fn wash_at(r: f64) -> f64 {
    if r >= 1.0 { 0.0 } else { (1.0 - r * r).powi(2) }
}

/// One flood cell: where it is in the picture, and where its colour is in the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cell {
    x: f64,
    y: f64,
    offset: usize,
}

/// The floods of a scene, placed once.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Floods {
    cells: Vec<Cell>,
    /// A wash's radius, in picture pixels.
    radius: f64,
}

impl Floods {
    /// The flood cells among `props`, or `None` when there are none.
    pub(crate) fn new(props: &[PreviewProp], view: &View) -> Option<Self> {
        let cells: Vec<Cell> = props
            .iter()
            .filter(|p| p.flood)
            .flat_map(|p| {
                let cpp = usize::from(p.channels_per_pixel);
                p.points
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .enumerate()
                    .map(move |(i, xy)| {
                        let (x, y) = view.place(f64::from(xy[0]), f64::from(xy[1]));
                        Cell {
                            x,
                            y,
                            offset: p.frame_offset + i * cpp,
                        }
                    })
            })
            .collect();
        let radius = (FLOOD_RADIUS * view.zoom).max(SMALLEST) * view.ratio;
        (!cells.is_empty()).then_some(Self { cells, radius })
    }

    /// Adds each lit cell's wash onto `rgb` (a picture `width` by `height`, three channels a pixel).
    pub(crate) fn add(&self, frame: &[u8], width: usize, height: usize, rgb: &mut [u16]) {
        let r = self.radius;
        for cell in &self.cells {
            let Some(color) = frame.get(cell.offset..cell.offset + 3) else {
                continue;
            };
            if color.iter().all(|&c| c == 0) {
                continue;
            }
            let span = |c: f64, max: usize| {
                let low = (c - r).floor().max(0.0) as usize;
                let high = ((c + r).ceil().max(0.0) as usize).min(max);
                low..high
            };
            for y in span(cell.y, height) {
                for x in span(cell.x, width) {
                    let (dx, dy) = (x as f64 + 0.5 - cell.x, y as f64 + 0.5 - cell.y);
                    let level = FLOOD_STRENGTH * wash_at((dx * dx + dy * dy).sqrt() / r);
                    if level <= 0.0 {
                        continue;
                    }
                    let at = (y * width + x) * 3;
                    for (out, &c) in rgb[at..at + 3].iter_mut().zip(color) {
                        *out = out.saturating_add((f64::from(c) * level).round() as u16);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View {
            cx: 0.0,
            cy: 0.0,
            zoom: 40.0,
            width: 200.0,
            height: 100.0,
            ratio: 1.0,
        }
    }

    fn flood(points: &[f32]) -> PreviewProp {
        PreviewProp {
            prop: pf_model::PropId::new(),
            frame_offset: 0,
            channels_per_pixel: 3,
            points: points.to_vec(),
            flood: true,
        }
    }

    #[test]
    fn a_lit_flood_washes_its_colour_round_it_fading_to_nothing() {
        let floods = Floods::new(&[flood(&[0.0, 0.0])], &view()).unwrap();
        assert_eq!(floods.radius, 30.0, "0.75 units at 40 pixels a unit");
        let mut rgb = vec![0u16; 200 * 100 * 3];
        floods.add(&[200, 100, 0], 200, 100, &mut rgb);
        let at = |x: usize, y: usize| [0, 1, 2].map(|c| rgb[(y * 200 + x) * 3 + c]);
        let middle = at(100, 50);
        assert!(middle[0] > 100 && middle[0] <= 110, "{middle:?}");
        assert_eq!(middle[2], 0);
        let halfway = at(115, 50);
        assert!(halfway[0] > 30 && halfway[0] < middle[0], "{halfway:?}");
        assert_eq!(at(131, 50), [0, 0, 0], "nothing past the radius");
        // Dark, it washes nothing.
        let mut dark = vec![0u16; 200 * 100 * 3];
        floods.add(&[0, 0, 0], 200, 100, &mut dark);
        assert!(dark.iter().all(|&v| v == 0));
    }

    #[test]
    fn only_floods_wash_and_a_wash_is_never_too_small_to_see() {
        let mut plain = flood(&[0.0, 0.0]);
        plain.flood = false;
        assert!(Floods::new(&[plain], &view()).is_none());
        let far = View { zoom: 2.0, ..view() };
        assert_eq!(Floods::new(&[flood(&[0.0, 0.0])], &far).unwrap().radius, SMALLEST);
        assert_eq!(wash_at(0.0), 1.0);
        assert_eq!(wash_at(1.0), 0.0);
    }
}
