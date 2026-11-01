//! What the fonts draw, held to the bit: every letter's coverage, advance, and place at several
//! sizes, made-up bold and italic included; each font's measurements and kerning; the letters it
//! lacks; which fonts can't be drawn at all; and how families are found by name.
//!
//! The fonts are made here, using what real fonts use: quadratic curves that start off the curve,
//! a contour with no point on it, letters made of other letters (moved, scaled, skewed, nested),
//! a left side bearing that disagrees with the outline, CFF outlines with fractions, subroutines,
//! hints, and flex, character maps of every kind, kerning of every kind, names in several
//! languages and encodings, and tables broken in the ways real files are. The expected values
//! were taken from the font reader PixelFlow used before (ttf-parser), so that a change of reader
//! is held to drawing exactly what it drew.

use crate::find::{FaceInfo, Library};
use crate::sized;
use crate::testing::{i16s, pad4, sfnt, tiny_collection, tiny_font, u16s};
use std::sync::Arc;

// ---- Digest ---------------------------------------------------------------------------------

/// FNV-1a over everything drawn: a change anywhere changes it.
struct Digest(u64);

impl Digest {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Sizes in pixels: a light prop's few pixels up to a matrix's many, whole and fractional.
const SIZES: [f32; 6] = [4.0, 6.5, 11.0, 17.25, 24.0, 48.0];

/// Everything `face` draws for `text`: at each size, plain and with a made-up bold, italic, and
/// both, the font's measurements, each letter's advance, place, size, and coverage, and the
/// kerning between every two of its letters; then the letters it lacks.
fn drawn(face: &FaceInfo, text: &str) -> String {
    let mut d = Digest::new();
    for &size in &SIZES {
        for (fake_bold, fake_italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let Some(font) = sized::sized(face, size, fake_bold, fake_italic) else {
                d.bytes(b"no font");
                continue;
            };
            d.f32(font.ascent);
            d.f32(font.descent);
            d.f32(font.line_gap);
            let glyphs: Vec<_> = text.chars().map(|c| font.glyph(c)).collect();
            for g in &glyphs {
                d.f32(g.advance);
                d.i32(g.left);
                d.i32(g.top);
                d.i32(g.width);
                d.i32(g.height);
                for y in 0..g.height {
                    for x in 0..g.width {
                        d.bytes(&[g.coverage(x, y)]);
                    }
                }
            }
            for a in &glyphs {
                for b in &glyphs {
                    d.f32(font.kerning(a, b));
                }
            }
        }
    }
    match sized::missing_letters(face, text) {
        Some(missing) => missing.into_iter().for_each(|c| d.u32(u32::from(c))),
        None => d.bytes(b"unreadable"),
    }
    d.hex()
}

/// The faces in a font file's bytes, as the computer's fonts would hold them.
fn library(bytes: Vec<u8>) -> Library {
    let mut lib = Library::default();
    lib.add_data(Arc::from(bytes));
    lib
}

/// The one face in `bytes`.
fn only_face(bytes: Vec<u8>) -> FaceInfo {
    let lib = library(bytes);
    assert_eq!(lib.len(), 1, "one face");
    lib.faces()[0].clone()
}

// ---- Writing tables -------------------------------------------------------------------------

type Tables = Vec<([u8; 4], Vec<u8>)>;

/// A number in 2.14, as a composite letter's scale is stored.
fn f2dot14(v: f32) -> i16 {
    (v * 16384.0).round() as i16
}

fn head(upem: u16, long_loca: bool) -> Vec<u8> {
    let mut head = Vec::new();
    u16s(&mut head, &[1, 0]);
    head.extend(0x0001_0000u32.to_be_bytes());
    head.extend(0u32.to_be_bytes());
    head.extend(0x5F0F_3CF5u32.to_be_bytes());
    u16s(&mut head, &[0, upem]);
    head.extend([0u8; 16]);
    i16s(&mut head, &[0, -300, 1000, 900]);
    u16s(&mut head, &[0, 8]);
    i16s(&mut head, &[2, i16::from(long_loca), 0]);
    head
}

fn hhea(ascender: i16, descender: i16, line_gap: i16, long_metrics: u16) -> Vec<u8> {
    let mut hhea = Vec::new();
    hhea.extend(0x0001_0000u32.to_be_bytes());
    i16s(&mut hhea, &[ascender, descender, line_gap]);
    u16s(&mut hhea, &[1000]);
    i16s(&mut hhea, &[0, 0, 900, 1, 0, 0, 0, 0, 0, 0, 0]);
    u16s(&mut hhea, &[long_metrics]);
    hhea
}

fn maxp(glyphs: u16) -> Vec<u8> {
    let mut maxp = 0x0000_5000u32.to_be_bytes().to_vec();
    u16s(&mut maxp, &[glyphs]);
    maxp
}

/// An OS/2 table of `version`, weight 400, with `selection` flags and typographic and Windows
/// line metrics.
fn os2(version: u16, selection: u16, typo: [i16; 3], win: [u16; 2]) -> Vec<u8> {
    let len = match version {
        0 => 78,
        1 => 86,
        2..=4 => 96,
        _ => 100,
    };
    let mut t = vec![0u8; len];
    t[0..2].copy_from_slice(&version.to_be_bytes());
    t[4..6].copy_from_slice(&400u16.to_be_bytes());
    t[6..8].copy_from_slice(&5u16.to_be_bytes());
    t[62..64].copy_from_slice(&selection.to_be_bytes());
    t[68..70].copy_from_slice(&typo[0].to_be_bytes());
    t[70..72].copy_from_slice(&typo[1].to_be_bytes());
    t[72..74].copy_from_slice(&typo[2].to_be_bytes());
    t[74..76].copy_from_slice(&win[0].to_be_bytes());
    t[76..78].copy_from_slice(&win[1].to_be_bytes());
    t
}

/// Advances and left side bearings: the first `long` glyphs each have an advance, the rest share
/// the last one and have only a bearing.
fn hmtx(metrics: &[(u16, i16)], long: usize) -> Vec<u8> {
    let mut t = Vec::new();
    for (i, &(advance, lsb)) in metrics.iter().enumerate() {
        if i < long {
            u16s(&mut t, &[advance]);
        }
        i16s(&mut t, &[lsb]);
    }
    t
}

/// A name table: each record is (platform, encoding, language, name ID, the string's bytes).
fn name(records: &[(u16, u16, u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let count = records.len() as u16;
    u16s(&mut out, &[0, count, 6 + 12 * count]);
    let mut offset = 0u16;
    for (platform, encoding, language, id, bytes) in records {
        u16s(
            &mut out,
            &[*platform, *encoding, *language, *id, bytes.len() as u16, offset],
        );
        offset += bytes.len() as u16;
    }
    for (.., bytes) in records {
        out.extend(bytes);
    }
    out
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// Windows US English names for `family`, as most fonts have.
fn windows_names(family: &str) -> Vec<u8> {
    name(&[
        (3, 1, 0x409, 1, utf16(family)),
        (3, 1, 0x409, 2, utf16("Regular")),
    ])
}

/// A character map: each subtable is (platform, encoding, its bytes), in the order given.
fn cmap(subtables: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[0, subtables.len() as u16]);
    let mut offset = 4 + 8 * subtables.len() as u32;
    for (platform, encoding, data) in subtables {
        u16s(&mut out, &[*platform, *encoding]);
        out.extend(offset.to_be_bytes());
        offset += data.len() as u32;
    }
    for (.., data) in subtables {
        out.extend(data);
    }
    out
}

/// Format 0: a glyph for each of the first 256 codes.
fn cmap0(map: &[(u8, u8)]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[0, 262, 0]);
    let mut ids = [0u8; 256];
    for &(c, g) in map {
        ids[usize::from(c)] = g;
    }
    out.extend(ids);
    out
}

/// Format 2, single bytes only: codes from `first` map through `glyphs` (0 is none), plus `delta`.
fn cmap2(first: u16, glyphs: &[u16], delta: i16) -> Vec<u8> {
    let mut out = Vec::new();
    // Every high byte uses subheader 0.
    let len = 6 + 512 + 8 + 2 * glyphs.len();
    u16s(&mut out, &[2, len as u16, 0]);
    out.extend([0u8; 512]);
    // The range offset counts from where it is stored to the first glyph.
    u16s(&mut out, &[first, glyphs.len() as u16]);
    i16s(&mut out, &[delta]);
    u16s(&mut out, &[2]);
    u16s(&mut out, glyphs);
    out
}

/// Format 4: a segment per entry of `deltas` ((first, last, glyph of first): each code the next
/// glyph), one segment from `ranged.0` mapping through the glyph list `ranged.1` (0 is none),
/// and the closing segment.
fn cmap4(deltas: &[(u16, u16, u16)], ranged: Option<(u16, Vec<u16>)>) -> Vec<u8> {
    let mut segs: Vec<(u16, u16, i16, Option<usize>)> = deltas
        .iter()
        .map(|&(first, last, glyph)| (first, last, glyph.wrapping_sub(first) as i16, None))
        .collect();
    let mut array = Vec::new();
    if let Some((first, glyphs)) = &ranged {
        segs.push((*first, first + glyphs.len() as u16 - 1, 0, Some(0)));
        array.extend(glyphs.iter().copied());
    }
    segs.sort_by_key(|s| s.0);
    segs.push((0xFFFF, 0xFFFF, 1, None));
    let n = segs.len() as u16;
    let mut body = Vec::new();
    let selector = 15 - n.leading_zeros() as u16;
    let range = (1u16 << selector) * 2;
    u16s(&mut body, &[n * 2, range, selector, n * 2 - range]);
    u16s(&mut body, &segs.iter().map(|s| s.1).collect::<Vec<_>>());
    u16s(&mut body, &[0]);
    u16s(&mut body, &segs.iter().map(|s| s.0).collect::<Vec<_>>());
    i16s(&mut body, &segs.iter().map(|s| s.2).collect::<Vec<_>>());
    for (i, s) in segs.iter().enumerate() {
        // From this entry of the range offsets to the glyph list after them.
        let offset = s.3.map_or(0, |at| 2 * (n as usize - i + at)) as u16;
        u16s(&mut body, &[offset]);
    }
    u16s(&mut body, &array);
    let mut out = Vec::new();
    u16s(&mut out, &[4, (6 + body.len()) as u16, 0]);
    out.extend(body);
    out
}

/// Format 6: codes from `first` map to `glyphs` in turn (0 included).
fn cmap6(first: u16, glyphs: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(
        &mut out,
        &[6, (10 + 2 * glyphs.len()) as u16, 0, first, glyphs.len() as u16],
    );
    u16s(&mut out, glyphs);
    out
}

/// Format 10: codes from `first` (any plane) map to `glyphs` in turn.
fn cmap10(first: u32, glyphs: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[10, 0]);
    out.extend((20 + 2 * glyphs.len() as u32).to_be_bytes());
    out.extend(0u32.to_be_bytes());
    out.extend(first.to_be_bytes());
    out.extend((glyphs.len() as u32).to_be_bytes());
    u16s(&mut out, glyphs);
    out
}

/// Format 12 (`format` 12: each code in a group the next glyph) or 13 (every code in a group the
/// same glyph): groups of (first code, last code, glyph).
fn cmap12(format: u16, groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[format, 0]);
    out.extend((16 + 12 * groups.len() as u32).to_be_bytes());
    out.extend(0u32.to_be_bytes());
    out.extend((groups.len() as u32).to_be_bytes());
    for &(first, last, glyph) in groups {
        out.extend(first.to_be_bytes());
        out.extend(last.to_be_bytes());
        out.extend(glyph.to_be_bytes());
    }
    out
}

