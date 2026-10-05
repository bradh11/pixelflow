//! Pixel positions and channel offsets for each xLights model type, reproducing xLights' own
//! node layout so an imported show looks and maps exactly like it does in xLights.
//!
//! Each model type is a port of the matching `src-core/models/*Model.cpp` node generator
//! (channel assignment and local "screen" coordinates), followed by the model's screen-location
//! transform as xLights draws it in the 2D layout view (Boxed, Two-point, Three-point or
//! Poly-point). Z is dropped after the transform. Where something cannot be reproduced exactly,
//! [`Geometry::approximate`] says so in plain language.

mod custom;
mod grid;
mod lines;
mod poly;
mod radial;
#[cfg(test)]
mod tests;
mod xform;

use crate::model::XmlModel;
use xform::Affine;

/// One xLights node: where its channels start within the model's block and where its lights are.
#[derive(Debug, Clone, PartialEq)]
pub struct XNode {
    /// 0-based channel offset of this node within the model's channel block.
    pub channel: u32,
    /// Light positions in layout (world) coordinates, x right and y up. Most nodes have one;
    /// "dumb" strings and multi-light nodes have several.
    pub points: Vec<[f32; 2]>,
}

/// A model's nodes (sorted by channel) and channel usage.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub nodes: Vec<XNode>,
    pub channels_per_node: u8,
    /// Channels the model uses (its block length).
    pub channels: u32,
    /// Set when the layout could only be approximated, saying how (plain language).
    pub approximate: Option<String>,
}

/// Most lights (or strings) imported for one model; larger models are skipped with a note.
const MAX_LIGHTS: i64 = 1_000_000;

/// Computes a model's nodes, channels, and positions.
pub fn geometry(model: &XmlModel) -> Geometry {
    let mut cx = Ctx::new(model);
    let raw = dispatch(&mut cx);
    finish(cx, raw)
}

/// A 3D point in a model's local space (before the screen-location transform).
type V3 = [f64; 3];

/// A node as produced by a type's generator: channel relative to the model start (may be out of
/// range for broken input; `finish` handles that) and local light positions.
struct RawNode {
    chan: i64,
    pts: Vec<V3>,
}

impl RawNode {
    fn new(chan: i64, pts: Vec<V3>) -> Self {
        RawNode { chan, pts }
    }
}

/// A generator's result: nodes in local space plus the transform into layout coordinates.
struct Raw {
    nodes: Vec<RawNode>,
    xf: Affine,
}

impl Raw {
    fn empty() -> Self {
        Raw {
            nodes: Vec::new(),
            xf: Affine::IDENTITY,
        }
    }
}

fn dispatch(cx: &mut Ctx) -> Raw {
    // Same matching order as `XmlDeserializingModelFactory::Deserialize`.
    let t = cx.m.display_as.trim().to_string();
    match t.as_str() {
        "Arches" => return lines::arches(cx),
        "Candy Canes" => return lines::candy_canes(cx),
        "Channel Block" => return lines::channel_block(cx),
        "Circle" => return radial::circle(cx),
        "Cube" => return grid::cube(cx),
        "Custom" => return custom::custom(cx),
        "Icicles" => return lines::icicles(cx),
        "Image" | "Label" | "ModelGroup" => return Raw::empty(),
        _ => {}
    }
    if t.starts_with("Dmx") {
        return dmx(cx);
    }
    if t.contains("Matrix") {
        return grid::matrix(cx);
    }
    if t.contains("MultiPoint") {
        return poly::multi_point(cx);
    }
    match t.as_str() {
        "Single Line" => return lines::single_line(cx),
        "Poly Line" => return poly::poly_line(cx),
        "Sphere" => return grid::sphere(cx),
        "Spinner" => return radial::spinner(cx),
        "Star" => return radial::star(cx),
        _ => {}
    }
    if t.contains("Tree") {
        return grid::tree(cx);
    }
    match t.as_str() {
        "Window Frame" => radial::window_frame(cx),
        "Wreath" => radial::wreath(cx),
        _ => unknown(cx),
    }
}

/// DMX fixtures: `DmxChannelCount` single-channel nodes (`DmxModel::InitModel`), shown as a small
/// row at the model position since their real look comes from a 3D fixture drawing.
fn dmx(cx: &mut Ctx) -> Raw {
    cx.cpn = 1;
    let n = cx.parm("DmxChannelCount", "parm1", "1").max(0);
    if cx.over_cap(n) {
        return Raw::empty();
    }
    cx.note("DMX fixture shown as a row of channel dots at its position");
    let start = cx.string_starts(1, n, &[])[0];
    row_at_position(cx, n, |i| start + i)
}

