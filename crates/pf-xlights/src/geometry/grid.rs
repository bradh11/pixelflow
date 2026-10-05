//! Grid-based Boxed models: Matrix, Tree and Sphere (which share `MatrixModel`'s wiring) and
//! Cube. Ports of `MatrixModel.cpp`, `TreeModel.cpp`, `SphereModel.cpp` and `CubeModel.cpp`.

use super::xform::boxed;
use super::{Ctx, Raw, RawNode, V3, strtol0};
use std::f64::consts::PI;

/// A matrix node: channel, buffer cell of each light, and the matrix's own screen coordinates.
struct MNode {
    chan: i64,
    bufs: Vec<(i64, i64)>,
    screen: Vec<V3>,
}

struct MLayout {
    nodes: Vec<MNode>,
    /// Buffer width/height (`BufferWi`/`BufferHt`).
    bw: i64,
    bh: i64,
}

/// `MatrixModel::InitVMatrix` / `InitHMatrix` (low-definition rendering does not affect layout).
/// `first_strand` is the tree's "first strand" channel rotation (vertical only).
fn init_matrix(cx: &mut Ctx, vertical: bool, first_strand: i64) -> Option<MLayout> {
    let strings = cx.parm("NumStrings", "parm1", "1");
    let nps = cx.parm("NodesPerString", "parm2", "1");
    let sps = cx.parm("StrandsPerString", "parm3", "1");
    let alternate = cx.is("AlternateNodes", "true");
    let no_zig = cx.is("NoZig", "true");
    if strings <= 0 || nps <= 0 {
        return None;
    }
    let sps = sps.max(1).min(nps);
    let num_strands = strings * sps;
    let pps = nps / sps;
    let ppstring = pps * sps;
    let rotated = vertical && first_strand > 0 && first_strand < num_strands;
    if cx.over_cap(strings.saturating_mul(ppstring).max(strings)) {
        if !rotated {
            cx.capped_block(|cx| {
                if cx.single_node {
                    cx.strings_block(strings, cx.default_cps(1), 1)
                } else {
                    cx.strings_block(strings, cx.default_cps(nps), ppstring)
                }
            });
        }
        return None;
    }
    let cpn = cx.cpn;
    let single = cx.single_node;
    let starts = cx.string_starts(strings, cx.default_cps(if single { 1 } else { nps }), &[]);
    let (btot, ltor) = (cx.btot, cx.ltor);
    let (bw, bh) = if vertical {
        (num_strands, pps)
    } else {
        (pps, num_strands)
    };
    let interleave = |y: i64| {
        if y < (pps + 1) / 2 {
            y * 2
        } else {
            (pps - (y + 1)) * 2 + 1
        }
    };

    let mut nodes = Vec::new();
    if single {
        let ns = num_strands as f64;
        let p = pps as f64;
        let (mut outer_f, mut outer_i) = (0.0f64, 0i64);
        for n in 0..strings {
            let mut screen = Vec::with_capacity(ppstring as usize);
            let mut inner_f = 0.0f64;
            for _ in 0..ppstring {
                let a = (if btot { outer_f } else { ns - outer_f - 1.0 }) - (ns - 1.0) / 2.0;
                let b = inner_f - (p - 1.0) / 2.0 - 0.5;
                screen.push(if vertical { [a, b, 0.0] } else { [b, a, 0.0] });
                inner_f += 1.0;
                if inner_f >= p {
                    inner_f = 0.0;
                    outer_f += 1.0;
                }
            }
            let mut bufs = Vec::with_capacity(ppstring as usize);
            let (mut inner, mut step) = (0i64, 1i64);
            for _ in 0..ppstring {
                bufs.push(if vertical {
                    (if ltor { outer_i } else { num_strands - outer_i - 1 }, inner)
                } else {
                    (inner, if btot { outer_i } else { num_strands - outer_i - 1 })
                });
                inner += step;
                if inner < 0 || inner >= pps {
                    step = -step;
                    inner += step;
                    outer_i += 1;
                }
            }
            nodes.push(MNode {
                chan: starts[n as usize],
                bufs,
                screen,
            });
        }
        return Some(MLayout { nodes, bw, bh });
    }

    let mut strand_start: Vec<i64> = (0..num_strands)
        .map(|x| starts[(x / sps) as usize] + (x % sps) * pps * cpn)
        .collect();
    if rotated {
        // xLights subtracts the first strand's absolute start channel from every strand's
        // absolute start, so the model's own start channel cancels out: the tree's channels
        // count from channel 1, whatever its start channel says. Reproduced as-is.
        cx.absolute = true;
        let offset = strand_start[first_strand as usize];
        for s in strand_start.iter_mut() {
            *s -= offset;
            if *s < 0 {
                *s += pps * num_strands * cpn;
            }
        }
    }
    nodes.reserve((num_strands * pps) as usize);
    for x in 0..num_strands {
        let seg = x % sps;
        for y in 0..pps {
            let chan = strand_start[x as usize] + y * cpn;
            let (bx, by) = if vertical {
                let by = if alternate {
                    if btot {
                        interleave(y)
                    } else {
                        (pps - 1) - interleave(y)
                    }
                } else if no_zig {
                    if btot { y } else { pps - y - 1 }
                } else if btot == (seg % 2 == 0) {
                    y
                } else {
                    pps - y - 1
                };
                (if ltor { x } else { num_strands - x - 1 }, by)
            } else {
                let bx = if alternate {
                    if ltor {
                        interleave(y)
                    } else {
                        (pps - 1) - interleave(y)
                    }
                } else if no_zig {
                    if ltor { y } else { pps - y - 1 }
                } else if ltor != (seg % 2 == 0) {
                    pps - y - 1
                } else {
                    y
                };
                (bx, if btot { x } else { num_strands - x - 1 })
            };
            let screen = vec![[(bx - bw / 2) as f64, (by - bh / 2) as f64, 0.0]];
            nodes.push(MNode {
                chan,
                bufs: vec![(bx, by)],
                screen,
            });
        }
    }
    Some(MLayout { nodes, bw, bh })
}

