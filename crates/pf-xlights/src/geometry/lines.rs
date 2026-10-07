//! Line-like models: Single Line, Channel Block (two-point), Arches, Candy Canes and Icicles
//! (three-point). Ports of the matching `src-core/models/*Model.cpp` generators.

use super::xform::{three_point, two_point};
use super::{Ctx, Raw, RawNode, layer_sizes, nodes_per_string_of, strtol0};
use std::collections::HashMap;
use std::f64::consts::PI;

/// `SingleLineModel::InitLine` + `InitModel`.
pub(super) fn single_line(cx: &mut Ctx) -> Raw {
    let s = cx.parm("NumStrings", "parm1", "1").max(0);
    let n = cx.parm("NodesPerString", "parm2", "50").max(0);
    let lpn = cx.parm("LightsPerNode", "parm3", "1").max(0);
    let lights = s.saturating_mul(n).saturating_mul(lpn.max(1));
    if cx.over_cap(lights.max(s)) {
        cx.capped_block(|cx| {
            let nps = if cx.single_node { 1 } else { n };
            cx.strings_block(s, cx.default_cps(nps), nps)
        });
        return Raw::empty();
    }
    if s == 0 || n == 0 {
        return Raw::empty();
    }
    let single = cx.single_node;
    let nps = if single { 1 } else { n };
    let starts = cx.string_starts(s, cx.default_cps(nps), &[]);
    let cpn = cx.cpn;
    let node_count = if single { s } else { s * n };
    let coords = if single { n } else { lpn };
    let buffer_wi = if single { s } else { s * n };

    let mut nodes = Vec::with_capacity(node_count as usize);
    let mut last = -1;
    let mut chan = 0;
    let incr = if cx.ltor { cpn } else { -cpn };
    for i in 0..node_count {
        let string = if single { i } else { i / n };
        if string != last {
            last = string;
            chan = starts[string as usize];
            if !cx.ltor {
                chan += nodes_per_string_of(string, s, nps, node_count, single, &[]) * cpn + incr;
            }
        }
        nodes.push(RawNode::new(chan, vec![[0.0; 3]; coords as usize]));
        chan += incr;
    }
    if buffer_wi > 1 || coords > 1 {
        let light_count = (buffer_wi * coords) as f64;
        let offset = buffer_wi as f64 / (light_count - 1.0);
        let mut x = 0.0;
        for p in nodes.iter_mut().flat_map(|n| n.pts.iter_mut()) {
            *p = [x, 0.0, 0.0];
            x += offset;
        }
    } else if let Some(p) = nodes[0].pts.first_mut() {
        *p = [0.5, 0.0, 0.0];
    }
    let xf = two_point(cx, buffer_wi as f64);
    Raw { nodes, xf }
}

/// `ChannelBlockModel`: one single-channel node per channel, spread along the line.
pub(super) fn channel_block(cx: &mut Ctx) -> Raw {
    cx.cpn = 1;
    let n = cx.parm("NumChannels", "parm1", "1").max(0);
    if cx.over_cap(n) {
        cx.capped_block(|_| n);
        return Raw::empty();
    }
    if n == 0 {
        return Raw::empty();
    }
    let starts = cx.string_starts(n, 1, &[]);
    let nodes = (0..n)
        .map(|i| RawNode::new(starts[i as usize], vec![[i as f64 + 0.5, 0.0, 0.0]]))
        .collect();
    let xf = two_point(cx, n as f64);
    Raw { nodes, xf }
}

/// `rotate_point` from the arch/cane generators: rotate `(x, y)` about `(cx, cy)`.
fn rotate_point(cx: f64, cy: f64, angle: f64, x: f64, y: f64) -> (f64, f64) {
    let (s, c) = angle.sin_cos();
    let (dx, dy) = (x - cx, y - cy);
    (dx * c - dy * s + cx, dx * s + dy * c + cy)
}

/// The three-point skew angle (`Angle`, overridden by a type-specific skew attribute).
fn skew_degrees(cx: &Ctx, skew_attr: &str) -> i64 {
    if cx.attr(skew_attr).is_some() {
        cx.int(skew_attr, 0)
    } else {
        cx.int("Angle", 0)
    }
}