/// Unknown model types: lay out `parm1 * parm2` RGB-style nodes in a row so channels still line up.
fn unknown(cx: &mut Ctx) -> Raw {
    let strings = cx.parm("NumStrings", "parm1", "1").max(0);
    let nodes = cx.parm("NodesPerString", "parm2", "0").max(0);
    let total = strings.saturating_mul(nodes);
    if cx.over_cap(total) {
        return Raw::empty();
    }
    let name = cx.m.display_as.trim().to_string();
    cx.note(format!(
        "Unknown model type \"{name}\" shown as a row of nodes at its position"
    ));
    let cpn = cx.cpn;
    row_at_position(cx, total, |i| i * cpn)
}

fn row_at_position(cx: &Ctx, n: i64, chan: impl Fn(i64) -> i64) -> Raw {
    let pos = cx.world_pos();
    let spacing = 4.0;
    let half = (n - 1).max(0) as f64 / 2.0;
    let nodes = (0..n)
        .map(|i| RawNode::new(chan(i), vec![[(i as f64 - half) * spacing, 0.0, 0.0]]))
        .collect();
    Raw {
        nodes,
        xf: Affine::translate(pos),
    }
}

fn finish(mut cx: Ctx, raw: Raw) -> Geometry {
    let mut bad_pos = false;
    let mut bad_chan = false;
    let mut nodes: Vec<XNode> = raw
        .nodes
        .into_iter()
        .map(|n| {
            let points = n
                .pts
                .iter()
                .map(|p| {
                    let w = raw.xf.apply(*p);
                    let (x, y) = (w[0] as f32, w[1] as f32);
                    if x.is_finite() && y.is_finite() {
                        [x, y]
                    } else {
                        bad_pos = true;
                        [0.0, 0.0]
                    }
                })
                .collect();
            let channel = if (0..=i64::from(u32::MAX)).contains(&n.chan) {
                n.chan as u32
            } else {
                bad_chan = true;
                n.chan.clamp(0, i64::from(u32::MAX)) as u32
            };
            XNode { channel, points }
        })
        .collect();
    if bad_pos {
        cx.note("some light positions could not be computed and were placed at the origin");
    }
    if bad_chan {
        cx.note("some channel numbers fell outside the model and were clamped");
    }
    nodes.sort_by_key(|n| n.channel);
    let cpn = cx.cpn.clamp(1, 255);
    let channels = nodes
        .iter()
        .map(|n| u64::from(n.channel) + cpn as u64)
        .max()
        .unwrap_or(0)
        .min(u64::from(u32::MAX));
    Geometry {
        nodes,
        channels_per_node: cpn as u8,
        channels: channels as u32,
        approximate: if cx.notes.is_empty() {
            None
        } else {
            Some(cx.notes.join("; "))
        },
    }
}

/// Per-model reading context: attribute access with xLights' read rules, the string type's
/// channel facts, wiring direction flags, and collected approximation notes.
struct Ctx<'a> {
    m: &'a XmlModel,
    /// Channel stride per node (`Model::GetNodeChannelCount`).
    cpn: i64,
    /// One node per string holding all its lights (`Model::HasSingleNode`).
    single_node: bool,
    /// `cpn == 1` and not "Node Single Color".
    single_channel: bool,
    /// `Dir != "R"`.
    ltor: bool,
    /// `StartSide == "B"`, or missing.
    btot: bool,
    notes: Vec<String>,
}

impl<'a> Ctx<'a> {
    fn new(m: &'a XmlModel) -> Self {
        let st = m.attr("StringType").unwrap_or("RGB Nodes");
        let cpn = node_channel_count(st, superstring_colours(m));
        let single_node = has_single_node(st);
        let single_channel = cpn == 1 && st != "Node Single Color";
        let ltor = m.attr("Dir").unwrap_or("L") != "R";
        let btot = m.attr("StartSide").is_none_or(|s| s == "B");
        let mut cx = Ctx {
            m,
            cpn,
            single_node,
            single_channel,
            ltor,
            btot,
            notes: Vec::new(),
        };
        let ts = m.connection.get("ts").map_or(0, |v| strtol0(v));
        if ts > 1 {
            cx.note("smart-remote tail strings (ts) are not modelled; channel order assumes one tail");
        }
        cx
    }