/// An OpenType kern table: each subtable is (format, coverage byte, its data after the header).
fn kern_ot(subtables: &[(u8, u8, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[0, subtables.len() as u16]);
    for (format, coverage, data) in subtables {
        u16s(&mut out, &[0, (6 + data.len()) as u16]);
        out.extend([*format, *coverage]);
        out.extend(data);
    }
    out
}

/// An Apple kern table: each subtable is (coverage byte, format, its data after the header).
fn kern_aat(subtables: &[(u8, u8, Vec<u8>)]) -> Vec<u8> {
    let mut out = 0x0001_0000u32.to_be_bytes().to_vec();
    out.extend((subtables.len() as u32).to_be_bytes());
    for (coverage, format, data) in subtables {
        out.extend((8 + data.len() as u32).to_be_bytes());
        out.extend([*coverage, *format]);
        u16s(&mut out, &[0]);
        out.extend(data);
    }
    out
}

/// Kerning format 0: pairs of (left glyph, right glyph, value), sorted.
fn kern0(pairs: &[(u16, u16, i16)]) -> Vec<u8> {
    let mut pairs = pairs.to_vec();
    pairs.sort_by_key(|p| (p.0, p.1));
    let n = pairs.len() as u16;
    let mut out = Vec::new();
    u16s(&mut out, &[n, 6, 0, 0]);
    for (l, r, v) in pairs {
        u16s(&mut out, &[l, r]);
        i16s(&mut out, &[v]);
    }
    out
}

/// Kerning format 2 (a class for each glyph, a value for each two classes), with `header` bytes
/// before it in its subtable: left classes for glyphs from `first`, right classes, and the
/// values, a row of `columns` per left class.
fn kern2(header: usize, first: u16, left: &[u16], right: &[u16], columns: u16, values: &[i16]) -> Vec<u8> {
    // Row width, then offsets (from the subtable's start) of the two class tables and the array.
    let left_at = header + 8;
    let right_at = left_at + 4 + 2 * left.len();
    let array_at = right_at + 4 + 2 * right.len();
    let mut out = Vec::new();
    u16s(
        &mut out,
        &[columns * 2, left_at as u16, right_at as u16, array_at as u16],
    );
    u16s(&mut out, &[first, left.len() as u16]);
    // Left classes are offsets into the array, from the subtable's start; right ones are column
    // offsets.
    u16s(
        &mut out,
        &left
            .iter()
            .map(|&c| array_at as u16 + c * columns * 2)
            .collect::<Vec<_>>(),
    );
    u16s(&mut out, &[first, right.len() as u16]);
    u16s(&mut out, &right.iter().map(|&c| c * 2).collect::<Vec<_>>());
    i16s(&mut out, values);
    out
}

/// Kerning format 3 (Apple's compact classes): for each glyph a left and a right class, an index
/// for each two classes, and the values the indexes pick.
fn kern3(values: &[i16], left: &[u8], right: &[u8], classes: (u8, u8), indexes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[left.len() as u16]);
    out.extend([values.len() as u8, classes.0, classes.1, 0]);
    i16s(&mut out, values);
    out.extend(left);
    out.extend(right);
    out.extend(indexes);
    out
}