/// `ArchesModel::InitModel` with `SetArchCoord` / `SetLayerdArchCoord`.
pub(super) fn arches(cx: &mut Ctx) -> Raw {
    let arches = cx.parm("NumArches", "parm1", "1").max(0);
    let npa = cx.parm("NodesPerArch", "parm2", "1").max(0);
    let lpn = cx.parm("LightsPerNode", "parm3", "1").max(0);
    let zigzag = cx.is("ZigZag", "true");
    let hollow = cx.int("Hollow", 70);
    let gap = cx.int("Gap", 0);
    let arc = cx.attr("Arc").or_else(|| cx.attr("arc")).map_or(180, strtol0);
    let layers = layer_sizes(cx.text("LayerSizes", ""));
    let height = cx.float("Height", 1.0);
    let skew = (skew_degrees(cx, "ArchesSkew") as f64).to_radians();
    let lights = arches.saturating_mul(npa).saturating_mul(lpn.max(1));
    if cx.over_cap(lights.max(arches).max(npa.saturating_mul(lpn.max(1)))) {
        if layers.is_empty() {
            cx.capped_block(|cx| cx.strings_block(arches, cx.cpn.saturating_mul(npa), npa));
        }
        return Raw::empty();
    }
    if npa == 0 {
        return Raw::empty();
    }
    let cpn = cx.cpn;
    let starts = cx.string_starts(arches, cpn * npa, &[]);
    let coords = if cx.single_node { lpn } else { lpn.max(1) };
    let total = (arc as f64).to_radians();
    let start = (PI - total) / 2.0;

    if layers.is_empty() {
        if arches == 0 {
            return Raw::empty();
        }
        let l = (npa * lpn) as f64;
        let midpt = (l - 1.0) / 2.0;
        let x0 = midpt * (-PI / 2.0 + start).sin() * 2.0 + l;
        let width = l * 2.0 - x0;
        let mut nodes = Vec::with_capacity((arches * npa) as usize);
        let mut ys = Vec::new();
        let mut gaps = 0;
        for y in 0..arches {
            for x in 0..npa {
                let chan = if cx.ltor {
                    starts[y as usize] + x * cpn
                } else {
                    starts[(arches - y - 1) as usize] + (npa - x - 1) * cpn
                };
                let mut pts = Vec::with_capacity(coords as usize);
                for c in 0..coords {
                    let ang = arch_angle(start, total, (x * lpn + c) as f64, midpt);
                    let px = y as f64 * width + midpt * ang.sin() * 2.0 + l + (gaps * gap) as f64;
                    let py = l * ang.cos();
                    ys.push(py);
                    let (rx, ry) = rotate_point(px, 0.0, skew, px, py * height);
                    pts.push([rx, ry, 0.0]);
                }
                nodes.push(RawNode::new(chan, pts));
                if (y * npa + x + 1) % npa == 0 {
                    gaps += 1;
                }
            }
        }
        shift_down(&mut nodes, &ys);
        if midpt == 0.0 {
            cx.note("single-light arches are drawn at their apex");
        }
        let rw = width * arches as f64 + ((arches - 1) * gap) as f64;
        let xf = three_point(cx, rw, true, false);
        return Raw { nodes, xf };
    }

    // Layered arch: concentric arches on one string.
    let lcount = layers.len() as i64;
    let max_len = layers.iter().copied().max().unwrap_or(1);
    let start0 = starts.first().copied().unwrap_or(0);
    let node_count = npa;
    let mut bufs: Vec<Option<(i64, i64)>> = vec![None; node_count as usize];
    let mut idx = 0i64;
    let mut dir = cx.ltor;
    for layer in 0..lcount {
        let yy = if cx.btot { lcount - layer - 1 } else { layer };
        let it = layers[yy as usize];
        if idx >= node_count {
            continue;
        }
        if it == 1 {
            bufs[idx as usize] = Some((max_len / 2, yy));
            idx += 1;
        } else {
            let g = (max_len - 1) as f32 / (it - 1) as f32;
            for x in 0..it {
                // Past the last pixel, the rest of the layer changes nothing.
                if idx >= node_count {
                    break;
                }
                let mut xx = (x as f32 * g).round() as i64;
                if !dir {
                    xx = max_len - 1 - xx;
                }
                bufs[idx as usize] = Some((xx, yy));
                idx += 1;
            }
        }
        if zigzag {
            dir = !dir;
        }
    }
    if bufs.iter().any(Option::is_none) {
        cx.note("arch nodes beyond the layer sizes are drawn at the arch start");
    }
    let ml = (max_len * lpn) as f64;
    let midpt = (ml - 1.0) / 2.0;
    let x0 = midpt * (-PI / 2.0 + start).sin() * 2.0 + ml;
    let width = ml * 2.0 - x0;
    let archgap = if lcount > 1 {
        (1.0 - hollow as f64 / 100.0) / (lcount - 1) as f64
    } else {
        0.0
    };
    let mut nodes = Vec::with_capacity(node_count as usize);
    let mut ys = Vec::new();
    for (i, b) in bufs.iter().enumerate() {
        let (bx, by) = b.unwrap_or((0, 0));
        let adj = 1.0 - archgap * (lcount - 1 - by) as f64;
        let mut pts = Vec::with_capacity(coords as usize);
        for c in 0..coords {
            let ang = arch_angle(start, total, (bx * lpn + c) as f64, midpt);
            let px = midpt * ang.sin() * 2.0 * adj + ml;
            let py = ml * ang.cos();
            ys.push(py);
            let (rx, ry) = rotate_point(px, 0.0, skew, px, py * height * adj);
            pts.push([rx, ry, 0.0]);
        }
        nodes.push(RawNode::new(start0 + i as i64 * cpn, pts));
    }
    shift_down(&mut nodes, &ys);
    let xf = three_point(cx, width, true, false);
    Raw { nodes, xf }
}