    fn note(&mut self, s: impl Into<String>) {
        let s = s.into();
        if !self.notes.contains(&s) {
            self.notes.push(s);
        }
    }

    /// Records a note and returns true when `count` exceeds the per-model import limit.
    fn over_cap(&mut self, count: i64) -> bool {
        if count > MAX_LIGHTS {
            self.note(format!(
                "model has {count} lights or strings, more than the {MAX_LIGHTS} imported per model; skipped"
            ));
            true
        } else {
            false
        }
    }

    fn attr(&self, key: &str) -> Option<&str> {
        self.m.attr(key)
    }

    /// `ReadAttrWithParmFallback`: the named attribute if present, else the legacy `parmN`, else
    /// `default`, parsed like `strtol` (garbage reads as 0, as in xLights).
    fn parm(&self, key: &str, parm: &str, default: &str) -> i64 {
        let v = self.attr(key).or_else(|| self.attr(parm)).unwrap_or(default);
        strtol0(v)
    }

    /// pugixml `as_int(default)`: `default` only when the attribute is missing.
    fn int(&self, key: &str, default: i64) -> i64 {
        self.attr(key).map_or(default, strtol0)
    }

    /// pugixml `as_float(default)`, with non-finite values treated as `default`.
    fn float(&self, key: &str, default: f64) -> f64 {
        match self.attr(key) {
            None => default,
            Some(v) => {
                let f = strtod(v).unwrap_or(0.0);
                if f.is_finite() { f } else { default }
            }
        }
    }

    fn text(&self, key: &str, default: &'a str) -> &'a str {
        self.m.attr(key).unwrap_or(default)
    }

    /// Exact string comparison, as xLights does for its "true"/"false" flags.
    fn is(&self, key: &str, value: &str) -> bool {
        self.attr(key) == Some(value)
    }

    fn world_pos(&self) -> V3 {
        let f = |k| {
            let v = self.float(k, 0.0);
            if v.is_finite() { v } else { 0.0 }
        };
        [f("WorldPosX"), f("WorldPosY"), f("WorldPosZ")]
    }

    /// `Model::CalcChannelsPerString` for models that don't override it.
    fn default_cps(&self, nodes_per_string: i64) -> i64 {
        if self.single_channel {
            1
        } else if self.single_node {
            self.cpn
        } else {
            nodes_per_string.saturating_mul(self.cpn)
        }
    }

    /// `Model::SetStringStartChannels`, relative to the model's start channel (string 0 of a
    /// plain model starts at 0). `indiv_nodes` are 1-based individual start nodes (Custom, Poly
    /// Line, MultiPoint); `Advanced="1"` individual start channels take precedence.
    fn string_starts(&mut self, n_strings: i64, cps: i64, indiv_nodes: &[i64]) -> Vec<i64> {
        let n = n_strings.clamp(0, MAX_LIGHTS) as usize;
        let contiguous = |i: usize| (i as i64).saturating_mul(cps);
        if self.int("Advanced", 0) != 0 {
            let rel: Option<Vec<i64>> = (0..n).map(|i| self.indiv_start(i)).collect();
            match rel {
                Some(v) if v.iter().all(|&c| c >= 0) => return v,
                _ => {
                    self.note(
                        "individual string start channels could not be related to the model start; \
                         strings laid out back to back",
                    );
                    return (0..n).map(contiguous).collect();
                }
            }
        }
        (0..n)
            .map(|i| match indiv_nodes.get(i) {
                Some(&node) => (node.max(1) - 1).saturating_mul(self.cpn),
                None => contiguous(i),
            })
            .collect()
    }

    /// Offset of `String{i+1}` from the model's `StartChannel`, when both use the same reference
    /// (plain numbers, or the same `!Controller:` / `>Model:` / `@Model:` / `#Universe:` prefix).
    fn indiv_start(&self, i: usize) -> Option<i64> {
        let s = self.attr(&format!("String{}", i + 1))?;
        let (p, n) = split_channel(s)?;
        let (p0, n0) = split_channel(self.text("StartChannel", "1"))?;
        (p == p0).then_some(n - n0)
    }
}

/// Splits a start-channel string into its reference prefix and number (`"!Ctl:10"` →
/// `("!Ctl", 10)`, `"25"` → `("", 25)`); plain numbers below 1 count as 1, as in xLights.
fn split_channel(s: &str) -> Option<(String, i64)> {
    let s = s.trim();
    match s.rfind(':') {
        Some(i) => Some((s[..i].trim().to_string(), strtol(&s[i + 1..])?)),
        None => Some((String::new(), strtol(s)?.max(1))),
    }
}

/// `Model::GetNodeChannelCount`.
fn node_channel_count(st: &str, superstring_colours: i64) -> i64 {
    let b = st.as_bytes();
    if st.starts_with("Single Color") || st == "Strobes White 3fps" || st == "Strobes" {
        1
    } else if st == "4 Channel RGBW" || st == "4 Channel WRGB" {
        4
    } else if st == "RGBWW Nodes" {
        5
    } else if b.first() == Some(&b'W') || b.get(3) == Some(&b'W') {
        4
    } else if st == "Superstring" {
        superstring_colours.max(1)
    } else if st == "Node Single Color" {
        1
    } else {
        3
    }
}

/// `Model::HasSingleNode`: "dumb" strings are one node per string.
fn has_single_node(st: &str) -> bool {
    if st == "Node Single Color" {
        return false;
    }
    if st == "Superstring" {
        return true;
    }
    st.len() >= " Nodes".len() && !st.contains(" Nodes")
}

/// Number of consecutive `SuperStringColour0..` attributes.
fn superstring_colours(m: &XmlModel) -> i64 {
    (0..)
        .take_while(|i| m.attr(&format!("SuperStringColour{i}")).is_some())
        .count() as i64
}

/// `strtol(s, nullptr, 10)` clamped to the 32-bit `int` range; `None` when there are no digits.
fn strtol(s: &str) -> Option<i64> {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let end = digits.bytes().take_while(u8::is_ascii_digit).count();
    if end == 0 {
        return None;
    }
    let mut v: i64 = 0;
    for b in digits[..end].bytes() {
        v = v.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    let v = if neg { -v } else { v };
    Some(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)))
}