// ---- TrueType outlines ----------------------------------------------------------------------

/// A contour: points across and up, and whether each is on the curve.
type Contour = Vec<(i16, i16, bool)>;

/// How a component of a composite letter is placed.
#[derive(Clone, Copy)]
enum Args {
    /// Moved by 16-bit amounts.
    Words(i16, i16),
    /// Moved by 8-bit amounts.
    Bytes(i8, i8),
    /// Placed so that one of the letter's points meets one of the component's.
    Points(u8, u8),
}

#[derive(Clone, Copy)]
enum Scale {
    Unscaled,
    One(f32),
    Xy(f32, f32),
    Matrix(f32, f32, f32, f32),
}

#[derive(Clone)]
struct Part {
    glyph: u16,
    args: Args,
    scale: Scale,
    /// Flags beyond those the placement and scale set (USE_MY_METRICS, SCALED_COMPONENT_OFFSET).
    flags: u16,
}

fn part(glyph: u16, args: Args, scale: Scale) -> Part {
    Part {
        glyph,
        args,
        scale,
        flags: 0,
    }
}

#[derive(Clone)]
enum Shape {
    Empty,
    Simple(Vec<Contour>),
    /// Components, and the bounding box the letter claims.
    Composite(Vec<Part>, [i16; 4]),
}

fn simple_glyph(contours: &[Contour]) -> Vec<u8> {
    let mut out = Vec::new();
    let points: Vec<_> = contours.iter().flatten().copied().collect();
    let xs = points.iter().map(|p| p.0);
    let ys = points.iter().map(|p| p.1);
    i16s(
        &mut out,
        &[
            contours.len() as i16,
            xs.clone().min().unwrap_or(0),
            ys.clone().min().unwrap_or(0),
            xs.max().unwrap_or(0),
            ys.max().unwrap_or(0),
        ],
    );
    let mut end = 0u16;
    for c in contours {
        end += c.len() as u16;
        u16s(&mut out, &[end - 1]);
    }
    // No instructions; coordinates as 16-bit steps from the last point.
    u16s(&mut out, &[0]);
    out.extend(points.iter().map(|p| u8::from(p.2)));
    let mut last = (0i16, 0i16);
    let (mut dx, mut dy) = (Vec::new(), Vec::new());
    for p in &points {
        dx.push(p.0 - last.0);
        dy.push(p.1 - last.1);
        last = (p.0, p.1);
    }
    i16s(&mut out, &dx);
    i16s(&mut out, &dy);
    out
}

fn composite_glyph(parts: &[Part], bbox: [i16; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    i16s(&mut out, &[-1, bbox[0], bbox[1], bbox[2], bbox[3]]);
    for (i, p) in parts.iter().enumerate() {
        let mut flags = p.flags;
        if i + 1 < parts.len() {
            flags |= 0x0020;
        }
        flags |= match p.args {
            Args::Words(..) => 0x0003,
            Args::Bytes(..) => 0x0002,
            Args::Points(..) => 0,
        };
        flags |= match p.scale {
            Scale::Unscaled => 0,
            Scale::One(_) => 0x0008,
            Scale::Xy(..) => 0x0040,
            Scale::Matrix(..) => 0x0080,
        };
        u16s(&mut out, &[flags, p.glyph]);
        match p.args {
            Args::Words(x, y) => i16s(&mut out, &[x, y]),
            Args::Bytes(x, y) => out.extend([x as u8, y as u8]),
            Args::Points(a, b) => out.extend([a, b]),
        }
        match p.scale {
            Scale::Unscaled => {}
            Scale::One(s) => i16s(&mut out, &[f2dot14(s)]),
            Scale::Xy(x, y) => i16s(&mut out, &[f2dot14(x), f2dot14(y)]),
            Scale::Matrix(a, b, c, d) => i16s(&mut out, &[f2dot14(a), f2dot14(b), f2dot14(c), f2dot14(d)]),
        }
    }
    out
}

/// glyf and loca (long offsets) for `shapes`.
fn glyf_loca(shapes: &[Shape]) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for s in shapes {
        loca.extend((glyf.len() as u32).to_be_bytes());
        match s {
            Shape::Empty => {}
            Shape::Simple(contours) => glyf.extend(simple_glyph(contours)),
            Shape::Composite(parts, bbox) => glyf.extend(composite_glyph(parts, *bbox)),
        }
        pad4(&mut glyf);
    }
    loca.extend((glyf.len() as u32).to_be_bytes());
    (glyf, loca)
}

fn on(x: i16, y: i16) -> (i16, i16, bool) {
    (x, y, true)
}

fn off(x: i16, y: i16) -> (i16, i16, bool) {
    (x, y, false)
}

fn rect(x0: i16, y0: i16, x1: i16, y1: i16) -> Contour {
    vec![on(x0, y0), on(x0, y1), on(x1, y1), on(x1, y0)]
}

