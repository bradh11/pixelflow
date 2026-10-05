//! Round and framed Boxed models: Circle, Wreath, Star, Spinner and Window Frame. Ports of the
//! matching `src-core/models/*Model.cpp` generators.

use super::xform::boxed;
use super::{Ctx, Raw, RawNode, layer_sizes, set_layer, trunc_i};
use std::f64::consts::PI;

/// Node/coordinate layout shared by models built with `SetNodeCount(strings, nodesPerString)`:
/// dumb strings are one node per string holding all its lights.
fn string_nodes(cx: &Ctx, strings: i64, nps: i64) -> (i64, i64) {
    if cx.single_node {
        (strings, nps)
    } else {
        (strings * nps, 1)
    }
}

/// String number of node `i` (`Node::StringNum` from `SetNodeCount`).
fn string_of(cx: &Ctx, i: i64, nps: i64) -> i64 {
    if cx.single_node { i } else { i / nps.max(1) }
}

/// `CircleModel::InitCircle` + `SetCircleCoord`: concentric rings, layer 0 outermost.
pub(super) fn circle(cx: &mut Ctx) -> Raw {
    let strings = cx.parm("NumStrings", "parm1", "1").max(0);
    let nps = cx.parm("NodesPerString", "parm2", "1").max(0);
    let center_pct = cx.parm("centerPercent", "parm3", "0");
    let inside_out = cx.is("InsideOut", "1");
    let mut layers = match cx.attr("circleSizes").filter(|s| !s.is_empty()) {
        Some(s) => {
            let mut v: Vec<&str> = s.split(',').collect();
            v.reverse();
            layer_sizes(&v.join(","))
        }
        None => layer_sizes(cx.text("LayerSizes", "")),
    };
    if cx.attr("StartSide").is_none() {
        cx.btot = false;
    }
    let num_lights = strings.saturating_mul(nps);
    if cx.over_cap(num_lights.max(strings)) || num_lights == 0 {
        return Raw::empty();
    }
    if layers.is_empty() {
        layers.push(1);
    }
    if layers.len() == 1 {
        set_layer(&mut layers, 0, num_lights);
    }
    let mut cnt = 0;
    let mut max_lights = 0;
    for x in 0..layers.len() {
        if cnt + layers[x] > num_lights {
            let v = if cnt > num_lights { 0 } else { num_lights - cnt };
            set_layer(&mut layers, x, v);
        }
        cnt += layers[x];
        max_lights = max_lights.max(layers[x]);
    }
    let (node_count, coords) = string_nodes(cx, strings, nps);
    let starts = cx.string_starts(strings, cx.default_cps(if cx.single_node { 1 } else { nps }), &[]);
    let lc = layers.len() as i64;
    let (single, cpn, btot, ltor) = (cx.single_node, cx.cpn, cx.btot, cx.ltor);
    let strand_len = |circle: i64| {
        if single {
            1
        } else {
            layers[(lc - circle - 1) as usize]
        }
    };
    let max_radius = max_lights as f64 / 2.0;
    let min_radius = center_pct as f64 / 100.0 * max_radius;

    let mut nodes = Vec::with_capacity(node_count as usize);
    let mut last = -1;
    let mut chan = 0;
    let mut push = |nodes: &mut Vec<RawNode>, pts: Vec<[f64; 3]>| {
        let i = nodes.len() as i64;
        let s = if single { i } else { i / nps };
        if s != last {
            last = s;
            chan = starts[s as usize];
        }
        nodes.push(RawNode::new(chan, pts));
        chan += cpn;
    };
    let mut to_map = node_count;
    for circle in 0..lc {
        let loop_count = to_map.min(strand_len(circle));
        let radius = if lc == 1 {
            max_radius
        } else {
            let l = if inside_out { lc - circle - 1 } else { circle };
            min_radius + (max_radius - min_radius) * (1.0 - l as f64 / (lc - 1) as f64)
        };
        for n in 0..loop_count {
            let pts = (0..coords)
                .map(|c| {
                    let frac = if loop_count == 1 {
                        c as f64 / coords as f64
                    } else {
                        n as f64 / loop_count as f64
                    };
                    let mut a = (if btot { -PI } else { 0.0 }) + PI * frac * 2.0;
                    if !ltor {
                        a = -a;
                    }
                    [a.sin() * radius, a.cos() * radius, 0.0]
                })
                .collect();
            push(&mut nodes, pts);
        }
        to_map -= loop_count;
    }
    if to_map > 0 {
        while (nodes.len() as i64) < node_count {
            push(&mut nodes, vec![[0.0; 3]; coords as usize]);
        }
        cx.note("circle nodes not covered by the layer sizes are drawn at the center");
    }
    let xf = boxed(cx, 0.0, [1.0; 3]);
    Raw { nodes, xf }
}

