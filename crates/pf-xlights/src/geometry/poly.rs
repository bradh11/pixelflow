//! Poly-point models: Poly Line and MultiPoint. Ports of `PolyLineModel.cpp` and
//! `MultiPointModel.cpp`, curved Poly Line stretches included (`BezierCurveCubic3D`).

use super::xform::{Affine, poly_point, quirky_bounds, rot_from_x_axis};
use super::{
    Ctx, MAX_LIGHTS, Raw, RawNode, V3, compute_string_start_node, nodes_per_string_of, strtod, strtol0,
};
use std::collections::HashMap;

/// `PolyPointScreenLocation::SetDataFromString`: `x,y,z` triples padded with zeros.
pub(crate) fn parse_points(s: &str, n: usize) -> Vec<V3> {
    let mut v: Vec<f64> = s
        .split(',')
        .map(|t| {
            let f = f64::from(strtod(t).unwrap_or(0.0) as f32);
            if f.is_finite() { f } else { 0.0 }
        })
        .collect();
    v.resize(v.len().max(n * 3), 0.0);
    (0..n).map(|i| [v[i * 3], v[i * 3 + 1], v[i * 3 + 2]]).collect()
}

/// Normalizes a coordinate over the quirky bounds; `min_delta` collapses near-flat axes.
fn norm(v: f64, lo: f64, hi: f64, min_delta: f64) -> f64 {
    let d = hi - lo;
    if d.abs() < min_delta || d == 0.0 {
        0.0
    } else {
        (v - lo) / d
    }
}

struct PNode {
    chan: i64,
    buf0: (i64, i64),
    pts: Vec<V3>,
}

/// A straight piece in normalized space: `point(t) = p1 + R * (t * |a|, 0, 0)`, and its length
/// in the model's own point space.
struct Piece {
    m: Affine,
    len: f64,
}

impl Piece {
    fn at(&self, t: f64) -> V3 {
        self.m.apply([t, 0.0, 0.0])
    }
}

/// One stretch between two points: a single straight piece, or a curve's pieces.
struct Seg {
    pieces: Vec<Piece>,
    curved: bool,
}

/// The joints of a curved stretch as `BezierCurveCubic3D::UpdatePoints` samples it: steps of
/// 1/25 added up in 32-bit floats while under 1 (de Casteljau), then the end point.
pub(crate) fn curve_joints(p0: V3, c0: V3, c1: V3, p1: V3) -> Vec<V3> {
    let f = |v: V3| v.map(|x| x as f32);
    let (p0, c0, c1, p1) = (f(p0), f(c0), f(c1), f(p1));
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t);
    let mut joints = Vec::with_capacity(27);
    let mut i = 0.0f32;
    while i < 1.0 {
        let (a, b, c) = (lerp(p0, c0, i), lerp(c0, c1, i), lerp(c1, p1, i));
        let p = lerp(lerp(a, b, i), lerp(b, c, i), i);
        joints.push(p.map(f64::from));
        i += 1.0 / 25.0;
    }
    joints.push(p1.map(f64::from));
    joints
}

/// Curved stretches from `cPointData` (seven fields each: the stretch, then two control points),
/// for stretches that exist, in the model's point space.
pub(crate) fn parse_curves(s: &str, nseg: usize) -> HashMap<usize, (V3, V3)> {
    let fields: Vec<&str> = s.split(',').collect();
    let num = |t: &str| f64::from(strtod(t).unwrap_or(0.0) as f32);
    fields
        .as_chunks::<7>()
        .0
        .iter()
        .filter_map(|c| {
            let seg = usize::try_from(strtol0(c[0])).ok().filter(|&i| i < nseg)?;
            Some((
                seg,
                (
                    [num(c[1]), num(c[2]), num(c[3])],
                    [num(c[4]), num(c[5]), num(c[6])],
                ),
            ))
        })
        .collect()
}