/// The letters of the TrueType test font, by glyph: (character, shape, advance, left bearing).
/// Each advance differs, so a letter drawn at the em's size names its glyph by its advance.
fn curve_letters() -> Vec<(Option<char>, Shape, u16, i16)> {
    use Args::*;
    use Scale::*;
    vec![
        // 0: the missing-letter shape, a box with a hole.
        (
            None,
            Shape::Simple(vec![
                rect(60, 0, 440, 700),
                vec![on(120, 60), on(380, 60), on(380, 640), on(120, 640)],
            ]),
            500,
            60,
        ),
        // 1: space.
        (Some(' '), Shape::Empty, 280, 0),
        // 2: curves from a point on the curve, and a hole of curves.
        (
            Some('a'),
            Shape::Simple(vec![
                vec![on(100, 0), off(470, 0), on(470, 350), off(470, 700), on(100, 700)],
                vec![
                    on(180, 120),
                    off(180, 580),
                    on(300, 580),
                    off(390, 350),
                    on(300, 120),
                ],
            ]),
            560,
            100,
        ),
        // 3: a contour whose first point is off the curve and second on it.
        (
            Some('b'),
            Shape::Simple(vec![vec![
                off(90, 350),
                on(90, 690),
                off(510, 710),
                on(520, 350),
                off(505, 3),
                on(97, 11),
            ]]),
            610,
            90,
        ),
        // 4: a contour with no point on the curve.
        (
            Some('c'),
            Shape::Simple(vec![vec![
                off(70, 351),
                off(301, 707),
                off(533, 349),
                off(299, -7),
            ]]),
            620,
            70,
        ),
        // 5: two points off the curve first, the last on it.
        (
            Some('d'),
            Shape::Simple(vec![vec![
                off(100, 100),
                off(110, 610),
                on(410, 700),
                off(530, 290),
                on(450, 0),
            ]]),
            630,
            100,
        ),
        // 6: a box whose left bearing (30) isn't where its outline starts (120).
        (Some('e'), Shape::Simple(vec![rect(120, 0, 420, 500)]), 640, 30),
        // 7: two letters moved, by 16-bit and 8-bit amounts.
        (
            Some('f'),
            Shape::Composite(
                vec![
                    part(2, Words(60, -40), Unscaled),
                    part(4, Bytes(90, 100), Unscaled),
                ],
                [70, -40, 623, 807],
            ),
            650,
            70,
        ),
        // 8: letters scaled, scaled across and up differently, and turned.
        (
            Some('g'),
            Shape::Composite(
                vec![
                    part(3, Words(31, 17), One(0.7)),
                    part(2, Words(400, -50), Xy(0.6, 1.3)),
                    part(4, Words(700, 300), Matrix(0.866, 0.5, -0.5, 0.866)),
                ],
                [93, -50, 1290, 960],
            ),
            1300,
            93,
        ),
        // 9: a composite of composites, one scaled.
        (
            Some('h'),
            Shape::Composite(
                vec![part(8, Words(10, 20), One(0.9)), part(7, Words(500, 0), Unscaled)],
                [0, 0, 0, 0],
            ),
            1310,
            0,
        ),
        // 10: a scaled component whose metrics the letter takes, its offset marked as scaled.
        (
            Some('i'),
            Shape::Composite(
                vec![Part {
                    flags: 0x0200 | 0x0800,
                    ..part(6, Words(200, 100), One(0.75))
                }],
                [290, 100, 515, 475],
            ),
            660,
            10,
        ),
        // 11: one point and nothing else.
        (Some('j'), Shape::Simple(vec![vec![on(300, 300)]]), 670, 300),
        // 12: a component placed by meeting points.
        (
            Some('k'),
            Shape::Composite(vec![part(2, Points(0, 1), Unscaled)], [100, 0, 470, 700]),
            680,
            100,
        ),
        // 13: components that are empty or aren't in the font, beside one that is.
        (
            Some('l'),
            Shape::Composite(
                vec![
                    part(1, Words(0, 0), Unscaled),
                    part(999, Words(0, 0), Unscaled),
                    part(6, Words(-20, 30), Unscaled),
                ],
                [100, 30, 400, 530],
            ),
            690,
            100,
        ),
        // 14: only in the full-repertoire map (U+1F642); shares the last advance.
        (
            Some('\u{1F642}'),
            Shape::Simple(vec![vec![on(50, 0), on(300, 650), on(550, 0)]]),
            690,
            50,
        ),
        // 15: a letter that kerns, sharing the last advance too.
        (
            Some('V'),
            Shape::Simple(vec![vec![on(0, 700), on(300, 0), on(600, 700)]]),
            690,
            0,
        ),
    ]
}

/// The tables of the TrueType test font, `family`, `upem` units to the em.
fn curve_font_tables(family: &str, upem: u16) -> Tables {
    let letters = curve_letters();
    let shapes: Vec<Shape> = letters.iter().map(|l| l.1.clone()).collect();
    let (glyf, loca) = glyf_loca(&shapes);
    let metrics: Vec<(u16, i16)> = letters.iter().map(|l| (l.2, l.3)).collect();
    // The last two share the advance before them.
    let long = letters.len() - 2;
    let bmp: Vec<(u16, u16, u16)> = letters
        .iter()
        .enumerate()
        .filter_map(|(g, l)| {
            let c = u32::from(l.0?);
            (c <= 0xFFFF).then_some((c as u16, c as u16, g as u16))
        })
        .collect();
    let mut full: Vec<(u32, u32, u32)> = letters
        .iter()
        .enumerate()
        .filter_map(|(g, l)| l.0.map(|c| (u32::from(c), u32::from(c), g as u32)))
        .collect();
    full.sort();
    let kern = kern_ot(&[(
        0,
        0x01,
        kern0(&[(2, 15, -80), (15, 2, 45), (6, 7, -13), (15, 15, 7)]),
    )]);
    vec![
        // Typographic metrics, which the font asks to be used.
        (
            *b"OS/2",
            os2(4, (1 << 7) | (1 << 6), [905, -297, 61], [1010, 330]),
        ),
        // A BMP map first (it can't hold U+1F642), then the full one.
        (
            *b"cmap",
            cmap(&[(0, 3, cmap4(&bmp, None)), (3, 10, cmap12(12, &full))]),
        ),
        (*b"glyf", glyf),
        (*b"head", head(upem, true)),
        (*b"hhea", hhea(880, -220, 40, long as u16)),
        (*b"hmtx", hmtx(&metrics, long)),
        (*b"kern", kern),
        (*b"loca", loca),
        (*b"maxp", maxp(letters.len() as u16)),
        (*b"name", windows_names(family)),
    ]
}

fn curve_font(family: &str, upem: u16) -> Vec<u8> {
    sfnt(0x0001_0000, curve_font_tables(family, upem), 0)
}

/// Every letter of the TrueType test font, and some it lacks.
const CURVE_TEXT: &str = "abcdefghijklV \u{1F642}aVa?é\u{FFFF}";

// ---- CFF outlines ---------------------------------------------------------------------------

/// A charstring number.
fn cs(out: &mut Vec<u8>, v: i32) {
    match v {
        -107..=107 => out.push((v + 139) as u8),
        108..=1131 => {
            let w = v - 108;
            out.extend([(w / 256 + 247) as u8, (w % 256) as u8]);
        }
        -1131..=-108 => {
            let w = -v - 108;
            out.extend([(w / 256 + 251) as u8, (w % 256) as u8]);
        }
        _ => {
            out.push(28);
            out.extend((v as i16).to_be_bytes());
        }
    }
}

/// A charstring number in 16.16, as a fraction is stored.
fn cs_fixed(out: &mut Vec<u8>, v: f64) {
    out.push(255);
    out.extend(((v * 65536.0).round() as i32).to_be_bytes());
}

/// A charstring written as text: numbers (a fraction is stored in 16.16), operators by name, and
/// a hint mask's bytes as `0x..`.
fn charstring(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let op: &[u8] = match word {
            "hstem" => &[1],
            "vstem" => &[3],
            "rlineto" => &[5],
            "hlineto" => &[6],
            "vlineto" => &[7],
            "rrcurveto" => &[8],
            "callsubr" => &[10],
            "return" => &[11],
            "endchar" => &[14],
            "hintmask" => &[19],
            "rmoveto" => &[21],
            "rcurveline" => &[24],
            "rlinecurve" => &[25],
            "vvcurveto" => &[26],
            "hhcurveto" => &[27],
            "callgsubr" => &[29],
            "vhcurveto" => &[30],
            "hvcurveto" => &[31],
            "flex" => &[12, 35],
            _ => &[],
        };
        if !op.is_empty() {
            out.extend(op);
        } else if let Some(hex) = word.strip_prefix("0x") {
            out.push(u8::from_str_radix(hex, 16).unwrap());
        } else if word.contains('.') {
            cs_fixed(&mut out, word.parse().unwrap());
        } else {
            cs(&mut out, word.parse().unwrap());
        }
    }
    out
}