fn into_raw(layout: MLayout, xf: super::xform::Affine) -> Raw {
    Raw {
        nodes: layout
            .nodes
            .into_iter()
            .map(|n| RawNode::new(n.chan, n.screen))
            .collect(),
        xf,
    }
}

/// `MatrixModel`.
pub(super) fn matrix(cx: &mut Ctx) -> Raw {
    let vertical = cx.m.display_as.trim() == "Vert Matrix" || cx.is("Vertical", "true");
    match init_matrix(cx, vertical, 0) {
        Some(layout) => {
            let xf = boxed(cx, 0.0, [1.0; 3]);
            into_raw(layout, xf)
        }
        None => Raw::empty(),
    }
}

/// `TreeModel::InitModel` + `SetTreeCoord`: the vertical/horizontal matrix wiring wrapped onto a
/// cone (round), a flat fan, or a ribbon, then tilted by the tree's 2D perspective.
pub(super) fn tree(cx: &mut Ctx) -> Raw {
    let vertical = cx.text("StrandDir", "Vertical") == "Vertical";
    let first_strand = (cx.int("exportFirstStrand", 0) - 1).max(0);
    let rotation = f64::from(cx.float("TreeRotation", 3.0) as f32);
    let spiral = cx.float("TreeSpiralRotations", 0.0) as f32;
    let ratio = f64::from(cx.float("TreeBottomTopRatio", 6.0) as f32);
    let perspective = f64::from(cx.float("TreePerspective", 0.2) as f32);
    let t = cx.m.display_as.trim().to_string();
    let (mut tree_type, mut degrees) = (0i64, 360i64);
    if t == "Tree" {
        tree_type = cx.int("TreeType", 0);
        degrees = cx.int("TreeDegrees", 360);
    } else if let Some(sp) = t.find(' ') {
        match &t[sp + 1..] {
            "Flat" => (tree_type, degrees) = (1, 0),
            "Ribbon" => (tree_type, degrees) = (2, -1),
            tok => degrees = strtol0(tok),
        }
    }
    let degrees = match tree_type {
        1 => 0,
        2 => -1,
        _ => degrees,
    };
    let Some(mut layout) = init_matrix(cx, vertical, first_strand) else {
        return Raw::empty();
    };
    let (bw, bh) = (layout.bw, layout.bh);
    if bw >= 1 && bh >= 1 {
        if degrees > 0 {
            let render_ht = (bh * 3) as f64;
            let render_wi = render_ht / 1.8;
            let rad = (degrees as f64).to_radians();
            let mut radius = render_wi / 2.0;
            let mut top_radius = radius;
            if ratio != 0.0 {
                top_radius = radius / ratio.abs();
            }
            if ratio < 0.0 {
                std::mem::swap(&mut top_radius, &mut radius);
            }
            let mut start_angle = -rad / 2.0;
            let mut angle_incr = rad / bw as f64;
            if degrees < 350 && bw > 1 {
                angle_incr = rad / (bw - 1) as f64;
            }
            start_angle += rotation.to_radians();
            let (y_pos, x_inc) = spiral_offsets(bh, spiral, radius as f32, top_radius as f32);
            for node in &mut layout.nodes {
                for (i, &(bx, by)) in node.bufs.iter().enumerate() {
                    let byu = by.clamp(0, bh - 1) as usize;
                    let angle = start_angle + bx as f64 * angle_incr + f64::from(x_inc[byu]);
                    let (xb, xt) = (radius * angle.sin(), top_radius * angle.sin());
                    let (zb, zt) = (radius * angle.cos(), top_radius * angle.cos());
                    let pos = if bh > 1 {
                        f64::from(y_pos[byu]) / (bh as f64 - 1.0)
                    } else {
                        0.5
                    };
                    node.screen[i] = [
                        xb + (xt - xb) * pos,
                        render_ht * pos - render_ht / 2.0,
                        zb + (zt - zb) * pos,
                    ];
                }
            }
        } else {
            let ribbon = degrees == -1;
            let tree_scale = if ribbon { 5.0 } else { 4.0 };
            let render_ht = bh as f64 * 2.0;
            let pos_of = |y: f64| if bh > 1 { y / (bh as f64 - 1.0) } else { 0.5 };
            for node in &mut layout.nodes {
                let mut screen = Vec::with_capacity(node.bufs.len() * if ribbon { 3 } else { 1 });
                let mut extra = Vec::new();
                for &(bx, by) in &node.bufs {
                    let xt = (bx as f64 + 0.5 - bw as f64 / 2.0) * 0.9;
                    let xb = (bx as f64 + 0.5 - bw as f64 / 2.0) * tree_scale;
                    if ribbon {
                        let h = (render_ht * render_ht + (xt - xb) * (xt - xb)).sqrt();
                        let at = |pos: f64| {
                            let newh = render_ht * pos;
                            [xb + (xt - xb) * pos, render_ht * newh / h - render_ht / 2.0, 0.0]
                        };
                        screen.push(at(pos_of(by as f64)));
                        let lo = if bh > 1 {
                            (by as f64 - 0.33) / (bh as f64 - 1.0)
                        } else {
                            0.0
                        };
                        let hi = if bh > 1 {
                            (by as f64 + 0.33) / (bh as f64 - 1.0)
                        } else {
                            1.0
                        };
                        extra.push(at(lo));
                        extra.push(at(hi));
                    } else {
                        let pos = pos_of(by as f64);
                        screen.push([xb + (xt - xb) * pos, render_ht * pos - render_ht / 2.0, 0.0]);
                    }
                }
                screen.extend(extra);
                node.screen = screen;
            }
        }
    }
    let xf = boxed(cx, perspective, [1.0; 3]);
    into_raw(layout, xf)
}

