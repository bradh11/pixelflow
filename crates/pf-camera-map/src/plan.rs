//! From pixels found in the video to points in the layout, with what looks wrong.

use crate::align::{Similarity, distance};
use crate::decode::Decoded;
use serde::{Deserialize, Serialize};

/// Which prop node a sequence index lights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    /// Index into the `props` given to [`plan`].
    pub prop: usize,
    pub node: u32,
}

/// A prop being mapped, as it is in the layout now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropInput {
    pub nodes: u32,
    /// Each node's position in the front view now (layout units, +y up), in node order.
    pub expected: Vec<[f64; 2]>,
    /// Its colour order as set ("RGB", "GRBW", …).
    pub color_order: String,
}

/// The current shape moved into place: `p ↦ scale · R(rotation) · p + (tx, ty)` on its front-view
/// positions, and how far the measured pixels are from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorFit {
    pub scale: f64,
    pub rotation_deg: f64,
    pub tx: f64,
    pub ty: f64,
    /// RMS distance from the measured pixels, as a fraction of the prop's size.
    pub error: f64,
    /// Whether the shape matches the measured pixels well enough to keep it instead.
    pub fits: bool,
}

/// One prop's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropPlan {
    pub nodes: u32,
    pub found: u32,
    /// Every node's measured position in the layout (missing ones filled in between their
    /// neighbours); empty when none was found.
    pub points: Vec<[f64; 2]>,
    /// Which of `points` were seen (the rest are filled in).
    pub measured: Vec<bool>,
    pub fit: Option<GeneratorFit>,
}

/// Something about the capture worth a look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Anomaly {
    /// Nodes never seen (first and last of each run, inclusive): dead pixels, blocked from the
    /// camera, or not wired where the show says.
    Missing { prop: usize, ranges: Vec<[u32; 2]> },
    /// A second spot read as this node, usually a reflection (at `x`, `y` in the video).
    Duplicate { prop: usize, node: u32, x: f64, y: f64 },
    /// The pixels run the other way from the layout: wired from the other end.
    Reversed { prop: usize },
    /// The node is far from the one before it, unlike in the layout: wired out of order, or misread.
    Jump { prop: usize, node: u32 },
    /// Most of the prop's pixels showed the wrong colours: its colour order looks like `suggested`.
    ColorOrder {
        prop: usize,
        configured: String,
        suggested: String,
    },
    /// Lit spots that didn't read as any pixel.
    Unreadable { count: u32 },
}

/// The whole result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Video (x, −y) to layout; `None` when the layout gave nothing to line up with (points are
    /// then the video scaled to 10 units tall, centred on the origin).
    pub alignment: Option<Similarity>,
    /// How far the measured pixels are from where the layout has them, RMS in layout units, over
    /// the pixels the alignment used.
    pub alignment_error: f64,
    pub props: Vec<PropPlan>,
    pub anomalies: Vec<Anomaly>,
}

/// A prop fits its shape when its pixels are within this fraction of its size, RMS.
const FITS: f64 = 0.06;