/// A CFF INDEX of `items`, with 4-byte offsets.
fn index(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    u16s(&mut out, &[items.len() as u16]);
    if items.is_empty() {
        return out;
    }
    out.push(4);
    let mut at = 1u32;
    out.extend(at.to_be_bytes());
    for item in items {
        at += item.len() as u32;
        out.extend(at.to_be_bytes());
    }
    for item in items {
        out.extend(item);
    }
    out
}

/// A DICT number, always five bytes, so a DICT's size doesn't depend on its values.
fn dict_int(out: &mut Vec<u8>, v: i32) {
    out.push(29);
    out.extend(v.to_be_bytes());
}

/// A CFF table: `glyphs` charstrings, `local` and `global` subroutines.
fn cff(glyphs: &[Vec<u8>], local: &[Vec<u8>], global: &[Vec<u8>]) -> Vec<u8> {
    let header = [1u8, 0, 4, 4];
    let names = index(&[b"PinPost".to_vec()]);
    let strings = index(&[]);
    let globals = index(global);
    let charstrings = index(glyphs);
    // The private DICT: where its subroutines are, counted from its start.
    let mut private = Vec::new();
    dict_int(&mut private, 6);
    private.push(19);
    let locals = index(local);
    // The top DICT is 17 bytes: CharStrings (17) and Private (18).
    let top_len = 2 + 1 + 8 + 17;
    let charstrings_at = header.len() + names.len() + top_len + strings.len() + globals.len();
    let private_at = charstrings_at + charstrings.len();
    let mut top = Vec::new();
    dict_int(&mut top, charstrings_at as i32);
    top.push(17);
    dict_int(&mut top, private.len() as i32);
    dict_int(&mut top, private_at as i32);
    top.push(18);
    let tops = index(&[top]);
    assert_eq!(tops.len(), top_len);
    let mut out = header.to_vec();
    for part in [names, tops, strings, globals, charstrings, private, locals] {
        out.extend(part);
    }
    out
}

/// The CFF test font's charstrings, by glyph.
fn post_glyphs() -> Vec<Vec<u8>> {
    [
        // 0: the missing-letter shape, a box.
        "100 0 rmoveto 300 hlineto 600 vlineto -300 hlineto endchar",
        // 1: space, nothing drawn.
        "endchar",
        // 2: lines and curves, then a second contour.
        "50 0 rmoveto 400 0 rlineto 0 300 rlineto 0 200 -100 200 -200 200 rrcurveto \
         -100 0 -100 -200 0 -500 rrcurveto 120 100 rmoveto 100 100 rlineto -100 0 rlineto endchar",
        // 3: curves that begin across or down.
        "100 350 rmoveto 200 150 170 30 hvcurveto -150 -160 -120 -200 vhcurveto \
         20 -80 60 -90 70 hhcurveto -30 -70 -50 -60 -40 vvcurveto endchar",
        // 4: fractions: 16.16 numbers, and thirds that no binary fraction holds exactly.
        "120.5 33.25 rmoveto 250.75 0 rlineto 0.333328247 400.125 rlineto \
         -83.666656 100.333328 -83.666656 0.666656 -83.666656 -100.333328 rrcurveto \
         1.5 -400.125 rlineto endchar",
        // 5: pieces from a local and a global subroutine.
        "100 0 rmoveto -107 callsubr -107 callgsubr endchar",
        // 6: a width first, stems, a hint mask, then a box.
        "512 100 50 hstem 100 50 vstem hintmask 0xC0 100 0 rmoveto 200 hlineto 200 vlineto \
         -200 hlineto endchar",
        // 7: flex, a curve then a line, and a line then a curve.
        "0 300 rmoveto 50 50 50 50 50 0 50 0 50 -50 50 -50 50 flex \
         0 -100 -50 -50 -100 0 -100 -50 rcurveline -50 0 0 50 -20 30 -30 20 rlinecurve endchar",
    ]
    .map(charstring)
    .to_vec()
}

fn post_font(family: &str) -> Vec<u8> {
    let glyphs = post_glyphs();
    let local = [charstring("300 0 rlineto 0 300 rlineto return")];
    let global = [charstring("-300 0 rlineto return")];
    let n = glyphs.len();
    let metrics: Vec<(u16, i16)> = (0..n).map(|g| (500 + 10 * g as u16, 0)).collect();
    let chars: Vec<(u16, u16, u16)> = "abcdef"
        .chars()
        .enumerate()
        .map(|(i, c)| (c as u16, c as u16, i as u16 + 2))
        .chain([(b' ' as u16, b' ' as u16, 1)])
        .collect();
    let tables = vec![
        (*b"CFF ", cff(&glyphs, &local, &global)),
        (*b"OS/2", os2(0, 1 << 6, [800, -200, 90], [900, 250])),
        (*b"cmap", cmap(&[(3, 1, cmap4(&chars, None))])),
        (*b"head", head(1000, false)),
        (*b"hhea", hhea(750, -250, 0, n as u16)),
        (*b"hmtx", hmtx(&metrics, n)),
        (*b"maxp", maxp(n as u16)),
        (*b"name", windows_names(family)),
    ];
    sfnt(u32::from_be_bytes(*b"OTTO"), tables, 0)
}

/// A collection holding each of `fonts` (flavor and tables).
fn collection(fonts: Vec<(u32, Tables)>) -> Vec<u8> {
    let header = 12 + 4 * fonts.len() as u32;
    let mut at = header.div_ceil(4) * 4;
    let mut offsets = Vec::new();
    let mut files = Vec::new();
    for (flavor, tables) in fonts {
        let file = sfnt(flavor, tables, at);
        offsets.push(at);
        at += file.len() as u32;
        files.push(file);
    }
    let mut out = b"ttcf".to_vec();
    out.extend(0x0001_0000u32.to_be_bytes());
    out.extend((offsets.len() as u32).to_be_bytes());
    for o in offsets {
        out.extend(o.to_be_bytes());
    }
    pad4(&mut out);
    for f in files {
        out.extend(f);
    }
    out
}

/// Replaces (or with `None` removes) the table `tag`.
fn with(mut tables: Tables, tag: &[u8; 4], data: Option<Vec<u8>>) -> Tables {
    tables.retain(|t| &t.0 != tag);
    if let Some(data) = data {
        tables.push((*tag, data));
    }
    tables
}

/// How each of `chars` is drawn at the em's size: its advance (which names the glyph in the test
/// fonts), or `None` once the font can't be drawn.
fn advances(face: &FaceInfo, upem: f32, chars: &str) -> Option<Vec<f32>> {
    let font = sized::sized(face, upem, false, false)?;
    Some(chars.chars().map(|c| font.glyph(c).advance).collect())
}

// ---- The tests ------------------------------------------------------------------------------

#[test]
fn the_tiny_font_is_unchanged_and_draws_as_before() {
    let mut d = Digest::new();
    d.bytes(&tiny_font("Pin Tiny", false, false));
    assert_eq!(d.hex(), "743fdd5b3fc9f71b");
    let text: String = (32u8..127).map(char::from).chain(['é', '\n']).collect();
    let face = only_face(tiny_font("Pin Tiny", false, false));
    assert_eq!(drawn(&face, &text), "2ca6dc4b3cfd744e");
    let lib = library(tiny_collection(&[
        ("Pin Pair", false, false),
        ("Pin Pair", true, true),
    ]));
    let second = lib.faces().iter().find(|f| f.index == 1).unwrap();
    assert_eq!(drawn(second, "AVOIgL x"), "9c25d951c55edb79");
}