/// `WreathModel::InitWreath`: lights on an integer grid around a circle.
pub(super) fn wreath(cx: &mut Ctx) -> Raw {
    let strings = cx.parm("NumStrings", "parm1", "1").max(0);
    let nps = cx.parm("NodesPerString", "parm2", "50").max(0);
    let num_lights = strings.saturating_mul(nps);
    if cx.over_cap(num_lights.max(strings)) || num_lights == 0 {
        return Raw::empty();
    }
    let (node_count, coords) = string_nodes(cx, strings, nps);
    let starts = cx.string_starts(strings, cx.default_cps(if cx.single_node { 1 } else { nps }), &[]);
    let buffer = num_lights + 1;
    let offset = num_lights / 2;
    let r = offset as f64;
    let mut pct = if cx.btot { 0.5 } else { 0.0 };
    let mut incr = 1.0 / num_lights as f64;
    if cx.ltor != cx.btot {
        incr = -incr;
    }
    let half = buffer / 2;
    let mut nodes = Vec::with_capacity(node_count as usize);
    let mut last = -1;
    let mut chan = 0;
    for i in 0..node_count {
        let s = string_of(cx, i, nps);
        if s != last {
            last = s;
            chan = starts[s as usize];
        }
        let mut pts = Vec::with_capacity(coords as usize);
        for _ in 0..coords {
            let a = pct * 2.0 * PI;
            let x = trunc_i(r * a.sin() + offset as f64 + 0.5);
            let y = trunc_i(r * a.cos() + offset as f64 + 0.5);
            pts.push([(x - half) as f64, (y - half) as f64, 0.0]);
            pct += incr;
            if pct >= 1.0 {
                pct -= 1.0;
            }
            if pct < 0.0 {
                pct += 1.0;
            }
        }
        nodes.push(RawNode::new(chan, pts));
        chan += cx.cpn;
    }
    let xf = boxed(cx, 0.0, [1.0; 3]);
    Raw { nodes, xf }
}

/// `StarModel::ConvertFromDirStartSide`.
fn star_start_from_dir(cx: &Ctx) -> &'static str {
    let dir = cx.text("Dir", "L");
    let side = cx.text("StartSide", "B");
    match (dir == "L", side == "B") {
        (true, true) => "Bottom Ctr-CW",
        (true, false) => "Top Ctr-CCW",
        (false, true) => "Bottom Ctr-CCW",
        (false, false) => "Top Ctr-CW",
    }
}