/// Lines the found pixels up with the layout and works out each prop's points. `owners` says
/// which node each sequence index lights; `anchors` (sequence indexes) are the pixels to line up
/// by, or empty to line up by every pixel found.
pub fn plan(decoded: &Decoded, owners: &[Owner], props: &[PropInput], anchors: &[u32]) -> Plan {
    let owner = |index: u32| owners.get(index as usize).filter(|o| o.prop < props.len());
    let flip = |x: f64, y: f64| [x, -y];
    // Found positions by prop and node.
    let mut seen: Vec<Vec<Option<[f64; 2]>>> = props.iter().map(|p| vec![None; p.nodes as usize]).collect();
    for f in &decoded.pixels {
        if let Some(o) = owner(f.index)
            && let Some(slot) = seen[o.prop].get_mut(o.node as usize)
        {
            *slot = Some(flip(f.x, f.y));
        }
    }
    let expected_at = |o: &Owner| props[o.prop].expected.get(o.node as usize).copied();

    let mut anomalies = Vec::new();
    let reversed: Vec<bool> = props
        .iter()
        .zip(&seen)
        .map(|(p, s)| runs_backwards(p, s))
        .collect();
    for (prop, _) in reversed.iter().enumerate().filter(|r| *r.1) {
        anomalies.push(Anomaly::Reversed { prop });
    }

    // Line up: the anchors, or every pixel (a reversed prop by its mirror order).
    let pairs: Vec<([f64; 2], [f64; 2])> = decoded
        .pixels
        .iter()
        .filter(|f| anchors.is_empty() || anchors.contains(&f.index))
        .filter_map(|f| {
            let o = owner(f.index)?;
            let node = if reversed[o.prop] && anchors.is_empty() {
                props[o.prop].nodes.checked_sub(o.node + 1)?
            } else {
                o.node
            };
            Some((flip(f.x, f.y), expected_at(&Owner { prop: o.prop, node })?))
        })
        .collect();
    let (from, to): (Vec<[f64; 2]>, Vec<[f64; 2]>) = pairs.iter().copied().unzip();
    let alignment = if anchors.is_empty() {
        Similarity::fit_robust(&from, &to)
    } else {
        Similarity::fit(&from, &to)
    }
    .filter(|s| s.scale.is_finite() && s.scale > 1e-9);
    let to_layout = alignment.unwrap_or_else(|| {
        let scale = 10.0 / decoded.height.max(1) as f64;
        Similarity {
            scale,
            angle: 0.0,
            tx: -scale * decoded.width as f64 / 2.0,
            ty: scale * decoded.height as f64 / 2.0,
        }
    });
    let alignment_error = alignment.map_or(0.0, |a| a.rms(&from, &to));

    let mut plans = Vec::new();
    for (k, (prop, found)) in props.iter().zip(&seen).enumerate() {
        let placed: Vec<Option<[f64; 2]>> = found.iter().map(|p| p.map(|p| to_layout.apply(p))).collect();
        let count = placed.iter().flatten().count() as u32;
        let missing = runs(&placed.iter().map(Option::is_none).collect::<Vec<_>>());
        if !missing.is_empty() {
            anomalies.push(Anomaly::Missing {
                prop: k,
                ranges: missing,
            });
        }
        anomalies.extend(
            jumps(prop, &placed)
                .into_iter()
                .map(|node| Anomaly::Jump { prop: k, node }),
        );
        plans.push(PropPlan {
            nodes: prop.nodes,
            found: count,
            points: if count == 0 { Vec::new() } else { fill_in(&placed) },
            measured: placed.iter().map(Option::is_some).collect(),
            fit: generator_fit(prop, &placed),
        });
    }
    for f in &decoded.duplicates {
        if let Some(o) = owner(f.index) {
            anomalies.push(Anomaly::Duplicate {
                prop: o.prop,
                node: o.node,
                x: f.x,
                y: f.y,
            });
        }
    }
    anomalies.extend(color_order_faults(decoded, owners, props));
    if !decoded.unreadable.is_empty() {
        anomalies.push(Anomaly::Unreadable {
            count: decoded.unreadable.len() as u32,
        });
    }
    Plan {
        alignment,
        alignment_error,
        props: plans,
        anomalies,
    }
}

/// The order a prop's colours should be set to, given the colours the camera saw for red, green,
/// and blue (as `Found::seen`) while it was set to `configured`. A white channel stays put.
pub fn corrected_color_order(configured: &str, seen: [u8; 3]) -> String {
    const RGB: [char; 3] = ['R', 'G', 'B'];
    configured
        .chars()
        .map(
            |ch| match RGB.iter().position(|&c| c == ch.to_ascii_uppercase()) {
                Some(i) => RGB[usize::from(seen[i] % 3)],
                None => ch,
            },
        )
        .collect()
}

/// Whether a prop's pixels line up with its layout better run backwards: it's then upright (as a
/// camera is) the other way round, or fits far better.
fn runs_backwards(prop: &PropInput, seen: &[Option<[f64; 2]>]) -> bool {
    let n = prop.nodes as usize;
    let pairs = |back: bool| -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
        seen.iter()
            .enumerate()
            .filter_map(|(i, p)| {
                Some((
                    prop.expected.get(if back { n - 1 - i } else { i }).copied()?,
                    (*p)?,
                ))
            })
            .unzip()
    };
    let ((ef, mf), (eb, mb)) = (pairs(false), pairs(true));
    if ef.len() < 4 {
        return false;
    }
    let (Some(forward), Some(backward)) = (Similarity::fit(&ef, &mf), Similarity::fit(&eb, &mb)) else {
        return false;
    };
    let (rf, rb) = (forward.rms(&ef, &mf), backward.rms(&eb, &mb));
    let upright = |s: &Similarity| s.angle.abs() < 35f64.to_radians();
    rb < 0.5 * rf || (upright(&backward) && !upright(&forward) && rb <= 1.5 * rf + 1e-9)
}