#[test]
fn truetype_curves_and_composites_draw_as_before() {
    let face = only_face(curve_font("Pin Curves", 1100));
    assert_eq!(drawn(&face, CURVE_TEXT), "c0f57d29086fca1b");
    // At the em's size, pixels are font units. A box whose left bearing disagrees with its
    // outline is drawn where its outline is; a component placed by meeting points isn't moved; a
    // letter of one point draws nothing.
    let f = sized::sized(&face, 1100.0, false, false).unwrap();
    let bounds = |c| {
        let g = f.glyph(c);
        (g.left, g.top, g.width, g.height)
    };
    assert_eq!(bounds('e'), (120, -500, 300, 500));
    assert_eq!(bounds('k'), bounds('a'));
    assert_eq!(bounds('j'), (0, 0, 0, 0));
    assert_eq!(bounds('g'), (94, -997, 882, 1047));
    // At the em's size each letter's advance is its glyph's (the last two share one).
    assert_eq!(
        advances(&face, 1100.0, "abcdefghijklV \u{1F642}?").unwrap(),
        [
            560.0, 610.0, 620.0, 630.0, 640.0, 650.0, 1300.0, 1310.0, 660.0, 670.0, 680.0, 690.0, 690.0,
            280.0, 690.0, 500.0
        ]
    );
}

#[test]
fn cff_outlines_draw_as_before() {
    let face = only_face(post_font("Pin Post"));
    assert_eq!(drawn(&face, "abcdef ?"), "3891d16c4460b069");
    // The letter of fractions, at the em's size (pixels are font units).
    let c = sized::sized(&face, 1000.0, false, false).unwrap().glyph('c');
    assert_eq!((c.left, c.top, c.width, c.height), (120, -509, 252, 476));
}

#[test]
fn a_collections_faces_of_both_kinds_draw_as_before() {
    let ttc = collection(vec![
        (u32::from_be_bytes(*b"OTTO"), {
            // The CFF font's tables, taken from its file.
            let file = post_font("Pin Mixed Post");
            tables_of(&file)
        }),
        (0x0001_0000, curve_font_tables("Pin Mixed Curves", 2048)),
    ]);
    let lib = library(ttc);
    assert_eq!(lib.len(), 2);
    let post = lib.find("pin mixed post", false, false).unwrap().face;
    let curves = lib.find("pin mixed curves", false, false).unwrap().face;
    assert_eq!((post.index, curves.index), (0, 1));
    assert_eq!(drawn(post, "abcdef ?"), "3891d16c4460b069");
    assert_eq!(drawn(curves, CURVE_TEXT), "8ee81ffcda5347d3");
}

/// The tables of a font file made by `sfnt`, read back.
fn tables_of(file: &[u8]) -> Tables {
    let n = usize::from(u16::from_be_bytes([file[4], file[5]]));
    (0..n)
        .map(|i| {
            let r = &file[12 + 16 * i..];
            let at = u32::from_be_bytes([r[8], r[9], r[10], r[11]]) as usize;
            let len = u32::from_be_bytes([r[12], r[13], r[14], r[15]]) as usize;
            ([r[0], r[1], r[2], r[3]], file[at..at + len].to_vec())
        })
        .collect()
}

#[test]
fn line_metrics_come_from_the_same_tables_as_before() {
    // (ascent, descent, line gap) at 1000 pixels to the 1000-unit em: font units.
    let metrics = |os2_table: Option<Vec<u8>>, hhea_table: Vec<u8>| {
        let tables = with(
            with(curve_font_tables("Pin Metrics", 1000), b"OS/2", os2_table),
            b"hhea",
            Some(hhea_table),
        );
        let face = only_face(sfnt(0x0001_0000, tables, 0));
        let f = sized::sized(&face, 1000.0, false, false).unwrap();
        (f.ascent, f.descent, f.line_gap)
    };
    let typo = [905, -297, 61];
    let win = [1010, 330];
    // The font asks for its typographic metrics (OS/2 version 4 or later).
    assert_eq!(
        metrics(Some(os2(4, 1 << 7, typo, win)), hhea(880, -220, 40, 14)),
        (905.0, 297.0, 61.0)
    );
    // Before version 4 the request isn't read: the horizontal header's.
    assert_eq!(
        metrics(Some(os2(3, 1 << 7, typo, win)), hhea(880, -220, 40, 14)),
        (880.0, 220.0, 40.0)
    );
    // Without the request: the horizontal header's.
    assert_eq!(
        metrics(Some(os2(5, 0, typo, win)), hhea(880, -220, 40, 14)),
        (880.0, 220.0, 40.0)
    );
    // A header with no ascender or descender takes the typographic ones, and their gap.
    assert_eq!(
        metrics(Some(os2(1, 0, typo, win)), hhea(0, 0, 40, 14)),
        (905.0, 297.0, 61.0)
    );
    // ... or, with none of those either, the Windows ones, and no gap.
    assert_eq!(
        metrics(Some(os2(2, 0, [0, 0, 61], win)), hhea(0, 0, 40, 14)),
        (1010.0, 330.0, 0.0)
    );
    // An OS/2 table of an unknown version, or too short for its version, isn't read.
    assert_eq!(
        metrics(Some(os2(6, 1 << 7, typo, win)), hhea(0, -220, 40, 14)),
        (0.0, 220.0, 40.0)
    );
    let mut short = os2(4, 1 << 7, typo, win);
    short.truncate(90);
    assert_eq!(
        metrics(Some(short), hhea(880, -220, 40, 14)),
        (880.0, 220.0, 40.0)
    );
    // No OS/2 table.
    assert_eq!(metrics(None, hhea(0, -220, 40, 14)), (0.0, 220.0, 40.0));
}

