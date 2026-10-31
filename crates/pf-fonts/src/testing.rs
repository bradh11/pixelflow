//! A tiny TrueType font made in code, for tests that can't count on any font being installed
//! (the computers tests run on have different fonts, or none).
//!
//! Its letters are plain shapes, in font units of a 1000-unit em, with an ascent of 800 and a
//! descent of 200:
//!
//! - every letter and digit not named below: a box 100 to 500 across and 0 to 700 up, 600 wide;
//! - `I`: a bar 100 to 250 across, 350 wide;
//! - `O`: a ring 100 to 600 across with a hole 250 to 450 across and 150 to 550 up, 700 wide;
//! - `L`: an L, 600 wide;
//! - `g`: a box that hangs below the baseline, 100 to 500 across and -200 to 500 up;
//! - `V`: a box like the others, kerned 100 units closer after `A` (and every other box letter);
//! - space: nothing, 300 wide.
//!
//! Anything else is the font's missing-letter shape, which is empty and 500 wide.

/// The tiny font's em, in font units.
pub const UNITS_PER_EM: u16 = 1000;

struct Shape {
    advance: u16,
    contours: Vec<Vec<(i16, i16)>>,
}

fn rect(x0: i16, y0: i16, x1: i16, y1: i16) -> Vec<(i16, i16)> {
    // Clockwise with y up, as TrueType draws outer edges.
    vec![(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
}

// Glyph 0 is the missing-letter shape.
const SPACE: u16 = 1;
const BOX: u16 = 2;
const BAR: u16 = 3;
const RING: u16 = 4;
const VEE: u16 = 5;
const ELL: u16 = 6;
const DESCENDER: u16 = 7;

fn shapes() -> Vec<Shape> {
    let shape = |advance, contours| Shape { advance, contours };
    vec![
        shape(500, vec![]),
        shape(300, vec![]),
        shape(600, vec![rect(100, 0, 500, 700)]),
        shape(350, vec![rect(100, 0, 250, 700)]),
        shape(
            700,
            // The hole runs the other way round.
            vec![
                rect(100, 0, 600, 700),
                vec![(250, 150), (450, 150), (450, 550), (250, 550)],
            ],
        ),
        shape(600, vec![rect(100, 0, 500, 700)]),
        shape(
            600,
            vec![vec![
                (100, 0),
                (100, 700),
                (250, 700),
                (250, 150),
                (500, 150),
                (500, 0),
            ]],
        ),
        shape(600, vec![rect(100, -200, 500, 500)]),
    ]
}

fn glyph_for(c: char) -> Option<u16> {
    Some(match c {
        ' ' => SPACE,
        'I' => BAR,
        'O' => RING,
        'V' => VEE,
        'L' => ELL,
        'g' => DESCENDER,
        c if c.is_ascii_alphanumeric() => BOX,
        _ => return None,
    })
}

fn u16s(out: &mut Vec<u8>, values: &[u16]) {
    for v in values {
        out.extend(v.to_be_bytes());
    }
}

fn i16s(out: &mut Vec<u8>, values: &[i16]) {
    for v in values {
        out.extend(v.to_be_bytes());
    }
}

fn pad4(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn glyf_entry(s: &Shape) -> Vec<u8> {
    let mut out = Vec::new();
    if s.contours.is_empty() {
        return out;
    }
    let points: Vec<(i16, i16)> = s.contours.iter().flatten().copied().collect();
    let xs = points.iter().map(|p| p.0);
    let ys = points.iter().map(|p| p.1);
    i16s(
        &mut out,
        &[
            s.contours.len() as i16,
            xs.clone().min().unwrap_or(0),
            ys.clone().min().unwrap_or(0),
            xs.max().unwrap_or(0),
            ys.max().unwrap_or(0),
        ],
    );
    let mut end = 0u16;
    for c in &s.contours {
        end += c.len() as u16;
        u16s(&mut out, &[end - 1]);
    }
    // No instructions; every point on the curve, coordinates as 16-bit steps from the last.
    u16s(&mut out, &[0]);
    out.extend(std::iter::repeat_n(0x01u8, points.len()));
    let mut last = (0i16, 0i16);
    let (mut dx, mut dy) = (Vec::new(), Vec::new());
    for &p in &points {
        dx.push(p.0 - last.0);
        dy.push(p.1 - last.1);
        last = p;
    }
    i16s(&mut out, &dx);
    i16s(&mut out, &dy);
    pad4(&mut out);
    out
}

fn name_table(family: &str, style: &str) -> Vec<u8> {
    let full = if style == "Regular" {
        family.to_string()
    } else {
        format!("{family} {style}")
    };
    let strings: Vec<(u16, Vec<u8>)> = [(1, family), (2, style), (4, full.as_str()), (16, family)]
        .into_iter()
        .map(|(id, s)| (id, s.encode_utf16().flat_map(u16::to_be_bytes).collect()))
        .collect();
    let mut out = Vec::new();
    let count = strings.len() as u16;
    u16s(&mut out, &[0, count, 6 + 12 * count]);
    let mut offset = 0u16;
    for (id, bytes) in &strings {
        // Windows, Unicode, US English.
        u16s(&mut out, &[3, 1, 0x0409, *id, bytes.len() as u16, offset]);
        offset += bytes.len() as u16;
    }
    for (_, bytes) in strings {
        out.extend(bytes);
    }
    out
}

fn tables(family: &str, bold: bool, italic: bool) -> Vec<([u8; 4], Vec<u8>)> {
    let shapes = shapes();
    let count = shapes.len() as u16;

    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for s in &shapes {
        loca.extend((glyf.len() as u32).to_be_bytes());
        glyf.extend(glyf_entry(s));
    }
    loca.extend((glyf.len() as u32).to_be_bytes());

    let mut hmtx = Vec::new();
    for s in &shapes {
        u16s(&mut hmtx, &[s.advance]);
        i16s(&mut hmtx, &[100]);
    }

    let mut head = Vec::new();
    u16s(&mut head, &[1, 0]);
    head.extend(0x0001_0000u32.to_be_bytes()); // revision
    head.extend(0u32.to_be_bytes()); // checksum adjustment
    head.extend(0x5F0F_3CF5u32.to_be_bytes());
    u16s(&mut head, &[0, UNITS_PER_EM]);
    head.extend([0u8; 16]); // created, modified
    i16s(&mut head, &[0, -200, 700, 700]);
    let mac_style = u16::from(bold) | u16::from(italic) << 1;
    u16s(&mut head, &[mac_style, 8]);
    i16s(&mut head, &[2, 1, 0]); // direction hint, long offsets, glyph format

    let mut hhea = Vec::new();
    hhea.extend(0x0001_0000u32.to_be_bytes());
    i16s(&mut hhea, &[800, -200, 0]);
    u16s(&mut hhea, &[700]);
    i16s(&mut hhea, &[0, 0, 600, 1, 0, 0, 0, 0, 0, 0, 0]);
    u16s(&mut hhea, &[count]);

    let mut maxp = Vec::new();
    maxp.extend(0x0000_5000u32.to_be_bytes());
    u16s(&mut maxp, &[count]);

    // cmap: one format 12 table (Windows, full Unicode), a group per character.
    let mut map: Vec<(u32, u16)> = (0u32..128)
        .filter_map(|c| Some((c, glyph_for(char::from_u32(c)?)?)))
        .collect();
    map.sort();
    let mut cmap = Vec::new();
    u16s(&mut cmap, &[0, 1, 3, 10]);
    cmap.extend(12u32.to_be_bytes());
    u16s(&mut cmap, &[12, 0]);
    cmap.extend((16 + 12 * map.len() as u32).to_be_bytes());
    cmap.extend(0u32.to_be_bytes());
    cmap.extend((map.len() as u32).to_be_bytes());
    for (c, g) in map {
        cmap.extend(c.to_be_bytes());
        cmap.extend(c.to_be_bytes());
        cmap.extend(u32::from(g).to_be_bytes());
    }

    // kern: one horizontal pair, box letter then V, 100 units closer.
    let mut kern = Vec::new();
    u16s(&mut kern, &[0, 1, 0, 6 + 8 + 6, 0x0001, 1, 6, 0, 0, BOX, VEE]);
    i16s(&mut kern, &[-100]);

    let mut os2 = vec![0u8; 78];
    os2[4..6].copy_from_slice(&(if bold { 700u16 } else { 400 }).to_be_bytes());
    os2[6..8].copy_from_slice(&5u16.to_be_bytes());
    let selection = if !bold && !italic {
        1 << 6
    } else {
        u16::from(italic) | u16::from(bold) << 5
    };
    os2[62..64].copy_from_slice(&selection.to_be_bytes());

    let style = match (bold, italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    };
    vec![
        (*b"OS/2", os2),
        (*b"cmap", cmap),
        (*b"glyf", glyf),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"kern", kern),
        (*b"loca", loca),
        (*b"maxp", maxp),
        (*b"name", name_table(family, style)),
    ]
}

/// A font file whose table offsets count from `base` (where it starts in a collection).
fn build(family: &str, bold: bool, italic: bool, base: u32) -> Vec<u8> {
    let mut tables = tables(family, bold, italic);
    tables.sort_by_key(|t| t.0);
    let n = tables.len() as u16;
    let selector = 15 - n.leading_zeros() as u16;
    let range = (1u16 << selector) * 16;
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    u16s(&mut out, &[n, range, selector, n * 16 - range]);
    let mut offset = 12 + 16 * u32::from(n);
    for (tag, data) in &tables {
        out.extend(tag);
        out.extend(0u32.to_be_bytes());
        out.extend((base + offset).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        offset += (data.len() as u32).div_ceil(4) * 4;
    }
    for (_, data) in tables {
        out.extend(data);
        pad4(&mut out);
    }
    out
}

/// The tiny font as a `.ttf` file's bytes: family `family`, bold (weight 700) or regular, italic
/// or upright.
pub fn tiny_font(family: &str, bold: bool, italic: bool) -> Vec<u8> {
    build(family, bold, italic, 0)
}

/// Several tiny fonts as one collection (`.ttc`), each (family, bold, italic).
pub fn tiny_collection(faces: &[(&str, bool, bool)]) -> Vec<u8> {
    let header = 12 + 4 * faces.len() as u32;
    let mut fonts = Vec::new();
    let mut offsets = Vec::new();
    let mut at = header.div_ceil(4) * 4;
    for &(family, bold, italic) in faces {
        let font = build(family, bold, italic, at);
        offsets.push(at);
        at += font.len() as u32;
        fonts.push(font);
    }
    let mut out = Vec::new();
    out.extend(b"ttcf");
    out.extend(0x0001_0000u32.to_be_bytes());
    out.extend((faces.len() as u32).to_be_bytes());
    for o in offsets {
        out.extend(o.to_be_bytes());
    }
    pad4(&mut out);
    for font in fonts {
        out.extend(font);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ttf_parser::{Face, GlyphId};

    #[test]
    fn the_tiny_font_reads_as_a_font() {
        let data = tiny_font("Tiny", false, false);
        let face = Face::parse(&data, 0).unwrap();
        assert_eq!(face.units_per_em(), UNITS_PER_EM);
        assert_eq!((face.ascender(), face.descender()), (800, -200));
        let o = face.glyph_index('O').unwrap();
        assert_eq!(o, GlyphId(RING));
        assert_eq!(face.glyph_hor_advance(o), Some(700));
        let bbox = face.glyph_bounding_box(o).unwrap();
        assert_eq!(
            (bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max),
            (100, 0, 600, 700)
        );
        assert!(face.glyph_index('\u{e9}').is_none());
        let both = tiny_collection(&[("A", false, false), ("B", true, false)]);
        assert_eq!(ttf_parser::fonts_in_collection(&both), Some(2));
        assert!(Face::parse(&both, 1).unwrap().is_bold());
    }
}