/// `StarModel::InitModel`: lights evenly spaced along each layer's star outline.
pub(super) fn star(cx: &mut Ctx) -> Raw {
    let strings = cx.parm("NumStrings", "parm1", "1").max(0);
    let nps = cx.parm("NodesPerString", "parm2", "1").max(0);
    let mut points = cx.parm("StarPoints", "parm3", "5").max(2);
    if points > 10_000 {
        cx.note("star point count limited to 10000");
        points = 10_000;
    }
    let mut layers = match cx.attr("starSizes") {
        Some(s) => layer_sizes(s),
        None => layer_sizes(cx.text("LayerSizes", "")),
    };
    let start_loc = match cx.attr("StarStartLocation").filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => star_start_from_dir(cx).to_string(),
    };
    let mut ratio = f64::from(cx.float("starRatio", 2.618034) as f32);
    let mut inner_pct = cx.int("starCenterPercent", -1);
    let num_lights = strings.saturating_mul(nps);
    if cx.over_cap(num_lights.max(strings)) || num_lights == 0 {
        return Raw::empty();
    }
    let (node_count, coords) = string_nodes(cx, strings, nps);
    let starts = cx.string_starts(strings, cx.default_cps(if cx.single_node { 1 } else { nps }), &[]);
    if layers.is_empty() {
        layers.push(1);
    }
    if layers.len() == 1 {
        set_layer(&mut layers, 0, num_lights);
    }
    let lc = layers.len() as i64;
    let max_on_layer = (0..lc)
        .map(|l| {
            let outside = (lc - l - 1) as f32;
            1 + (f64::from(layers[l as usize] as f32) * (1.0 + f64::from(outside / lc as f32))) as i64
        })
        .max()
        .unwrap_or(1);
    let buffer = max_on_layer;
    let mut outer = buffer as f64 / 2.0;
    if ratio < 1.0 {
        ratio = 1.0;
    }
    let mut inner = outer / ratio;
    let mut delta = 0.0;
    if lc > 1 {
        if inner_pct == -1 {
            inner_pct = (100.0f32 / lc as f32) as i64;
        }
        delta = (outer * (100.0 - inner_pct as f32) as f64) / (100.0 * (lc as f32 - 1.0)) as f64;
    }
    let gap = PI * 2.0 / points as f64;
    let dir = if start_loc.contains("-CCW") { -1.0 } else { 1.0 };
    let odd = points % 2 == 1;
    let start_angle = if start_loc.contains("Top") {
        0.0
    } else if start_loc.contains("Bottom Ctr") {
        PI
    } else if start_loc.contains("Left") {
        PI + if odd { gap / 2.0 } else { 0.0 }
    } else {
        PI - if odd { gap / 2.0 } else { 0.0 }
    };
    let segments = 2 * points;
    let (first, end, step) = if start_loc.contains("Inside") {
        outer -= delta * (lc - 1) as f64;
        inner = outer / ratio;
        delta = -delta;
        (0, lc, 1)
    } else {
        (lc - 1, -1, -1)
    };
    let on_circle = |r: f64, a: f64| [r * a.sin(), r * a.cos()];
    let on_line = |s: [f64; 2], e: [f64; 2], d: f64| {
        let len = ((e[0] - s[0]).powi(2) + (e[1] - s[1]).powi(2)).sqrt();
        if len == 0.0 {
            return s;
        }
        let t = d / len;
        [(1.0 - t) * s[0] + t * e[0], (1.0 - t) * s[1] + t * e[1]]
    };
    let bottom_ctr = start_loc.contains("Bottom Ctr");

    let mut nodes: Vec<Option<RawNode>> = (0..node_count).map(|_| None).collect();
    let mut cur = 0i64;
    if !cx.single_node {
        let mut chan = 0i64;
        let mut l = first;
        while l != end {
            if cur >= node_count {
                break;
            }
            let layer_nodes = layers[l as usize];
            let end_node = cur + layer_nodes;
            let mut start_outer = !bottom_ctr;
            let s0 = on_circle(if start_outer { outer } else { inner }, start_angle);
            let e0 = on_circle(if start_outer { inner } else { outer }, start_angle + gap / 2.0);
            let seg_len = ((e0[0] - s0[0]).powi(2) + (e0[1] - s0[1]).powi(2)).sqrt();
            let coord_gap = segments as f64 * seg_len / (layer_nodes * coords) as f64;
            let mut pos = 0.0;
            let mut angle = start_angle;
            let mut seg_end = 0.0;
            for s in 0..segments {
                if cur >= node_count {
                    break;
                }
                let st = on_circle(if start_outer { outer } else { inner }, angle);
                let en = on_circle(if start_outer { inner } else { outer }, angle + gap * dir / 2.0);
                let seg_start = seg_end;
                seg_end = seg_start + seg_len;
                if s == segments - 1 {
                    seg_end += 0.001;
                }
                while pos < seg_end && cur < end_node {
                    let (string, in_string) = (cur / nps, cur % nps);
                    if in_string == 0 && string < strings {
                        chan = starts[string as usize];
                    }
                    let mut pts = Vec::with_capacity(coords as usize);
                    for _ in 0..coords {
                        let p = on_line(st, en, pos - seg_start);
                        pts.push([p[0], p[1], 0.0]);
                        pos += coord_gap;
                    }
                    nodes[cur as usize] = Some(RawNode::new(chan, pts));
                    chan += cx.cpn;
                    cur += 1;
                    if cur >= node_count {
                        break;
                    }
                }
                angle += gap * dir / 2.0;
                start_outer = !start_outer;
            }
            outer -= delta;
            inner = outer / ratio;
            l += step;
        }
        if cur < node_count {
            cx.note("star nodes beyond the layer sizes are drawn at the center");
        }
        for n in cur..node_count {
            let (string, in_string) = (n / nps, n % nps);
            if in_string == 0 {
                chan = starts[string as usize];
            }
            nodes[n as usize] = Some(RawNode::new(chan, vec![[0.0; 3]; coords as usize]));
            chan += cx.cpn;
        }
    } else {
        let half = buffer / 2;
        let mut l = first;
        while l != end {
            if cur >= node_count {
                break;
            }
            let count = layers[l as usize].min(coords);
            if count == 0 {
                l += step;
                continue;
            }
            let chan = starts[cur as usize];
            let mut start_outer = !bottom_ctr;
            let s0 = on_circle(if start_outer { outer } else { inner }, start_angle);
            let e0 = on_circle(if start_outer { inner } else { outer }, start_angle + gap / 2.0);
            let seg_len = ((e0[0] - s0[0]).powi(2) + (e0[1] - s0[1]).powi(2)).sqrt();
            let coord_gap = segments as f64 * seg_len / count as f64;
            let mut pts: Vec<[f64; 3]> = Vec::with_capacity(coords as usize);
            let mut last_buf = (0i64, 0i64);
            let mut pos = 0.0;
            let mut angle = start_angle;
            for s in 0..segments {
                if pts.len() as i64 >= count {
                    break;
                }
                let st = on_circle(if start_outer { outer } else { inner }, angle);
                let en = on_circle(if start_outer { inner } else { outer }, angle + gap * dir / 2.0);
                let seg_start = s as f64 * seg_len;
                while pos < seg_start + seg_len {
                    let p = on_line(st, en, pos - seg_start);
                    last_buf = (trunc_i(p[0] + half as f64), trunc_i(p[1] + half as f64 - 1.0));
                    pts.push([p[0], p[1], 0.0]);
                    pos += coord_gap;
                    if pts.len() as i64 >= count {
                        break;
                    }
                }
                angle += gap * dir / 2.0;
                start_outer = !start_outer;
            }
            while (pts.len() as i64) < coords {
                pts.push([(last_buf.0 - half) as f64, (last_buf.1 - half) as f64, 0.0]);
            }
            nodes[cur as usize] = Some(RawNode::new(chan, pts));
            cur += 1;
            outer -= delta;
            inner = outer / ratio;
            l += step;
        }
        if cur < node_count {
            cx.note("star strings beyond the layer count are drawn at the center");
        }
        for n in cur..node_count {
            nodes[n as usize] = Some(RawNode::new(starts[n as usize], vec![[0.0; 3]; coords as usize]));
        }
    }
    let nodes = nodes.into_iter().flatten().collect();
    let xf = boxed(cx, 0.0, [1.0; 3]);
    Raw { nodes, xf }
}