/// Angle of light `i` along an arch; a one-light arch (xLights divides 0 by 0) sits at the apex.
fn arch_angle(start: f64, total: f64, i: f64, midpt: f64) -> f64 {
    if midpt == 0.0 {
        0.0
    } else {
        -PI / 2.0 + start + total * i / midpt / 2.0
    }
}

/// Arches whose lowest light is above 1 are shifted down by it (unscaled y, as xLights does).
fn shift_down(nodes: &mut [RawNode], ys: &[f64]) {
    let min_y = ys.iter().copied().fold(999999.0, f64::min);
    if min_y > 1.0 {
        for p in nodes.iter_mut().flat_map(|n| n.pts.iter_mut()) {
            p[1] -= min_y;
        }
    }
}

/// `CandyCaneModel::InitModel` + `SetCaneCoord`.
pub(super) fn candy_canes(cx: &mut Ctx) -> Raw {
    let canes = cx.parm("NumCanes", "parm1", "1").max(0);
    let mut npc = cx.parm("NodesPerCane", "parm2", "1").max(0);
    let mut lpn = cx.parm("LightsPerNode", "parm3", "1").max(0);
    let reverse = cx.is("CandyCaneReverse", "true");
    let sticks = cx.is("CandyCaneSticks", "true");
    let alternate = cx.is("AlternateNodes", "true");
    let cane_height = cx.float("CandyCaneHeight", 1.0);
    let mh = cx.float("Height", 1.0);
    let angle = (skew_degrees(cx, "CandyCaneSkew") as f64).to_radians();
    let lights = canes.saturating_mul(npc).saturating_mul(lpn.max(1));
    if cx.over_cap(lights.max(canes)) || canes == 0 {
        return Raw::empty();
    }
    let single = cx.single_node;
    let cpn = cx.cpn;
    let cps = if single { cpn } else { cpn * npc };
    let mut starts = cx.string_starts(canes, cps, &[]);

    let mut segs = npc;
    if single && npc <= 1 && lpn > 1 {
        segs = lpn;
        npc = lpn;
    }
    // Nodes: single -> one node per cane holding `segs` lights; else canes*segs nodes.
    let coords_per_node = if single { segs } else { lpn.max(1) };
    if single {
        segs = 1;
        lpn = npc;
        npc = 1;
    }
    if !cx.ltor {
        starts.reverse();
    }
    let mut nodes = Vec::with_capacity((canes * segs) as usize);
    let mut at: HashMap<(i64, i64), usize> = HashMap::new();
    for y in 0..canes {
        for x in 0..segs {
            let buf_y = if alternate {
                if x < (segs + 1) / 2 {
                    2 * x
                } else {
                    (segs - (x + 1)) * 2 + 1
                }
            } else {
                x
            };
            at.entry((y, buf_y)).or_insert(nodes.len());
            nodes.push(RawNode::new(
                starts[y as usize] + x * cpn,
                vec![[0.0; 3]; coords_per_node as usize],
            ));
        }
    }

    // SetCaneCoord, with the (possibly swapped) per-cane counts.
    let segments = npc;
    let lights_per_cane = segments * lpn;
    let mut upright = (segments as f64 * 6.0 / 9.0) as i64 * lpn;
    if single {
        upright = (lpn as f64 * 6.0 / 9.0) as i64;
    }
    let width_per_cane = lights_per_cane as f64 * 3.0 / 9.0;
    let cane_gap = 2.0;
    let width = canes as f64 * width_per_cane + (canes - 1) as f64 * cane_gap;
    let find = |i: i64, y: i64| at.get(&(i, y)).copied();
    let mut set = |node: Option<usize>, c: usize, x: f64, y: f64, ox: f64| {
        if let Some(p) = node.and_then(|n| nodes[n].pts.get_mut(c)) {
            let (rx, ry) = rotate_point(ox, 0.0, angle, x, y);
            *p = [rx, ry, 0.0];
        }
    };
    let ccount = coords_per_node as usize;
    if sticks {
        for i in 0..canes {
            let mut y = 0i64;
            let x = i as f64 * (width_per_cane + cane_gap) + width_per_cane / 2.0;
            for n in 0..segments {
                let node = if single {
                    Some((n + i * segments) as usize)
                } else {
                    find(i, y / lpn.max(1))
                };
                for c in 0..ccount {
                    set(node, c, x, cane_height * y as f64 * mh, x);
                    y += 1;
                }
            }
        }
    } else {
        let arc_lights = lights_per_cane - upright;
        for i in 0..canes {
            let mut x = i as f64 * (width_per_cane + cane_gap);
            if reverse {
                x += width_per_cane;
            }
            let mut y = 0.0f64;
            let mut cur_light = 0i64;
            let mut cur_node = 0i64;
            let cxp = if reverse {
                x - width_per_cane / 2.0 * mh
            } else {
                x + width_per_cane / 2.0 * mh
            };
            let ox = x;
            while cur_light < upright {
                if single {
                    let node = Some((cur_node + i * segments) as usize);
                    for c in 0..upright as usize {
                        set(node, c, x, cane_height * y * mh, x);
                        y += 1.0;
                        cur_light += 1;
                    }
                } else {
                    let node = find(i, (y / lpn as f64) as i64);
                    for c in 0..ccount {
                        set(node, c, x, cane_height * y * mh, x);
                        y += 1.0;
                        cur_light += 1;
                    }
                    cur_node += 1;
                }
            }
            y -= 1.0;
            x = cxp;
            while cur_light < lights_per_cane {
                let (node, c0, cn) = if single {
                    (
                        Some((cur_node + i * segments) as usize),
                        cur_light as usize,
                        lights_per_cane as usize,
                    )
                } else {
                    (find(i, cur_light / lpn.max(1)), 0, ccount)
                };
                for c in c0..cn {
                    let a = PI - PI * (cur_light - upright + 1) as f64 / arc_lights as f64;
                    let y2 = a.sin() * width_per_cane / 2.0 * mh;
                    let x2 = a.cos() * width_per_cane / 2.0 * mh;
                    let px = if reverse { x - x2 } else { x + x2 };
                    set(node, c, px, cane_height * (y * mh + y2), ox);
                    cur_light += 1;
                }
                cur_node += 1;
            }
        }
    }
    let xf = three_point(cx, width, true, false);
    Raw { nodes, xf }
}