/// Per-row height positions and angle offsets for spiral trees (`TreeSpiralRotations`).
fn spiral_offsets(bh: i64, spiral: f32, radius: f32, top_radius: f32) -> (Vec<f32>, Vec<f32>) {
    let n = bh.max(0) as usize;
    let mut y_pos: Vec<f32> = (0..n).map(|x| x as f32).collect();
    let mut x_inc = vec![0.0f32; n];
    if spiral == 0.0 || n == 0 {
        return (y_pos, x_inc);
    }
    let bhf = bh as f32;
    let rgap = (radius - top_radius) / 10.0;
    let mut lengths = [0.0f32; 10];
    let mut total = 0.0f32;
    for (x, l) in lengths.iter_mut().enumerate() {
        *l = 2.0 * std::f32::consts::PI * (radius - rgap * x as f32) - rgap / 2.0;
        *l *= spiral / 10.0;
        *l = (*l * *l + bhf / 10.0 * bhf / 10.0).sqrt();
        total += *l;
    }
    for l in lengths.iter_mut() {
        *l /= total;
    }
    let mut cur_seg = 0usize;
    let mut in_seg = (lengths[0] * bhf).round();
    let mut cur_in_seg = 0.0f32;
    for x in 1..n {
        if cur_in_seg >= in_seg {
            cur_seg = (cur_seg + 1).min(9);
            cur_in_seg = 0.0;
            in_seg = if cur_seg == 9 {
                (bh - x as i64) as f32
            } else {
                (lengths[cur_seg] * bhf).round()
            };
        }
        if in_seg > 0.0 {
            let ang = spiral * 2.0 * std::f32::consts::PI / 10.0 / in_seg;
            y_pos[x] = y_pos[x - 1] + (f64::from(bhf) / 10.0 / f64::from(in_seg)) as f32;
            x_inc[x] = x_inc[x - 1] + ang;
        } else {
            y_pos[x] = y_pos[x - 1];
            x_inc[x] = x_inc[x - 1];
        }
        cur_in_seg += 1.0;
    }
    (y_pos, x_inc)
}