/// `SpinnerModel::InitModel` + `SetSpinnerCoord`: arms radiating from a hollow center.
pub(super) fn spinner(cx: &mut Ctx) -> Raw {
    let strings = cx.parm("NumStrings", "parm1", "1").max(0);
    let npa = cx.parm("NodesPerArm", "parm2", "1").max(0);
    let aps = cx.parm("ArmsPerString", "parm3", "1").max(0);
    let hollow = cx.int("Hollow", 20);
    let start_angle = cx.int("StartAngle", 0);
    let arc = cx.int("Arc", 360);
    let zigzag = cx.is("ZigZag", "true");
    let alternate = cx.is("Alternate", "true");
    let pps = aps.saturating_mul(npa);
    let lights = strings.saturating_mul(pps);
    if cx.over_cap(lights.max(strings)) || lights == 0 {
        return Raw::empty();
    }
    let single = cx.single_node;
    let cpn = cx.cpn;
    let cps = if single { cpn } else { cpn * npa * aps };
    let starts = cx.string_starts(strings, cps, &[]);
    let arms = aps * strings;
    let (node_count, coords) = string_nodes(cx, strings, pps);
    let mut nodes: Vec<RawNode> = (0..node_count)
        .map(|i| {
            let chan = if single {
                starts[i as usize]
            } else {
                let (x, y) = (i / npa, i % npa);
                starts[(x / aps) as usize] + (x % aps) * npa * cpn + y * cpn
            };
            RawNode::new(chan, vec![[0.0; 3]; coords as usize])
        })
        .collect();

    let pi = std::f32::consts::PI;
    let mut angle = (pi * 2.0 * (270.0 + start_angle as f32)) / 360.0;
    let mut incr = (pi * 2.0 * arc as f32) / (strings as f32 * aps as f32 * 360.0);
    if arc < 360 && aps * strings > 1 {
        incr = (pi * 2.0 * arc as f32) / ((strings as f32 * aps as f32 - 1.0) * 360.0);
    }
    let cw = !cx.ltor;
    let from_centre = !cx.btot;
    let hollow_r = hollow as f64 * 2.0 * npa as f64 / 100.0;
    let pos_on_arm = |n: i64, a: i64| -> i64 {
        let mut n1 = if from_centre { n } else { npa - n - 1 };
        if zigzag && a % 2 > 0 {
            n1 = if from_centre { npa - n - 1 } else { n };
        }
        n1
    };
    for a in 0..arms {
        let (s, c) = (f64::from(angle).sin(), f64::from(angle).cos());
        let point = |n1: i64| {
            let r = 0.5 + n1 as f64 + hollow_r;
            [r * c, r * s, 0.0]
        };
        if single {
            let a1 = a / aps;
            let first = (a % aps) * npa;
            for n in 0..npa {
                if let Some(p) = nodes[a1 as usize].pts.get_mut((first + n) as usize) {
                    *p = point(pos_on_arm(n, a));
                }
            }
        } else {
            for n in 0..npa {
                let n1 = if alternate {
                    if n < (npa + 1) / 2 {
                        2 * n
                    } else {
                        (npa - (n + 1)) * 2 + 1
                    }
                } else {
                    pos_on_arm(n, a)
                };
                for p in nodes[(n + a * npa) as usize].pts.iter_mut() {
                    *p = point(n1);
                }
            }
        }
        if cw {
            angle -= incr;
        } else {
            angle += incr;
        }
    }
    let xf = boxed(cx, 0.0, [1.0; 3]);
    Raw { nodes, xf }
}