/// `PolyLineModel::InitModel` with `DistributeLightsEvenly` / `DistributeLightsAcrossIndivSegments`.
pub(super) fn poly_line(cx: &mut Ctx) -> Raw {
    let lpn = cx.parm("LightsPerNode", "parm3", "1").max(1);
    let total_attr = cx.parm("NodesPerString", "parm2", "0");
    let num_points = cx.int("NumPoints", 2).max(2);
    if cx.over_cap(num_points) {
        return Raw::empty();
    }
    let np = num_points as usize;
    let pts = parse_points(cx.text("PointData", "0.0, 0.0, 0.0, 0.0, 0.0, 0.0"), np);
    let nseg = np - 1;
    let curves: HashMap<usize, Vec<V3>> = parse_curves(cx.text("cPointData", ""), nseg)
        .into_iter()
        .map(|(i, (c0, c1))| (i, curve_joints(pts[i], c0, c1, pts[i + 1])))
        .collect();
    let strings = cx.int("PolyStrings", 1).max(1);
    let drops: Vec<i64> = cx
        .text("DropPattern", "1")
        .split(',')
        .map(|t| match strtol0(t) {
            0 => 1,
            d => d,
        })
        .collect();
    let max_h = drops.iter().map(|d| d.abs()).max().unwrap_or(1);
    let alternate = cx.is("AlternateNodes", "true");
    let model_height = f64::from(cx.float("ModelHeight", 1.0) as f32);
    let indiv: Vec<i64> = if strings > 1 && cx.attr("PolyNode1").is_some() {
        (0..strings.min(MAX_LIGHTS))
            .map(|i| cx.int(&format!("PolyNode{}", i + 1), 0))
            .collect()
    } else {
        Vec::new()
    };
    let auto = cx.attr("Seg1").is_none();
    let mut sizes: Vec<i64> = if auto {
        vec![50; nseg]
    } else {
        (0..nseg)
            .map(|i| cx.int(&format!("Seg{}", i + 1), 0).max(0))
            .collect()
    };
    let corner = |i: usize| cx.text(&format!("Corner{}", i + 1), "Neither");
    let mut lead = vec![0.5f32; nseg];
    let mut trail = vec![0.5f32; nseg];
    for x in 0..=nseg {
        let c = corner(x);
        let l = match c {
            "Leading Segment" => 1.0,
            "Trailing Segment" => 0.0,
            _ => 0.5,
        };
        let t = match c {
            "Leading Segment" => 0.0,
            "Trailing Segment" => 1.0,
            _ => 0.5,
        };
        if x < nseg {
            lead[x] = l;
        }
        if x > 0 {
            trail[x - 1] = t;
        }
    }

    // Light and drop-position counts.
    let max_drop = drops.iter().map(|d| d.abs()).max().unwrap_or(1);
    let est = if auto {
        total_attr
    } else {
        sizes.iter().sum::<i64>().saturating_mul(max_drop)
    };
    if cx.over_cap(est.saturating_mul(lpn)) {
        return Raw::empty();
    }
    let dl = drops.len();
    let mut di = 0usize;
    let mut num_lights = 0i64;
    let mut drop_points = 0i64;
    if !auto {
        for &size in &sizes {
            for _ in 0..size {
                num_lights += drops[di].abs();
                di = (di + 1) % dl;
            }
            drop_points += size;
        }
    } else {
        let mut left = total_attr;
        while left > 0 {
            let d = drops[di].abs();
            di = (di + 1) % dl;
            num_lights += d;
            drop_points += 1;
            left -= d;
        }
    }
    if num_lights == 0 {
        return Raw::empty();
    }
    let single = cx.single_node;
    let cpn = cx.cpn;
    let coords = if single { num_lights } else { lpn };
    let node_count = if single { 1 } else { num_lights };

    // String start channels (steady state, after xLights' second channel pass).
    let total_nodes = {
        let mut t = 0;
        let mut d = 0usize;
        for _ in 0..drop_points {
            t += drops[d].abs();
            d = (d + 1) % dl;
        }
        t
    };
    let nps_all = if strings <= 1 {
        num_lights
    } else if auto {
        num_lights / strings
    } else {
        total_nodes / strings
    };
    let nps_of = |s: i64| -> i64 {
        if strings <= 1 {
            return nps_all;
        }
        if let Some(&start) = indiv.get(s as usize) {
            let end = if s == strings - 1 {
                total_nodes + 1
            } else {
                indiv.get(s as usize + 1).copied().unwrap_or(0)
            };
            return end - start;
        }
        nps_all
    };
    let mut cps = cx.default_cps(nps_all);
    if strings != 1 {
        cps /= strings;
    }
    let starts = cx.string_starts(strings, cps, &indiv);
    let mut ssn: Vec<(i64, usize)> = Vec::new();
    let mut next_ssn = num_lights;
    let mut sorted_idx = 0usize;
    if strings > 1 {
        ssn = (0..starts.len())
            .map(|i| {
                let s1 = match indiv.get(i) {
                    Some(&n) => n,
                    None => compute_string_start_node(i as i64, strings, node_count),
                };
                (s1 - 1, i)
            })
            .collect();
        ssn.sort();
        sorted_idx = 1;
        next_ssn = ssn.get(1).map_or(num_lights, |s| s.0);
    }

    let mut nodes: Vec<PNode> = (0..node_count)
        .map(|_| PNode {
            chan: 0,
            buf0: (0, 0),
            pts: vec![[0.0; 3]; coords as usize],
        })
        .collect();
    let mut left = num_lights * if single { 1 } else { lpn };
    let (mut y, mut width) = (0i64, 0i64);
    let (mut cur_node, mut cur_coord) = (0usize, 0i64);
    di = 0;
    let mut up = drops[0] < 0;
    let mut in_drop = drops[0].abs();
    let mut in_drop_last = in_drop;
    let mut chan = 0i64;
    let mut first = true;
    while left > 0 {
        if cur_coord >= coords {
            cur_node += 1;
            cur_coord = 0;
            if !single {
                chan += cpn;
            }
        }
        while y >= drops[di].abs() {
            width += 1;
            y = 0;
            di = (di + 1) % dl;
            in_drop = drops[di].abs();
            if !cx.ltor && !single && cur_coord == 0 {
                chan -= (in_drop_last + in_drop) * cpn;
            }
            in_drop_last = in_drop;
            up = drops[di] < 0;
        }
        if strings > 1 && cur_node as i64 >= next_ssn && sorted_idx < ssn.len() {
            let s = ssn[sorted_idx].1;
            chan = starts[s];
            if !cx.ltor && !single && cur_coord == 0 {
                chan += (nps_of(s as i64) - in_drop) * cpn;
            }
            sorted_idx += 1;
            next_ssn = ssn.get(sorted_idx).map_or(num_lights, |s| s.0);
        } else if first {
            first = false;
            let s = if strings > 1 { ssn[0].1 } else { 0 };
            chan = starts[s];
            if !cx.ltor && !single && cur_coord == 0 {
                chan += (nps_of(s as i64) - in_drop) * cpn;
            }
        }
        let by = if alternate {
            let k = if y < (in_drop + 1) / 2 {
                2 * y
            } else {
                (in_drop - (y + 1)) * 2 + 1
            };
            if up { k } else { max_h - 1 - k }
        } else if up {
            y
        } else {
            max_h - y - 1
        };
        let node = &mut nodes[cur_node];
        node.chan = chan;
        if cur_coord == 0 {
            node.buf0 = (if single { 0 } else { width }, by);
        }
        left -= 1;
        cur_coord += 1;
        if single || cur_coord == lpn {
            y += 1;
        }
    }

    // Segments in normalized point space; the bounds take in the curves' joints too.
    let all: Vec<V3> = pts.iter().chain(curves.values().flatten()).copied().collect();
    let (lo, hi) = quirky_bounds(&all);
    // Straight stretches drop a nearly flat height; curves only an exactly flat one.
    let n = |p: V3, flat: f64| {
        [
            norm(p[0], lo[0], hi[0], 0.0),
            norm(p[1], lo[1], hi[1], flat),
            norm(p[2], lo[2], hi[2], 0.0),
        ]
    };
    let raw_len = |a: V3, b: V3| {
        let w = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        f64::from(((w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt()) as f32)
    };
    let piece = |p1: V3, p2: V3, y_scale: f64| {
        let a = [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]];
        let scale = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        Affine::translate(p1)
            .then(&rot_from_x_axis(a))
            .then(&Affine::scale([scale, y_scale, 0.0]))
    };
    let segs: Vec<Seg> = (0..nseg)
        .map(|i| match curves.get(&i) {
            Some(joints) => Seg {
                pieces: joints
                    .windows(2)
                    .map(|w| Piece {
                        m: piece(n(w[0], 0.0), n(w[1], 0.0), 1.0),
                        len: raw_len(w[0], w[1]),
                    })
                    .collect(),
                curved: true,
            },
            None => Seg {
                pieces: vec![Piece {
                    m: piece(n(pts[i], 0.1), n(pts[i + 1], 0.1), 0.0),
                    len: raw_len(pts[i], pts[i + 1]),
                }],
                curved: false,
            },
        })
        .collect();
    let model_h = (hi[1] - lo[1]).max(max_h as f64);
    let mheight = f64::from((model_height * 10.0 / model_h) as f32);
    let at: HashMap<(i64, i64), usize> = {
        let mut m = HashMap::new();
        for (i, nd) in nodes.iter().enumerate() {
            m.entry(nd.buf0).or_insert(i);
        }
        m
    };
    let mut place = Placer {
        nodes: &mut nodes,
        at: &at,
        single,
        max_h,
        mheight,
        guard_hit: false,
    };
    if auto {
        place.evenly(&segs, &drops, num_lights, drop_points, &mut sizes);
    } else {
        place.per_segment(&segs, &drops, &sizes, &lead, &trail);
    }
    if place.guard_hit {
        cx.note("some poly line lights could not be placed exactly");
    }
    let xf = poly_point(cx, &all, max_h as f64);
    Raw {
        nodes: nodes.into_iter().map(|n| RawNode::new(n.chan, n.pts)).collect(),
        xf,
    }
}