/// Runs of `true` as inclusive [first, last] pairs.
fn runs(flags: &[bool]) -> Vec<[u32; 2]> {
    let mut out: Vec<[u32; 2]> = Vec::new();
    for (i, _) in flags.iter().enumerate().filter(|f| *f.1) {
        let i = i as u32;
        match out.last_mut() {
            Some(last) if last[1] + 1 == i => last[1] = i,
            _ => out.push([i, i]),
        }
    }
    out
}

/// Nodes much farther from the node before them than usual, where the layout has them close.
fn jumps(prop: &PropInput, placed: &[Option<[f64; 2]>]) -> Vec<u32> {
    let steps: Vec<(usize, f64, Option<f64>)> = (1..placed.len())
        .filter_map(|i| {
            let d = distance(placed[i - 1]?, placed[i]?);
            let e = prop
                .expected
                .get(i - 1)
                .zip(prop.expected.get(i))
                .map(|(a, b)| distance(*a, *b));
            Some((i, d, e))
        })
        .collect();
    if steps.len() < 4 {
        return Vec::new();
    }
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let typical = median(steps.iter().map(|s| s.1).collect());
    let typical_expected = median(
        steps
            .iter()
            .filter_map(|s| s.2)
            .collect::<Vec<_>>()
            .into_iter()
            .chain([0.0])
            .collect(),
    );
    steps
        .iter()
        .filter(|(_, d, e)| {
            let far = *d > 4.0 * typical.max(1e-9);
            let expected_far = e.is_some_and(|e| typical_expected > 1e-9 && e > 2.0 * typical_expected);
            far && !expected_far
        })
        .map(|s| s.0 as u32)
        .collect()
}

/// Every node's point: those found, and the rest in a straight line between the found ones
/// around them (continuing the nearest step past either end).
fn fill_in(placed: &[Option<[f64; 2]>]) -> Vec<[f64; 2]> {
    let known: Vec<(usize, [f64; 2])> = placed
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((i, (*p)?)))
        .collect();
    let lerp = |a: (usize, [f64; 2]), b: (usize, [f64; 2]), i: usize| {
        let t = (i as f64 - a.0 as f64) / (b.0 as f64 - a.0 as f64);
        [a.1[0] + (b.1[0] - a.1[0]) * t, a.1[1] + (b.1[1] - a.1[1]) * t]
    };
    (0..placed.len())
        .map(|i| {
            if let Some(p) = placed[i] {
                return p;
            }
            let after = known.partition_point(|k| k.0 < i);
            match (after.checked_sub(1).map(|b| known[b]), known.get(after).copied()) {
                (Some(a), Some(b)) => lerp(a, b, i),
                (Some(a), None) if known.len() >= 2 => lerp(known[known.len() - 2], a, i),
                (None, Some(b)) if known.len() >= 2 => lerp(b, known[1], i),
                (Some(a), None) => a.1,
                (None, Some(b)) => b.1,
                (None, None) => [0.0, 0.0],
            }
        })
        .collect()
}

/// The prop's current shape moved onto its measured pixels, and how well it matches.
fn generator_fit(prop: &PropInput, placed: &[Option<[f64; 2]>]) -> Option<GeneratorFit> {
    let (from, to): (Vec<[f64; 2]>, Vec<[f64; 2]>) = placed
        .iter()
        .enumerate()
        .filter_map(|(i, p)| Some((*prop.expected.get(i)?, (*p)?)))
        .unzip();
    if from.len() < 3 {
        return None;
    }
    let fit = Similarity::fit(&from, &to)?;
    let centre = to
        .iter()
        .fold([0.0, 0.0], |c, p| [c[0] + p[0], c[1] + p[1]])
        .map(|v| v / to.len() as f64);
    let size = to
        .iter()
        .map(|p| distance(*p, centre))
        .fold(0.0, f64::max)
        .max(1e-9);
    let error = fit.rms(&from, &to) / size;
    Some(GeneratorFit {
        scale: fit.scale,
        rotation_deg: fit.angle.to_degrees(),
        tx: fit.tx,
        ty: fit.ty,
        error,
        fits: error < FITS,
    })
}