/// `SphereModel::InitModel` + `SetSphereCoord`: vertical-matrix wiring on a globe, with the
/// pre-version-8 scale preservation from `DeserializeSphere`.
pub(super) fn sphere(cx: &mut Ctx) -> Raw {
    let start_lat = cx.int("StartLatitude", -86) as f64;
    let end_lat = cx.int("EndLatitude", 86) as f64;
    let degrees = cx.int("Degrees", 360) as f64;
    let version = cx.attr("versionNumber").unwrap_or("");
    let mut scale_mul = [1.0; 3];
    if version.is_empty() || strtol0(version) < 8 {
        let nps = cx.parm("NodesPerString", "parm2", "1");
        let sps = cx.parm("StrandsPerString", "parm3", "1").max(1).min(nps);
        let pps = if sps > 0 { nps / sps } else { nps };
        let strands = cx.parm("NumStrings", "parm1", "1") * sps;
        let mx = pps.max(strands);
        if mx > 0 {
            let r = pps as f32 / mx as f32;
            scale_mul = [f64::from(r / 1.8), f64::from(r), f64::from(r / 1.8)];
        }
    }
    let Some(mut layout) = init_matrix(cx, true, 0) else {
        return Raw::empty();
    };
    let (bw, bh) = (layout.bw as f64, layout.bh as f64);
    if bw >= 1.0 && bh >= 1.0 {
        let radius = bw.max(bh) / 1.8 / 2.0;
        let remove = (360.0 - degrees).to_radians();
        let fudge = ((360.0 - degrees) / bw).to_radians();
        let h_start = 2.0 * PI / 4.0 + 0.003 - remove / 2.0;
        let h_incr = (-2.0 * PI + remove - fudge) / bw;
        let v_start = (start_lat - 90.0).to_radians();
        let v_incr = if bh > 1.0 {
            ((-start_lat).to_radians() + end_lat.to_radians()) / (bh - 1.0)
        } else {
            cx.note("one-row sphere drawn as a single ring");
            0.0
        };
        for node in &mut layout.nodes {
            for (i, &(bx, by)) in node.bufs.iter().enumerate() {
                let h = h_start + bx as f64 * h_incr;
                let v = v_start + by as f64 * v_incr;
                let sv = v.sin();
                node.screen[i] = [radius * h.cos() * sv, radius * v.cos(), radius * h.sin() * sv];
            }
        }
    }
    let xf = boxed(cx, f64::from(0.1f32), scale_mul);
    into_raw(layout, xf)
}