#[test]
fn characters_map_to_the_same_letters_as_before() {
    // The TrueType test font's tables with another character map. Each glyph's advance (at the
    // em's size) names it: 2 is 560, 3 is 610, 4 is 620, 5 is 630, 0 (none) is 500.
    let mapped = |subtables: &[(u16, u16, Vec<u8>)], chars: &str| {
        let tables = with(curve_font_tables("Pin Map", 1100), b"cmap", Some(cmap(subtables)));
        let face = only_face(sfnt(0x0001_0000, tables, 0));
        (
            advances(&face, 1100.0, chars).unwrap(),
            sized::missing_letters(&face, chars).unwrap(),
        )
    };
    // A symbol map isn't read: every letter is missing.
    assert_eq!(
        mapped(&[(3, 0, cmap4(&[(0xF041, 0xF043, 2)], None))], "A\u{F041}"),
        (vec![500.0, 500.0], vec!['A', '\u{F041}'])
    );
    // The first map that has a letter draws it; later ones fill in what it lacks.
    assert_eq!(
        mapped(
            &[
                (0, 3, cmap4(&[(0x41, 0x41, 2)], None)),
                (3, 10, cmap12(12, &[(0x41, 0x42, 3), (0x1F600, 0x1F600, 4)])),
            ],
            "AB\u{1F600}C"
        ),
        (vec![560.0, 620.0, 620.0, 500.0], vec!['C'])
    );
    // A full-repertoire Windows map must be format 12 or 13; the BMP one may be any.
    assert_eq!(
        mapped(
            &[
                (3, 10, cmap4(&[(0x41, 0x41, 2)], None)),
                (3, 1, cmap12(12, &[(0x41, 0x41, 3)])),
            ],
            "A"
        ),
        (vec![610.0], vec![])
    );
    // A Mac map isn't read; format 6 maps a run, glyph 0 included (not missing).
    assert_eq!(
        mapped(
            &[
                (1, 0, cmap0(&[(0x41, 4), (0x44, 4)])),
                (0, 3, cmap6(0x41, &[2, 0, 3])),
            ],
            "ABCD"
        ),
        (vec![560.0, 500.0, 610.0, 500.0], vec!['D'])
    );
    // Format 0 counts glyph 0 as none, so the next map is asked.
    assert_eq!(
        mapped(
            &[
                (0, 3, cmap0(&[(0x41, 2)])),
                (3, 1, cmap4(&[(0x41, 0x42, 4)], None)),
            ],
            "AB"
        ),
        (vec![560.0, 630.0], vec![])
    );
    // Format 4 through its glyph list (0 is none), then format 13 (a range to one glyph).
    assert_eq!(
        mapped(
            &[
                (0, 3, cmap4(&[(0x61, 0x61, 5)], Some((0x41, vec![2, 0, 3])))),
                (3, 10, cmap12(13, &[(0x41, 0x5A, 4)])),
            ],
            "ABCDa"
        ),
        (vec![560.0, 620.0, 610.0, 620.0, 630.0], vec![])
    );
    // Format 2 (single bytes here) and format 10.
    assert_eq!(
        mapped(
            &[
                (0, 3, cmap2(0x41, &[2, 0, 3], 1)),
                (0, 4, cmap10(0x1F600, &[4, 5])),
            ],
            "ABC\u{1F600}\u{1F601}\u{1F602}"
        ),
        (
            vec![610.0, 500.0, 620.0, 620.0, 630.0, 500.0],
            vec!['B', '\u{1F602}']
        )
    );
    // Groups out of order are searched as they are.
    assert_eq!(
        mapped(&[(3, 10, cmap12(12, &[(0x42, 0x42, 3), (0x41, 0x41, 2)]))], "AB"),
        (vec![560.0, 500.0], vec!['B'])
    );
    // A map whose platform isn't known ends the reading: nothing after it is looked at.
    assert_eq!(
        mapped(
            &[
                (7, 1, cmap4(&[(0x41, 0x41, 2)], None)),
                (3, 1, cmap4(&[(0x41, 0x41, 3)], None)),
            ],
            "A"
        ),
        (vec![500.0], vec!['A'])
    );
    // So does a map of a format that isn't known, or one too short to read.
    let mut unknown = cmap4(&[(0x41, 0x41, 2)], None);
    unknown[0..2].copy_from_slice(&3u16.to_be_bytes());
    assert_eq!(
        mapped(&[(0, 3, unknown), (3, 1, cmap4(&[(0x41, 0x41, 3)], None))], "A"),
        (vec![500.0], vec!['A'])
    );
    // No character map at all: every letter is missing and drawn as the missing-letter shape.
    let tables = with(curve_font_tables("Pin No Map", 1100), b"cmap", None);
    let face = only_face(sfnt(0x0001_0000, tables, 0));
    assert_eq!(advances(&face, 1100.0, "a ").unwrap(), [500.0, 500.0]);
    assert_eq!(sized::missing_letters(&face, "a b").unwrap(), ['a', ' ', 'b']);
}