/// [`strtol`] with no digits reading as 0.
fn strtol0(s: &str) -> i64 {
    strtol(s).unwrap_or(0)
}

/// `strtod`: the longest leading decimal number in `s`.
fn strtod(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i += 1;
    }
    let int_digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    i += int_digits;
    let mut frac_digits = 0;
    if b.get(i) == Some(&b'.') {
        frac_digits = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if int_digits + frac_digits > 0 {
            i += 1 + frac_digits;
        }
    }
    if int_digits + frac_digits == 0 {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exp_digits = b[j..].iter().take_while(|c| c.is_ascii_digit()).count();
        if exp_digits > 0 {
            i = j + exp_digits;
        }
    }
    s[..i].parse().ok()
}

/// `(int)` conversion of a double: truncation toward zero, saturating, NaN as 0.
fn trunc_i(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else {
        v.trunc().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i64
    }
}

/// Positive integers from a comma list (`Model::DeserializeLayerSizes`).
fn layer_sizes(s: &str) -> Vec<i64> {
    s.split(',').map(strtol0).filter(|&v| v > 0).collect()
}

/// xLights' `SetLayerSize`, which ignores a size of 0.
fn set_layer(sizes: &mut [i64], i: usize, v: i64) {
    if v != 0
        && let Some(s) = sizes.get_mut(i)
    {
        *s = v;
    }
}

/// `Model::NodesPerString(int string)` for models that don't override it, given the model's
/// overall nodes-per-string, string count and total node count.
fn nodes_per_string_of(
    string: i64,
    num_strings: i64,
    base_nps: i64,
    node_count: i64,
    single_node: bool,
    indiv_nodes: &[i64],
) -> i64 {
    if num_strings <= 1 {
        return base_nps;
    }
    if single_node {
        return 1;
    }
    let start = |x: i64| -> i64 {
        if !indiv_nodes.is_empty() {
            return indiv_nodes.get(x as usize).copied().unwrap_or(0);
        }
        compute_string_start_node(x, num_strings, node_count)
    };
    let v1 = start(string);
    if string < num_strings - 1 {
        start(string + 1) - v1
    } else {
        node_count - v1 + 1
    }
}

/// `Model::ComputeStringStartNode` (1-based, float arithmetic as in xLights).
fn compute_string_start_node(x: i64, strings: i64, nodes: i64) -> i64 {
    if x == 0 || strings <= 0 {
        return 1;
    }
    let per = nodes as f32 / strings as f32;
    trunc_i(f64::from(x as f32 * per + 1.0))
}