/// `{ xRotate, yRotate, zRotate, flipX }` per start corner and style (`CubeModel.cpp`).
const CUBE_TRANSFORMS: [[i64; 4]; 48] = [
    [1, 0, -1, 0],
    [0, 0, -1, 1],
    [0, -1, 0, 1],
    [0, 0, 0, 0],
    [-1, 2, 0, 1],
    [-1, -1, 0, 0],
    [1, 0, -1, 1],
    [0, 0, -1, 0],
    [0, -1, 0, 0],
    [0, 0, 0, 1],
    [-1, 2, 0, 0],
    [-1, -1, 0, 1],
    [1, 0, 1, 1],
    [0, 0, 1, 0],
    [0, -1, 2, 0],
    [0, 0, 2, 1],
    [-1, 2, 2, 0],
    [-1, -1, 2, 1],
    [1, 0, 1, 0],
    [0, 0, 1, 1],
    [0, -1, 2, 1],
    [0, 0, 2, 0],
    [-1, 2, 2, 1],
    [-1, -1, 2, 0],
    [-1, 0, -1, 1],
    [0, 2, 1, 0],
    [0, 1, 0, 0],
    [0, 2, 0, 1],
    [-1, 0, 0, 0],
    [-1, 1, 0, 1],
    [-1, 0, -1, 0],
    [0, 2, 1, 1],
    [0, 1, 0, 1],
    [0, 2, 0, 0],
    [-1, 0, 0, 1],
    [-1, 1, 0, 0],
    [-1, 0, 1, 0],
    [0, 2, -1, 1],
    [0, -1, 2, 0],
    [2, 0, 0, 0],
    [-1, 2, 2, 0],
    [1, -1, 0, 0],
    [-1, 0, 1, 1],
    [0, 2, -1, 0],
    [0, -1, 2, 1],
    [2, 0, 0, 1],
    [-1, 2, 2, 1],
    [1, -1, 0, 1],
];

const CUBE_STARTS: [&str; 8] = [
    "Front Bottom Left",
    "Front Bottom Right",
    "Front Top Left",
    "Front Top Right",
    "Back Bottom Left",
    "Back Bottom Right",
    "Back Top Left",
    "Back Top Right",
];

const CUBE_STYLES: [&str; 6] = [
    "Vertical Front/Back",
    "Vertical Left/Right",
    "Horizontal Front/Back",
    "Horizontal Left/Right",
    "Stacked Front/Back",
    "Stacked Left/Right",
];

const STRAND_STYLES: [&str; 3] = ["Zig Zag", "No Zig Zag", "Aternate Pixel"];

fn rotate_x90(p: &mut (i64, i64, i64), by: i64, mut h: i64, mut d: i64) {
    for _ in 0..by.abs() {
        if by > 0 {
            let t = p.1;
            p.1 = d - p.2 - 1;
            p.2 = t;
        } else {
            let t = p.2;
            p.2 = h - p.1 - 1;
            p.1 = t;
        }
        std::mem::swap(&mut h, &mut d);
    }
}

fn rotate_y90(p: &mut (i64, i64, i64), by: i64, mut w: i64, mut d: i64) {
    for _ in 0..by.abs() {
        if by > 0 {
            let t = p.2;
            p.2 = w - p.0 - 1;
            p.0 = t;
        } else {
            let t = p.0;
            p.0 = d - p.2 - 1;
            p.2 = t;
        }
        std::mem::swap(&mut w, &mut d);
    }
}

fn rotate_z90(p: &mut (i64, i64, i64), by: i64, mut w: i64, mut h: i64) {
    for _ in 0..by.abs() {
        if by > 0 {
            let t = p.0;
            p.0 = p.1;
            p.1 = w - t - 1;
        } else {
            let t = p.0;
            p.0 = h - p.1 - 1;
            p.1 = t;
        }
        std::mem::swap(&mut w, &mut h);
    }
}