#[test]
fn kerning_comes_from_the_same_subtables_as_before() {
    // Kerning between glyphs 2 ('a') and 15 ('V'), each way, at the em's size: font units.
    let kerned = |kern: Option<Vec<u8>>| {
        let tables = with(curve_font_tables("Pin Kern", 1000), b"kern", kern);
        let face = only_face(sfnt(0x0001_0000, tables, 0));
        let f = sized::sized(&face, 1000.0, false, false).unwrap();
        let (a, v, e) = (f.glyph('a'), f.glyph('V'), f.glyph('e'));
        [
            f.kerning(&a, &v),
            f.kerning(&v, &a),
            f.kerning(&v, &v),
            f.kerning(&e, &a),
        ]
    };
    assert_eq!(kerned(None), [0.0; 4]);
    // A vertical subtable is passed over; the first horizontal one with the pair answers.
    assert_eq!(
        kerned(Some(kern_ot(&[
            (0, 0x00, kern0(&[(2, 15, -11)])),
            (0, 0x01, kern0(&[(2, 15, -22), (15, 15, 3)])),
            (0, 0x01, kern0(&[(15, 2, 33), (2, 15, -44)])),
        ]))),
        [-22.0, 33.0, 3.0, 0.0]
    );
    // OpenType format 2: classes.
    assert_eq!(
        kerned(Some(kern_ot(&[(
            2,
            0x01,
            kern2(
                6,
                2,
                &[1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 2],
                &[1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                3,
                &[0, 0, 0, 0, -50, 9, 0, 70, -5]
            ),
        )]))),
        [-50.0, 70.0, 70.0, 70.0]
    );
    // Apple's table: a state machine (no pairs), a cross-stream subtable, then format 3.
    assert_eq!(
        kerned(Some(kern_aat(&[
            (0x00, 1, vec![0; 8]),
            (0x40, 0, kern0(&[(2, 15, -66)])),
            (
                0x00,
                3,
                kern3(
                    &[0, -31, 17],
                    &[0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                    &[0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
                    (3, 3),
                    &[0, 0, 0, 0, 0, 1, 0, 2, 0],
                ),
            ),
        ]))),
        [-66.0, 0.0, -31.0, 17.0]
    );
    // A variable Apple subtable is passed over.
    assert_eq!(
        kerned(Some(kern_aat(&[
            (0x20, 0, kern0(&[(2, 15, -7)])),
            (0x00, 0, kern0(&[(2, 15, -8)]))
        ]))),
        [-8.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn fonts_that_could_not_be_drawn_still_cannot() {
    // Each is found (its names are fine) but can't be drawn: no size, no letters to check.
    let broken = |tables: Tables| {
        let face = only_face(sfnt(0x0001_0000, tables, 0));
        (
            sized::sized(&face, 20.0, false, false).is_none(),
            sized::missing_letters(&face, "a").is_none(),
        )
    };
    let fine = curve_font_tables("Pin Broken", 1000);
    assert_eq!(broken(fine.clone()), (false, false));
    let mut tiny_em = head(15, true);
    assert_eq!(
        broken(with(fine.clone(), b"head", Some(tiny_em.clone()))),
        (true, true)
    );
    tiny_em = head(16385, true);
    assert_eq!(broken(with(fine.clone(), b"head", Some(tiny_em))), (true, true));
    let mut loca_format = head(1000, true);
    loca_format[51] = 2;
    assert_eq!(
        broken(with(fine.clone(), b"head", Some(loca_format))),
        (true, true)
    );
    let mut short_head = head(1000, true);
    short_head.truncate(53);
    assert_eq!(
        broken(with(fine.clone(), b"head", Some(short_head))),
        (true, true)
    );
    assert_eq!(broken(with(fine.clone(), b"head", None)), (true, true));
    assert_eq!(broken(with(fine.clone(), b"maxp", Some(maxp(0)))), (true, true));
    let mut maxp_version = maxp(16);
    maxp_version[0..4].copy_from_slice(&0x0002_0000u32.to_be_bytes());
    assert_eq!(
        broken(with(fine.clone(), b"maxp", Some(maxp_version))),
        (true, true)
    );
    let mut short_hhea = hhea(880, -220, 40, 14);
    short_hhea.truncate(35);
    assert_eq!(
        broken(with(fine.clone(), b"hhea", Some(short_hhea))),
        (true, true)
    );
    assert_eq!(broken(with(fine.clone(), b"hhea", None)), (true, true));

    // Fonts missing what letters are drawn with still draw: as nothing, or with no advance.
    let drawn_without = |tag: &[u8; 4]| {
        let face = only_face(sfnt(
            0x0001_0000,
            with(curve_font_tables("Pin Without", 1000), tag, None),
            0,
        ));
        drawn(&face, "abgV ?")
    };
    assert_eq!(drawn_without(b"loca"), "ef9a91d008113246");
    assert_eq!(drawn_without(b"glyf"), "ef9a91d008113246");
    assert_eq!(drawn_without(b"hmtx"), "feaa7e8c540e51cc");
    // No number of advances: the advances aren't read.
    let tables = with(
        curve_font_tables("Pin Without", 1000),
        b"hhea",
        Some(hhea(880, -220, 40, 0)),
    );
    assert_eq!(
        drawn(&only_face(sfnt(0x0001_0000, tables, 0)), "abgV ?"),
        "feaa7e8c540e51cc"
    );

    // A collection face whose offset is past the file's end isn't found; one whose tables are
    // broken is found and can't be drawn.
    let mut ttc = collection(vec![
        (0x0001_0000, curve_font_tables("Pin Coll A", 1000)),
        (
            0x0001_0000,
            with(curve_font_tables("Pin Coll B", 1000), b"maxp", Some(maxp(0))),
        ),
    ]);
    let lib = library(ttc.clone());
    assert_eq!(lib.families(), ["Pin Coll A", "Pin Coll B"]);
    let b = lib.find("pin coll b", false, false).unwrap().face;
    assert!(sized::sized(b, 20.0, false, false).is_none());
    ttc[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(library(ttc).families(), ["Pin Coll A"]);
}

/// Each face in `bytes`: its family, every name it goes by, its weight, and whether it's italic.
fn found(bytes: Vec<u8>) -> Vec<String> {
    library(bytes)
        .faces()
        .iter()
        .map(|f| {
            format!(
                "{} | {} | {} {}",
                f.family,
                f.names.join(", "),
                f.weight,
                f.italic
            )
        })
        .collect()
}

fn named(records: &[(u16, u16, u16, u16, Vec<u8>)]) -> Vec<String> {
    found(sfnt(
        0x0001_0000,
        with(curve_font_tables("unused", 1000), b"name", Some(name(records))),
        0,
    ))
}

#[test]
fn families_are_named_as_before() {
    // The Windows US English name is the menu's; every family name the face goes by is kept,
    // a Mac name's letters past ASCII as '?'.
    assert_eq!(
        named(&[
            (1, 0, 0, 1, b"Caf\x8e Pin".to_vec()),
            (3, 1, 0x411, 1, utf16("ピン")),
            (3, 1, 0x409, 1, utf16("Café Pin")),
            (3, 1, 0x409, 2, utf16("Bold")),
            (3, 1, 0x409, 16, utf16("Café Group")),
        ]),
        ["Café Pin | caf? pin, ピン, café pin, café group | 400 false"]
    );
    // Without a Windows US English name: a Mac or Unicode one before another language.
    assert_eq!(
        named(&[
            (3, 1, 0x411, 1, utf16("ピン")),
            (0, 3, 0, 1, utf16("Pin Unicode")),
        ]),
        ["Pin Unicode | ピン, pin unicode | 400 false"]
    );
    // Only the family its maker groups it in: that is the menu's.
    assert_eq!(
        named(&[
            (3, 1, 0x409, 16, utf16("Pin Grouped")),
            (3, 1, 0x409, 2, utf16("Light"))
        ]),
        ["Pin Grouped | pin grouped | 400 false"]
    );
    // Symbol-encoded Windows names are UTF-16 too; a name that isn't valid UTF-16, or in an
    // encoding not read, is passed over; a blank one is too.
    let mut lone = utf16("Pin");
    lone.extend([0xD8, 0x00]);
    assert_eq!(
        named(&[
            (3, 1, 0x409, 1, lone),
            (1, 1, 0, 1, b"Pin Japanese Mac".to_vec()),
            (3, 1, 0x409, 1, utf16("   ")),
            (3, 0, 0x409, 1, utf16("Pin Symbol")),
        ]),
        ["Pin Symbol | pin symbol | 400 false"]
    );
    // A name whose text lies past the table ends the reading: names after it aren't read.
    let mut table = name(&[
        (3, 1, 0x409, 1, utf16("Pin First")),
        (3, 1, 0x409, 16, utf16("Pin Second")),
        (3, 1, 0x409, 16, utf16("Pin Third")),
    ]);
    // The second record's offset (at 6 + 12 + 10) points far past the strings.
    table[28..30].copy_from_slice(&0x7000u16.to_be_bytes());
    assert_eq!(
        found(sfnt(
            0x0001_0000,
            with(curve_font_tables("unused", 1000), b"name", Some(table)),
            0
        )),
        ["Pin First | pin first | 400 false"]
    );
    // So does a name of an unknown platform.
    assert_eq!(
        named(&[
            (3, 1, 0x409, 1, utf16("Pin Before")),
            (9, 1, 0x409, 16, utf16("Pin Unknown")),
            (3, 1, 0x409, 16, utf16("Pin After"))
        ]),
        ["Pin Before | pin before | 400 false"]
    );
    // A face with no family name isn't found.
    assert!(named(&[(3, 1, 0x409, 2, utf16("Regular"))]).is_empty());
    // A version 1 name table (language tags after the names), read as before.
    let mut v1 = name(&[
        (3, 1, 0x409, 1, utf16("Pin Version One")),
        (3, 1, 0x409, 16, utf16("Pin Version One Group")),
    ]);
    v1[0..2].copy_from_slice(&1u16.to_be_bytes());
    let storage = usize::from(u16::from_be_bytes([v1[4], v1[5]]));
    // No language tags, before the strings.
    v1.splice(storage..storage, [0, 0]);
    v1[4..6].copy_from_slice(&(storage as u16 + 2).to_be_bytes());
    assert_eq!(
        found(sfnt(
            0x0001_0000,
            with(curve_font_tables("unused", 1000), b"name", Some(v1)),
            0
        )),
        Vec::<String>::new()
    );
}