/// Light placement along the segments (shared state of the two distribution functions).
struct Placer<'a> {
    nodes: &'a mut [PNode],
    at: &'a HashMap<(i64, i64), usize>,
    single: bool,
    max_h: i64,
    mheight: f64,
    guard_hit: bool,
}

impl Placer<'_> {
    fn coords_per_node(&self) -> usize {
        self.nodes[0].pts.len()
    }

    /// Sets the coordinate(s) for drop light `z` at `v`; returns how many lights were placed.
    fn put(&mut self, v: V3, z: i64, up: bool, xpos: i64, c: &mut usize, find_up: bool) -> i64 {
        let icicles = self.max_h > 1;
        let cpnode = self.coords_per_node();
        let offset = if icicles { 1.0 / cpnode as f64 } else { 0.0 };
        let y_at = |c: usize| {
            let d = (z as f64 + c as f64 * offset) * self.mheight;
            if up { v[1] + d } else { v[1] - d }
        };
        if self.single {
            let cc = *c;
            if let Some(p) = self.nodes[0].pts.get_mut(cc) {
                *p = [v[0], y_at(cc), v[2]];
            }
            *c += 1;
            return 1;
        }
        let key = if find_up && up {
            (xpos, z)
        } else {
            (xpos, self.max_h - z - 1)
        };
        let Some(&node) = self.at.get(&key) else {
            self.guard_hit = true;
            return 0;
        };
        let range = if icicles { 0..cpnode } else { *c..*c + 1 };
        let mut placed = 0;
        for cc in range.clone() {
            let y = y_at(cc);
            if let Some(p) = self.nodes[node].pts.get_mut(cc) {
                *p = [v[0], y, v[2]];
            }
            placed += 1;
        }
        *c = range.end;
        placed
    }

    /// `DistributeLightsEvenly` (auto-distribute).
    fn evenly(&mut self, segs: &[Seg], drops: &[i64], num_lights: i64, drop_points: i64, sizes: &mut [i64]) {
        let icicles = self.max_h > 1;
        let cpnode = self.coords_per_node();
        let to_place = if self.single {
            num_lights
        } else {
            num_lights * cpnode as i64
        };
        let total_len: f64 = segs.iter().flat_map(|s| &s.pieces).map(|p| p.len).sum();
        let divisor = drop_points as f64
            * if !self.single && !icicles {
                cpnode as f64
            } else {
                1.0
            };
        let offset = if divisor > 0.0 { total_len / divisor } else { 0.0 };
        let mut cur = offset / 2.0;
        let (mut segment, mut piece) = (0usize, 0usize);
        let mut seg_start = cur;
        let mut seg_len = segs[0].pieces[0].len;
        let mut seg_end = seg_start + seg_len;
        let (mut c, mut xpos, mut di) = (0usize, 0i64, 0usize);
        let (mut drop_pos, mut last_drop_pos) = (0i64, 0i64);
        sizes.iter_mut().for_each(|s| *s = 0);
        let mut m = 0i64;
        let mut guard = 0i64;
        while m < to_place {
            guard += 1;
            if guard > to_place * 2 + 16 {
                self.guard_hit = true;
                break;
            }
            while cur > seg_end {
                if piece + 1 < segs[segment].pieces.len() {
                    piece += 1;
                    seg_start = seg_end;
                    seg_len = segs[segment].pieces[piece].len;
                    seg_end = seg_start + seg_len;
                } else if segment == segs.len() - 1 {
                    seg_end = cur;
                } else {
                    sizes[segment] = drop_pos - last_drop_pos;
                    last_drop_pos = drop_pos;
                    segment += 1;
                    piece = 0;
                    seg_start = seg_end;
                    seg_len = segs[segment].pieces[0].len;
                    seg_end = seg_start + seg_len;
                }
            }
            let pos = if seg_len > 0.0 {
                (cur - seg_start) / seg_len
            } else {
                0.0
            };
            let v = segs[segment].pieces[piece].at(pos);
            let up = drops[di] < 0;
            let count = drops[di].abs();
            di += 1;
            for z in 0..count {
                m += self.put(v, z, up, xpos, &mut c, true);
                if !self.single && c == cpnode {
                    c = 0;
                }
            }
            di %= drops.len();
            cur += offset;
            if c == 0 {
                xpos += 1;
            }
            drop_pos += 1;
        }
        if let Some(s) = sizes.get_mut(segment) {
            *s = drop_pos - last_drop_pos;
        }
    }

    /// `DistributeLightsAcrossIndivSegments` (explicit `SegN` light counts).
    fn per_segment(&mut self, segs: &[Seg], drops: &[i64], sizes: &[i64], lead: &[f32], trail: &[f32]) {
        let icicles = self.max_h > 1;
        let cpnode = self.coords_per_node();
        let (mut di, mut idx, mut xpos) = (0usize, 0usize, 0i64);
        for (segment, seg) in segs.iter().enumerate() {
            let size = sizes[segment];
            let mut lights = 0i64;
            let mut d = di;
            for _ in 0..size {
                lights += drops[d].abs();
                d = (d + 1) % drops.len();
            }
            let to_place = if self.single {
                lights
            } else {
                lights * cpnode as i64
            };
            // A curve is walked by its length, a straight stretch by its pixels.
            let total_length = if seg.curved {
                seg.pieces.iter().map(|p| p.len as f32).sum()
            } else {
                size as f32
            };
            let (ld, tr) = (lead[segment], trail[segment]);
            let gaps = if icicles {
                ld + tr + size as f32 - 1.0
            } else {
                ld + tr + to_place as f32 - 1.0
            };
            let offset = if gaps > 0.0 { total_length / gaps } else { 0.0 };
            let mut cur = ld * offset;
            let (mut piece, mut piece_start) = (0usize, 0.0f32);
            let mut c = 0usize;
            let mut m = 0i64;
            let mut guard = 0i64;
            while m < to_place {
                guard += 1;
                if guard > to_place * 2 + 16 {
                    self.guard_hit = true;
                    break;
                }
                let up = drops[di] < 0;
                let count = drops[di].abs();
                let v = if seg.curved {
                    // `DistributeLightsAcrossSegment`: on to the next piece while past this one.
                    let len = |k: usize| seg.pieces.get(k).map_or(0.0, |p| p.len as f32);
                    while cur > piece_start + len(piece) && len(piece + 1) > 0.0 {
                        piece_start += len(piece);
                        piece += 1;
                    }
                    let t = if len(piece) > 0.0 {
                        (cur - piece_start) / len(piece)
                    } else {
                        0.0
                    };
                    seg.pieces[piece].at(f64::from(t))
                } else {
                    let t = if size > 0 {
                        f64::from(cur) / size as f64
                    } else {
                        0.0
                    };
                    seg.pieces[0].at(t)
                };
                for z in 0..count {
                    if self.single {
                        // xLights indexes lights by `idx` here but its drop offset uses c (= 0).
                        let d = z as f64 * self.mheight;
                        if let Some(p) = self.nodes[0].pts.get_mut(idx) {
                            *p = [v[0], if up { v[1] + d } else { v[1] - d }, v[2]];
                        }
                        idx += 1;
                        m += 1;
                    } else {
                        m += self.put(v, z, up, xpos, &mut c, false);
                        if c == cpnode {
                            c = 0;
                        }
                    }
                }
                di = (di + 1) % drops.len();
                cur += offset;
                if c == 0 {
                    xpos += 1;
                }
            }
        }
    }
}

