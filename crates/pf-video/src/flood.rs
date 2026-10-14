//! DMX floods, washed round their cells the way the 2D previews wash them (the window's
//! `app/src/lib/floodWash.ts` and `stampPixels`, from the same numbers, to the same bits: the
//! scenes in `app/src/api/contracts/pixels/` hold the two to each other). A wash reaches
//! `FLOOD_RADIUS` layout units (never less than `SMALLEST_WASH` screen pixels), and at a place r
//! of the way out it is `FLOOD_STRENGTH` × (1 - r²)² solid, in the cell's color, over what's
//! under it, under every dot.

use crate::dots::View;
use pf_engine::PreviewProp;

/// How far a flood's wash reaches, in layout units.
pub(crate) const FLOOD_RADIUS: f64 = 0.75;
/// How solid a wash is at its middle.
pub(crate) const FLOOD_STRENGTH: f64 = 0.55;
/// The smallest a wash's radius is drawn, in screen pixels.
const SMALLEST_WASH: f64 = 12.0;

/// How much of a wash shows `r2` of the way out, squared.
pub(crate) fn wash_at(r2: f64) -> f64 {
    if r2 >= 1.0 {
        return 0.0;
    }
    let t = 1.0 - r2;
    t * t
}

/// A wash's reach in picture pixels, in `view`.
pub(crate) fn wash_reach(view: &View) -> f64 {
    (FLOOD_RADIUS * view.zoom).max(SMALLEST_WASH) * view.ratio
}

/// One flood cell: where it is in the picture, and where its color is in the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Wash {
    pub x: f64,
    pub y: f64,
    pub offset: usize,
}

/// The cells of the floods among `props`, placed in `view`.
pub(crate) fn washes(props: &[PreviewProp], view: &View) -> Vec<Wash> {
    props
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
                    Wash {
                        x,
                        y,
                        offset: p.frame_offset + i * cpp,
                    }
                })
        })
        .collect()
}

/// The picture pixels a wash `reach` wide at (`x`, `y`) can touch, in a picture `width` by
/// `height`: columns `x0..x1`, rows `y0..y1` (empty when it's outside).
pub(crate) fn wash_box(
    x: f64,
    y: f64,
    reach: f64,
    width: usize,
    height: usize,
) -> (usize, usize, usize, usize) {
    let low = |v: f64| v.floor().max(0.0) as usize;
    let high = |v: f64, most: usize| (v.ceil().max(0.0) as usize).min(most);
    (
        low(x - reach),
        high(x + reach, width),
        low(y - reach),
        high(y + reach, height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wash_falls_from_full_to_nothing_and_is_never_too_small_to_see() {
        assert_eq!(wash_at(0.0), 1.0);
        assert_eq!(wash_at(0.25), 0.5625);
        assert_eq!(wash_at(1.0), 0.0);
        let view = View {
            cx: 0.0,
            cy: 0.0,
            zoom: 40.0,
            width: 100.0,
            height: 60.0,
            ratio: 2.0,
        };
        assert_eq!(wash_reach(&view), 60.0);
        assert_eq!(wash_reach(&View { zoom: 2.0, ..view }), 24.0);
        assert_eq!(wash_box(10.5, 5.0, 3.0, 12, 100), (7, 12, 2, 8));
    }
}