/// `WindowFrameModel::InitFrame`: one string around the frame; `Dir`/`StartSide` pick the start
/// corner and `Rotation` the direction. Float arithmetic as in xLights.
pub(super) fn window_frame(cx: &mut Ctx) -> Raw {
    let top = cx.parm("TopNodes", "parm1", "0").max(0);
    let side = cx.parm("SideNodes", "parm2", "0").max(0);
    let bottom = cx.parm("BottomNodes", "parm3", "0").max(0);
    let rotation = cx.text("Rotation", "CW");
    let ccw = !(rotation == "Clockwise" || rotation == "CW");
    let total = top + 2 * side + bottom;
    if cx.over_cap(total) || side + top + bottom == 0 {
        return Raw::empty();
    }
    let single = cx.single_node;
    let cpn = cx.cpn;
    let start0 = cx.string_starts(1, 0, &[])[0];
    let (ltor, btot) = (cx.ltor, cx.btot);
    let width = top.max(bottom) + 2;
    let height = side;
    let w = width as f32;
    let dir: f32 = if ccw { -1.0 } else { 1.0 };
    let odd_corner = if ccw { btot == ltor } else { btot != ltor };
    let (wadj, hadj) = if odd_corner { (2, -2) } else { (0, 0) };
    let mut top_si = 1.0f32;
    if top + wadj - 1 != 0 {
        top_si = w / (top + 1) as f32;
    }
    let mut bot_si = 1.0f32;
    if bottom + wadj - 1 != 0 {
        bot_si = -w / (bottom + 1) as f32;
    }
    let lengths = [side + hadj, top + wadj, side + hadj, bottom + wadj];
    let xsi = [0.0, top_si, 0.0, bot_si];
    let ysi = [1.0f32, 0.0, -1.0, 0.0];
    let hh = (height - 1) as f32 / 2.0;
    let (xs, ys): ([f32; 4], [f32; 4]) = if ccw {
        if odd_corner {
            (
                [-w / 2.0, w / 2.0, w / 2.0, -w / 2.0],
                [hh - 1.0, hh, -hh + 1.0, -hh],
            )
        } else {
            (
                [-w / 2.0, w / 2.0 - top_si, w / 2.0, -w / 2.0 - bot_si],
                [hh, hh, -hh, -hh],
            )
        }
    } else if odd_corner {
        (
            [-w / 2.0, -w / 2.0, w / 2.0, w / 2.0],
            [-hh + 1.0, hh, hh - 1.0, -hh],
        )
    } else {
        (
            [-w / 2.0, -w / 2.0 + top_si, w / 2.0, w / 2.0 + bot_si],
            [-hh, hh, hh, -hh],
        )
    };
    let idx: [usize; 4] = match (ltor, btot, ccw) {
        (true, true, false) => [0, 1, 2, 3],
        (true, true, true) => [3, 2, 1, 0],
        (true, false, false) => [1, 2, 3, 0],
        (true, false, true) => [0, 3, 2, 1],
        (false, true, false) => [3, 0, 1, 2],
        (false, true, true) => [2, 1, 0, 3],
        (false, false, false) => [2, 3, 0, 1],
        (false, false, true) => [1, 0, 3, 2],
    };
    let next_side = |mut s: usize| {
        for _ in 0..4 {
            if lengths[idx[s]] != 0 {
                break;
            }
            s = (s + 1) % 4;
        }
        s
    };
    let mut s = next_side(0);
    let (mut sx, mut sy) = (xs[idx[s]], ys[idx[s]]);
    let mut cur_len = lengths[idx[s]];
    let mut pts = Vec::with_capacity(total as usize);
    for _ in 0..total {
        pts.push([f64::from(sx), f64::from(sy), 0.0]);
        sx += xsi[idx[s]] * dir;
        sy += ysi[idx[s]] * dir;
        cur_len -= 1;
        if cur_len <= 0 {
            s = next_side((s + 1) % 4);
            sx = xs[idx[s]];
            sy = ys[idx[s]];
            cur_len = lengths[idx[s]];
        }
    }
    let nodes = if single {
        vec![RawNode::new(start0, pts)]
    } else {
        pts.into_iter()
            .enumerate()
            .map(|(i, p)| RawNode::new(start0 + i as i64 * cpn, vec![p]))
            .collect()
    };
    let xf = boxed(cx, 0.0, [1.0; 3]);
    Raw { nodes, xf }
}