/// `MultiPointModel::InitLine` + `InitModel`: one node per point.
pub(super) fn multi_point(cx: &mut Ctx) -> Raw {
    let num_points = cx.int("NumPoints", 2).max(1);
    if cx.over_cap(num_points) {
        return Raw::empty();
    }
    let np = num_points as usize;
    let pts = parse_points(cx.text("PointData", "0.0, 0.0, 0.0, 0.0, 0.0, 0.0"), np);
    let strings = cx.int("MultiStrings", 1).max(1);
    let indiv: Vec<i64> = if strings > 1 && cx.attr("MultiNode1").is_some() {
        (0..strings.min(MAX_LIGHTS))
            .map(|i| cx.int(&format!("MultiNode{}", i + 1), 0))
            .collect()
    } else {
        Vec::new()
    };
    let single = cx.single_node;
    let cpn = cx.cpn;
    let starts = cx.string_starts(strings, cx.default_cps(1), &indiv);
    let start0 = starts[0];
    let node_count = if single { 1 } else { num_points };
    let (lo, hi) = quirky_bounds(&pts);
    let normed: Vec<V3> = pts
        .iter()
        .map(|p| {
            [
                norm(p[0], lo[0], hi[0], 0.0),
                norm(p[1], lo[1], hi[1], 0.0),
                norm(p[2], lo[2], hi[2], 0.0),
            ]
        })
        .collect();
    let mut chans: Vec<i64> = if cx.ltor {
        (0..node_count).map(|i| start0 + i * cpn).collect()
    } else {
        let nps = nodes_per_string_of(0, strings, 1, node_count, single, &indiv);
        (0..node_count)
            .map(|i| start0 + nps * cpn - cpn - i * cpn)
            .collect()
    };
    if chans.iter().any(|&c| c < 0) {
        cx.note("reversed MultiPoint numbering laid out last point first");
        chans = (0..node_count)
            .map(|i| start0 + (node_count - 1 - i) * cpn)
            .collect();
    }
    let nodes: Vec<RawNode> = if node_count > 1 || single && np > 1 {
        (0..node_count as usize)
            .map(|i| {
                let lights = if single { np } else { 1 };
                RawNode::new(chans[i], vec![normed[i]; lights])
            })
            .collect()
    } else {
        vec![RawNode::new(chans[0], vec![[0.5, 0.0, 0.0]])]
    };
    let xf = if np >= 2 {
        poly_point(cx, &pts, 1.0)
    } else {
        cx.note("single-point MultiPoint placed at its point");
        let p = pts[0];
        let s = |k: &str| {
            let v = cx.float(k, 1.0);
            if v <= 0.0 { 1.0 } else { v }
        };
        let w = cx.world_pos();
        Affine::translate([
            p[0] * s("ScaleX") + w[0] - 0.5,
            p[1] * s("ScaleY") + w[1],
            p[2] * s("ScaleZ") + w[2],
        ])
    };
    Raw { nodes, xf }
}