/// `CubeModel::BuildCube`: the (x, y, z) cell of each node in wiring order.
fn build_cube(
    w0: i64,
    h0: i64,
    d0: i64,
    start: usize,
    style: usize,
    strand: usize,
    per_layer: bool,
) -> Vec<(i64, i64, i64)> {
    let name = CUBE_STYLES[style];
    let index = start * CUBE_STYLES.len()
        + (usize::from(name.contains("Horizontal")) << 1)
        + (usize::from(name.contains("Stacked")) << 2)
        + usize::from(name.contains("Left"));
    let [xr, yr, zr, xf] = CUBE_TRANSFORMS[index.min(CUBE_TRANSFORMS.len() - 1)];
    let (mut width, mut height, mut depth) = (w0, h0, d0);
    if zr.abs() == 1 {
        std::mem::swap(&mut width, &mut height);
    }
    if yr.abs() == 1 {
        std::mem::swap(&mut width, &mut depth);
    }
    if xr.abs() == 1 {
        std::mem::swap(&mut height, &mut depth);
    }
    let total = width * height * depth;
    (0..total)
        .map(|i| {
            let z = i / (width * height);
            let base = i % (width * height);
            let mut y = base / width;
            let mut x = if (strand == 1 || y % 2 == 0) && strand != 2 {
                base % width
            } else if strand == 2 {
                let pos = base % width + 1;
                if pos <= (width + 1) / 2 {
                    2 * (pos - 1)
                } else {
                    (width - pos) * 2 + 1
                }
            } else {
                width - base % width - 1
            };
            if !per_layer && z % 2 != 0 {
                y = height - y - 1;
                if height % 2 != 0 && strand == 0 {
                    x = width - x - 1;
                }
            }
            let mut p = (x, y, z);
            let (mut w, mut h, mut d) = (width, height, depth);
            rotate_x90(&mut p, xr, h, d);
            if xr.abs() == 1 {
                std::mem::swap(&mut h, &mut d);
            }
            rotate_y90(&mut p, yr, w, d);
            if yr.abs() == 1 {
                std::mem::swap(&mut w, &mut d);
            }
            rotate_z90(&mut p, zr, w, h);
            if zr.abs() == 1 {
                std::mem::swap(&mut w, &mut h);
            }
            if xf > 0 {
                p.0 = w - p.0 - 1;
            }
            p
        })
        .collect()
}

/// `CubeModel::InitModel`: one node per cell, contiguous channels, cube or cylinder shape.
pub(super) fn cube(cx: &mut Ctx) -> Raw {
    let w = cx.parm("CubeWidth", "parm1", "1");
    let h = cx.parm("CubeHeight", "parm2", "1");
    let d = cx.parm("CubeDepth", "parm3", "1");
    let find = |list: &[&str], v: &str| list.iter().position(|s| *s == v).unwrap_or(0);
    let start = find(&CUBE_STARTS, cx.text("Start", ""));
    let style = find(&CUBE_STYLES, cx.text("Style", ""));
    let strand = find(&STRAND_STYLES, cx.text("StrandPerLine", ""));
    let per_layer = cx.text("StrandPerLayer", "FALSE") == "TRUE";
    let cylinder = cx.int("CubeShape", 0) == 1;
    let hollow = cx.int("CubeHollow", 0).clamp(0, 99);
    let row_offset = cx.int("CubeRowOffset", 0);
    if w <= 0 || h <= 0 || d <= 0 {
        return Raw::empty();
    }
    let total = w.saturating_mul(h).saturating_mul(d);
    if cx.over_cap(total) {
        return Raw::empty();
    }
    let cpn = cx.cpn;
    let start0 = cx.string_starts(1, 0, &[])[0];
    let cells = build_cube(w, h, d, start, style, strand, per_layer);
    let two_pi = 2.0 * std::f32::consts::PI;
    let outer = w as f32 / two_pi;
    let screen = |(lx, ly, lz): (i64, i64, i64)| -> V3 {
        if cylinder {
            let ring = if d <= 1 {
                outer
            } else {
                outer * (1.0 - lz as f32 / (d - 1) as f32 * (1.0 - hollow as f32 / 100.0))
            };
            let a = two_pi * lx as f32 / w as f32;
            [
                f64::from(ring * a.cos()),
                (ly - h / 2) as f64,
                f64::from(ring * a.sin()),
            ]
        } else {
            let mut sx = (lx - w / 2) as f64;
            if lz % 2 == 1 {
                sx += match row_offset {
                    1 => 0.5,
                    2 => -0.5,
                    _ => 0.0,
                };
            }
            [sx, (ly - h / 2) as f64, (d - lz - 1 - d / 2) as f64]
        }
    };
    let nodes = if cx.single_node || cx.single_channel {
        vec![RawNode::new(start0, cells.into_iter().map(screen).collect())]
    } else {
        cells
            .into_iter()
            .enumerate()
            .map(|(n, c)| RawNode::new(start0 + n as i64 * cpn, vec![screen(c)]))
            .collect()
    };
    let xf = boxed(cx, f64::from(0.1f32), [1.0; 3]);
    Raw { nodes, xf }
}