/// `IciclesModel::InitModel`: strings side by side, lights filling drops of the drop pattern.
pub(super) fn icicles(cx: &mut Ctx) -> Raw {
    let strings = cx.parm("NumStrings", "parm1", "1").max(0);
    let lps = cx.parm("NodesPerString", "parm2", "1").max(0);
    let alternate = cx.is("AlternateNodes", "true");
    let mut drops: Vec<i64> = cx
        .text("DropPattern", "3,4,5,4")
        .split(',')
        .map(strtol0)
        .filter(|&d| d >= 0)
        .collect();
    if drops.is_empty() {
        drops.push(5);
    }
    if drops.iter().all(|&d| d == 0) {
        cx.note("icicle drop pattern has no lights; drops of 5 assumed");
        drops = vec![5];
    }
    let lights = strings.saturating_mul(lps);
    if cx.over_cap(lights.max(strings)) || strings == 0 {
        return Raw::empty();
    }
    let single = cx.single_node;
    let cpn = cx.cpn;
    let start0 = cx.string_starts(strings, cx.default_cps(if single { 1 } else { lps }), &[])[0];
    let (node_count, coords) = if single { (strings, lps) } else { (lights, 1) };
    let mut nodes: Vec<RawNode> = (0..node_count)
        .map(|i| RawNode::new(start0 + i * cpn, vec![[0.0; 3]; coords as usize]))
        .collect();

    let mut width = -1i64;
    let (mut cur_node, mut cur_coord) = (0usize, 0usize);
    for _ in 0..strings {
        let mut left = lps;
        let mut y = 0i64;
        let mut cur_drop = 0usize;
        let mut in_drop = drops[0];
        width += 1;
        while left > 0 {
            if cur_coord >= nodes[cur_node].pts.len() {
                cur_node += 1;
                cur_coord = 0;
            }
            while y >= drops[cur_drop] {
                width += 1;
                y = 0;
                cur_drop = (cur_drop + 1) % drops.len();
                in_drop = drops[cur_drop];
            }
            let sy = if alternate {
                if y < (in_drop + 1) / 2 {
                    2 * y
                } else {
                    (in_drop - (y + 1)) * 2 + 1
                }
            } else {
                y
            };
            nodes[cur_node].pts[cur_coord] = [width as f64, sy as f64, 0.0];
            left -= 1;
            y += 1;
            cur_coord += 1;
        }
    }
    if !cx.ltor {
        for p in nodes.iter_mut().flat_map(|n| n.pts.iter_mut()) {
            p[0] = width as f64 - p[0];
        }
    }
    if width == 0 {
        for p in nodes.iter_mut().flat_map(|n| n.pts.iter_mut()) {
            p[0] = 0.5;
        }
        width += 1;
    }
    let xf = three_point(cx, width as f64, false, true);
    Raw { nodes, xf }
}