/// Props where most pixels showed the same wrong colours.
fn color_order_faults(decoded: &Decoded, owners: &[Owner], props: &[PropInput]) -> Vec<Anomaly> {
    let mut tallies: Vec<Vec<([u8; 3], u32)>> = vec![Vec::new(); props.len()];
    let mut totals = vec![0u32; props.len()];
    for f in &decoded.pixels {
        let Some(o) = owners.get(f.index as usize).filter(|o| o.prop < props.len()) else {
            continue;
        };
        totals[o.prop] += 1;
        if let Some(seen) = f.seen {
            let tally = &mut tallies[o.prop];
            match tally.iter_mut().find(|t| t.0 == seen) {
                Some(t) => t.1 += 1,
                None => tally.push((seen, 1)),
            }
        }
    }
    tallies
        .iter()
        .enumerate()
        .filter_map(|(prop, tally)| {
            let &(seen, count) = tally.iter().max_by_key(|t| t.1)?;
            let suggested = corrected_color_order(&props[prop].color_order, seen);
            (seen != [0, 1, 2]
                && count >= 2
                && count * 5 >= totals[prop] * 3
                && suggested != props[prop].color_order)
                .then(|| Anomaly::ColorOrder {
                    prop,
                    configured: props[prop].color_order.clone(),
                    suggested,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{Found, Unreadable};

    fn found(index: u32, x: f64, y: f64) -> Found {
        Found {
            index,
            x,
            y,
            confidence: 1.0,
            brightness: 200.0,
            seen: Some([0, 1, 2]),
        }
    }

    /// A 10-node horizontal line in the layout from (0, 0) to (9, 0), seen in a 1000 × 500 video
    /// 50 px apart from (100, 300) rightwards.
    fn line() -> (PropInput, Vec<Owner>) {
        let prop = PropInput {
            nodes: 10,
            expected: (0..10).map(|i| [f64::from(i), 0.0]).collect(),
            color_order: "RGB".into(),
        };
        let owners = (0..10).map(|node| Owner { prop: 0, node }).collect();
        (prop, owners)
    }

    fn decoded(pixels: Vec<Found>) -> Decoded {
        Decoded {
            width: 1000,
            height: 500,
            pixels,
            ..Decoded::default()
        }
    }

    #[test]
    fn places_pixels_in_layout_units_and_fills_gaps() {
        let (prop, owners) = line();
        // Node 4 missing; the measured line bows up 1 unit (50 px) at node 7.
        let pixels = (0..10u32)
            .filter(|&i| i != 4)
            .map(|i| found(i, 100.0 + 50.0 * f64::from(i), if i == 7 { 250.0 } else { 300.0 }))
            .collect();
        let result = plan(&decoded(pixels), &owners, &[prop], &[]);
        let points = &result.props[0].points;
        assert_eq!(points.len(), 10);
        assert!(
            (points[0][0] - 0.0).abs() < 1e-6 && (points[9][0] - 9.0).abs() < 1e-6,
            "{points:?}"
        );
        assert!((points[7][1] - 1.0).abs() < 1e-6);
        assert!((points[4][0] - 4.0).abs() < 1e-6, "filled in");
        assert_eq!(result.props[0].found, 9);
        assert!(!result.props[0].measured[4]);
        assert!(result.anomalies.contains(&Anomaly::Missing {
            prop: 0,
            ranges: vec![[4, 4]]
        }));
    }

    #[test]
    fn two_anchors_line_up_by_those_pixels_only() {
        let (prop, owners) = line();
        let mut pixels: Vec<Found> = (0..10u32)
            .map(|i| found(i, 100.0 + 50.0 * f64::from(i), 300.0))
            .collect();
        // Node 5 is way off; lining up by nodes 0 and 9 keeps it there.
        pixels[5].y = 100.0;
        let result = plan(&decoded(pixels), &owners, &[prop], &[0, 9]);
        let points = &result.props[0].points;
        assert!((points[9][0] - 9.0).abs() < 1e-6);
        assert!((points[5][1] - 4.0).abs() < 1e-6, "{:?}", points[5]);
        assert!(
            result
                .anomalies
                .iter()
                .any(|a| matches!(a, Anomaly::Jump { prop: 0, node: 5 }))
        );
    }

    #[test]
    fn a_reversed_string_is_flagged_and_still_lines_up_upright() {
        let (prop, owners) = line();
        let pixels = (0..10u32)
            .map(|i| found(i, 550.0 - 50.0 * f64::from(i), 300.0))
            .collect();
        let result = plan(&decoded(pixels), &owners, &[prop], &[]);
        assert!(result.anomalies.contains(&Anomaly::Reversed { prop: 0 }));
        let a = result.alignment.unwrap();
        assert!(a.angle.abs() < 1e-6, "{a:?}");
        // Node 0 is where the camera saw it: at the right end.
        assert!((result.props[0].points[0][0] - 9.0).abs() < 1e-6);
    }

    #[test]
    fn duplicates_unreadable_and_colour_order() {
        let (prop, owners) = line();
        let mut pixels: Vec<Found> = (0..10u32)
            .map(|i| found(i, 100.0 + 50.0 * f64::from(i), 300.0))
            .collect();
        for p in &mut pixels[..8] {
            p.seen = Some([1, 0, 2]);
        }
        let mut d = decoded(pixels);
        d.duplicates.push(found(3, 250.0, 450.0));
        d.unreadable.push(Unreadable {
            x: 1.0,
            y: 1.0,
            brightness: 20.0,
        });
        let result = plan(&d, &owners, &[prop], &[]);
        assert!(result.anomalies.contains(&Anomaly::Duplicate {
            prop: 0,
            node: 3,
            x: 250.0,
            y: 450.0
        }));
        assert!(result.anomalies.contains(&Anomaly::Unreadable { count: 1 }));
        assert!(result.anomalies.contains(&Anomaly::ColorOrder {
            prop: 0,
            configured: "RGB".into(),
            suggested: "GRB".into()
        }));
    }

    #[test]
    fn corrected_order_accounts_for_the_order_set_now() {
        assert_eq!(corrected_color_order("RGB", [1, 0, 2]), "GRB");
        assert_eq!(corrected_color_order("GRB", [1, 0, 2]), "RGB");
        assert_eq!(corrected_color_order("RGBW", [2, 1, 0]), "BGRW");
        assert_eq!(corrected_color_order("BRG", [0, 1, 2]), "BRG");
    }

    #[test]
    fn a_matching_shape_fits_and_a_bent_one_does_not() {
        let (prop, owners) = line();
        let straight = (0..10u32)
            .map(|i| found(i, 100.0 + 50.0 * f64::from(i), 300.0))
            .collect();
        let result = plan(&decoded(straight), &owners, std::slice::from_ref(&prop), &[]);
        assert!(result.props[0].fit.unwrap().fits);
        let bent = (0..10u32)
            .map(|i| {
                found(
                    i,
                    100.0 + 50.0 * f64::from(i),
                    300.0 - 40.0 * f64::from(i.saturating_sub(5)),
                )
            })
            .collect();
        // Anchored on the straight half, the bent half stays bent.
        let result = plan(&decoded(bent), &owners, &[prop], &[0, 4]);
        assert!(!result.props[0].fit.unwrap().fits);
    }

    #[test]
    fn nothing_to_line_up_with_falls_back_to_the_video_frame() {
        let prop = PropInput {
            nodes: 3,
            expected: vec![[0.0, 0.0]; 3],
            color_order: "RGB".into(),
        };
        let owners: Vec<Owner> = (0..3).map(|node| Owner { prop: 0, node }).collect();
        let result = plan(
            &decoded(vec![found(0, 500.0, 250.0), found(2, 600.0, 250.0)]),
            &owners,
            &[prop],
            &[],
        );
        assert!(result.alignment.is_none());
        let points = &result.props[0].points;
        assert!(points[0][0].abs() < 1e-9 && points[0][1].abs() < 1e-9);
        assert!((points[2][0] - 2.0).abs() < 1e-9);
        assert!((points[1][0] - 1.0).abs() < 1e-9);
    }
}
