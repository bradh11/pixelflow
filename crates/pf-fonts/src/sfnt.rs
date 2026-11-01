//! A font file's tables, read for the Text effect, and its letters' outlines, drawn with skrifa.
//!
//! skrifa (Google Fonts' fontations, MIT OR Apache-2.0) reads the hard part: a TrueType letter's
//! points and curves, and a CFF letter's charstrings (subroutines, hints, flex). The rest is read
//! here from the tables' bytes: whether a face can be drawn at all, its line metrics, which glyph
//! draws a character, the advances, the kerning, the family names, and how a composite letter's
//! pieces are put together.
//!
//! All of it reads as ttf-parser 0.25 read it (the reader PixelFlow used until November 2026),
//! quirks included, so that a show draws the letters it always has (`pinned.rs` holds that to
//! the bit). Where skrifa's own answers differ, they aren't used: its character map reads symbol
//! maps and prefers the fullest map where ttf-parser took the first; its line metrics fall back
//! differently; it moves a TrueType letter to its left side bearing (undone here), places a
//! component by matching points, and rounds a scaled component's points in another order; and
//! its CFF drawing cuts points to whole font units and applies the font's matrix (so the
//! charstring is evaluated through skrifa here, and its points taken as they are).

use skrifa::font::glyph::outline::{DrawOptions, OutlineGlyph, PathContourStart, PathElement};
use skrifa::font::{Blob, Font};
use skrifa::raw::ps::charstring::CommandSink;
use skrifa::raw::types::Fixed;
use skrifa::{DrawMemory, GlyphId, GlyphSource, GlyphSourceOrder, PathSink};
use std::cmp::Ordering;
use std::sync::Arc;

fn u8_at(d: &[u8], at: usize) -> Option<u8> {
    d.get(at).copied()
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        d.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn i16_at(d: &[u8], at: usize) -> Option<i16> {
    u16_at(d, at).map(|v| v as i16)
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        d.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// `len` bytes of `d` from `at`, or `None` when they aren't all there.
fn bytes(d: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    d.get(at..at.checked_add(len)?)
}

const TRUETYPE: u32 = 0x0001_0000;
const OPENTYPE: u32 = u32::from_be_bytes(*b"OTTO");
const APPLE_TRUETYPE: u32 = u32::from_be_bytes(*b"true");
const COLLECTION: u32 = u32::from_be_bytes(*b"ttcf");

/// The tables of one face that the Text effect reads.
#[derive(Default)]
struct Tables<'a> {
    head: &'a [u8],
    hhea: &'a [u8],
    maxp: &'a [u8],
    os2: Option<&'a [u8]>,
    cmap: Option<&'a [u8]>,
    hmtx: Option<&'a [u8]>,
    kern: Option<&'a [u8]>,
    loca: Option<&'a [u8]>,
    glyf: Option<&'a [u8]>,
    cff: Option<&'a [u8]>,
}

/// One face of a font file, ready to measure and draw.
pub(crate) struct Face<'a> {
    units_per_em: u16,
    long_loca: bool,
    glyph_count: u16,
    hhea_ascender: i16,
    hhea_descender: i16,
    hhea_line_gap: i16,
    hhea_long_metrics: u16,
    os2: Option<&'a [u8]>,
    cmap: Option<&'a [u8]>,
    hmtx: Option<&'a [u8]>,
    kern: Option<&'a [u8]>,
    /// The `loca` table's offsets (2 or 4 bytes each), when they can be read.
    loca: Option<&'a [u8]>,
    glyf: Option<&'a [u8]>,
    cff: Option<&'a [u8]>,
}

impl<'a> Face<'a> {
    /// Face `index` of the font file `data` (0 for a file with one face), or `None` when it
    /// can't be drawn: no such face, an unknown kind of file, or its `head`, `hhea` or `maxp`
    /// table missing or broken.
    pub fn parse(data: &'a [u8], index: u32) -> Option<Self> {
        let tables = tables(data, index)?;
        let head = tables.head;
        if head.len() < 54 {
            return None;
        }
        let units_per_em = u16_at(head, 18)?;
        if !(16..=16384).contains(&units_per_em) {
            return None;
        }
        let long_loca = match u16_at(head, 50)? {
            0 => false,
            1 => true,
            _ => return None,
        };
        let hhea = tables.hhea;
        if hhea.len() < 36 {
            return None;
        }
        let maxp = tables.maxp;
        if !matches!(u32_at(maxp, 0)?, 0x0000_5000 | 0x0001_0000) {
            return None;
        }
        let glyph_count = u16_at(maxp, 4).filter(|&n| n > 0)?;
        let mut face = Face {
            units_per_em,
            long_loca,
            glyph_count,
            hhea_ascender: i16_at(hhea, 4)?,
            hhea_descender: i16_at(hhea, 6)?,
            hhea_line_gap: i16_at(hhea, 8)?,
            hhea_long_metrics: u16_at(hhea, 34)?,
            os2: tables.os2.filter(|t| os2_is_read(t)),
            cmap: tables
                .cmap
                .filter(|t| u16_at(t, 2).is_some_and(|n| bytes(t, 4, 8 * usize::from(n)).is_some())),
            hmtx: None,
            kern: tables.kern,
            loca: None,
            glyf: None,
            cff: tables.cff,
        };
        face.hmtx = tables.hmtx.filter(|t| face.hmtx_is_read(t));
        face.loca = tables.loca.and_then(|t| face.loca_offsets(t));
        face.glyf = face.loca.and(tables.glyf);
        Some(face)
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    // ---- Line metrics ----

    /// The 16-bit number at `at` in the OS/2 table (68, 70 and 72 are the typographic ascender,
    /// descender and line gap; 74 and 76 the Windows ascent and descent), 0 without one.
    fn os2_metric(&self, at: usize) -> i16 {
        self.os2.and_then(|t| i16_at(t, at)).unwrap_or(0)
    }

    /// Whether the font asks for its typographic line metrics (OS/2 version 4 and later).
    fn uses_typo_metrics(&self) -> bool {
        self.os2
            .is_some_and(|t| u16_at(t, 0).unwrap_or(0) >= 4 && u16_at(t, 62).unwrap_or(0) & (1 << 7) != 0)
    }

    /// How far the font's letters reach above the baseline, in font units.
    pub fn ascender(&self) -> i16 {
        if self.uses_typo_metrics() {
            return self.os2_metric(68);
        }
        let mut value = self.hhea_ascender;
        if value == 0 && self.os2.is_some() {
            value = self.os2_metric(68);
            if value == 0 {
                value = self.os2_metric(74);
            }
        }
        value
    }

    /// How far below the baseline the font's letters reach, in font units (negative).
    pub fn descender(&self) -> i16 {
        if self.uses_typo_metrics() {
            return self.os2_metric(70);
        }
        let mut value = self.hhea_descender;
        if value == 0 && self.os2.is_some() {
            value = self.os2_metric(70);
            if value == 0 {
                value = self.os2_metric(76).wrapping_neg();
            }
        }
        value
    }

    /// The gap the font asks for between lines, in font units.
    pub fn line_gap(&self) -> i16 {
        if self.uses_typo_metrics() {
            return self.os2_metric(72);
        }
        let mut value = self.hhea_line_gap;
        if (self.hhea_ascender == 0 || self.hhea_descender == 0) && self.os2.is_some() {
            value = if self.os2_metric(68) != 0 || self.os2_metric(70) != 0 {
                self.os2_metric(72)
            } else {
                0
            };
        }
        value
    }

    // ---- Character map ----

    /// The glyph for `c`: from the first of the font's Unicode character maps that has it.
    /// `None` when none does (glyph 0 then draws it).
    pub fn glyph_index(&self, c: char) -> Option<u16> {
        let cmap = self.cmap?;
        let count = u16_at(cmap, 2)?;
        let code = u32::from(c);
        for i in 0..usize::from(count) {
            // A map that can't be read ends the reading.
            let record = bytes(cmap, 4 + 8 * i, 8)?;
            let platform = u16_at(record, 0)?;
            if platform > 4 {
                return None;
            }
            let encoding = u16_at(record, 2)?;
            let data = cmap.get(u32_at(record, 4)? as usize..)?;
            let map = CharMap::parse(data)?;
            let unicode = match (platform, encoding) {
                (0, _) | (3, 1) => true,
                (3, 10) => matches!(map, CharMap::Groups { .. }),
                _ => false,
            };
            if unicode && let Some(glyph) = map.glyph(code) {
                return Some(glyph);
            }
        }
        None
    }

    // ---- Advances ----

    /// Whether the `hmtx` table can be read: an advance for each of the first glyphs the
    /// horizontal header counts.
    fn hmtx_is_read(&self, hmtx: &[u8]) -> bool {
        self.hhea_long_metrics != 0 && bytes(hmtx, 0, 4 * usize::from(self.hhea_long_metrics)).is_some()
    }

    /// How far the next letter starts from where `glyph` does, in font units. Glyphs past those
    /// with an advance of their own share the last one.
    pub fn advance(&self, glyph: u16) -> Option<u16> {
        let hmtx = self.hmtx?;
        let long = self.hhea_long_metrics;
        let counted = long.max(self.glyph_count);
        if glyph >= counted {
            return None;
        }
        u16_at(hmtx, 4 * usize::from(glyph.min(long - 1)))
    }

    // ---- Kerning ----

    /// How much closer (negative) or further apart the font sets `right` after `left`, in font
    /// units: from the first horizontal, non-variable subtable of its `kern` table that has the
    /// pair.
    pub fn kerning(&self, left: u16, right: u16) -> Option<i16> {
        let kern = self.kern?;
        let version = u16_at(kern, 0)?;
        let (apple, count, mut at) = if version == 0 {
            (false, u32::from(u16_at(kern, 2)?), 4)
        } else {
            (true, u32_at(kern, 4)?, 8)
        };
        if at > kern.len() || count == 0 {
            return None;
        }
        // Subtables are read until the table ends or one can't be read; the count says only
        // whether there are any, and (in an OpenType table) whether the one runs to the end.
        while at < kern.len() {
            let (horizontal, variable, format, data, header);
            if apple {
                let len = u32_at(kern, at)? as usize;
                let coverage = u8_at(kern, at + 4)?;
                format = u8_at(kern, at + 5)?;
                if format > 3 {
                    return None;
                }
                header = 8;
                data = bytes(kern, at + 8, len.checked_sub(8)?)?;
                at += len;
                horizontal = coverage & 0x80 == 0;
                variable = coverage & 0x20 != 0;
            } else {
                let len = usize::from(u16_at(kern, at + 2)?);
                format = u8_at(kern, at + 4)?;
                let coverage = u8_at(kern, at + 5)?;
                if format != 0 && format != 2 {
                    return None;
                }
                header = 6;
                let data_len = if count == 1 {
                    kern.len().checked_sub(at + 6)?
                } else {
                    len.checked_sub(6)?
                };
                data = bytes(kern, at + 6, data_len)?;
                at += 6 + data_len;
                horizontal = coverage & 0x01 != 0;
                variable = false;
            }
            let value = match format {
                0 => {
                    let pairs = bytes(data, 8, 6 * usize::from(u16_at(data, 0)?))?;
                    (horizontal && !variable)
                        .then(|| kern_pair(pairs, left, right))
                        .flatten()
                }
                2 => (horizontal && !variable)
                    .then(|| kern_classes(data, header, left, right))
                    .flatten(),
                3 => (horizontal && !variable)
                    .then(|| kern_compact(data, left, right))
                    .flatten(),
                // A state machine: no pairs.
                _ => None,
            };
            if value.is_some() {
                return value;
            }
        }
        None
    }

    // ---- Outlines ----

    /// The `loca` table's offsets, as many as there are glyphs and the table holds.
    fn loca_offsets(&self, loca: &'a [u8]) -> Option<&'a [u8]> {
        let size = if self.long_loca { 4 } else { 2 };
        let wanted = if self.glyph_count == u16::MAX {
            u16::MAX
        } else {
            self.glyph_count + 1
        };
        let held = u16::try_from(loca.len() / size).ok()?;
        bytes(loca, 0, usize::from(wanted.min(held)) * size)
    }

    /// Where `glyph`'s outline is in the `glyf` table: `None` for an empty glyph or one the
    /// table doesn't have.
    fn glyph_data(&self, glyph: u16) -> Option<&'a [u8]> {
        let (loca, glyf) = (self.loca?, self.glyf?);
        let size = if self.long_loca { 4 } else { 2 };
        let count = loca.len() / size;
        if glyph == u16::MAX || usize::from(glyph) + 1 >= count {
            return None;
        }
        let offset = |i: usize| -> Option<usize> {
            if self.long_loca {
                u32_at(loca, 4 * i).map(|v| v as usize)
            } else {
                u16_at(loca, 2 * i).map(|v| usize::from(v) * 2)
            }
        };
        let (start, end) = (offset(usize::from(glyph))?, offset(usize::from(glyph) + 1)?);
        if start >= end {
            return None;
        }
        glyf.get(start..end)
    }

    /// Draws `glyph`'s outline in font units (y up) into `pen`, with `font` (this face, as skrifa
    /// reads it) drawing its curves. Returns `false`, having drawn nothing, when the glyph has
    /// no outline or it can't be read: such a letter has only its advance.
    pub fn outline(&self, font: Option<&Font>, glyph: u16, pen: &mut impl PathSink) -> bool {
        let mut path = Path::default();
        let drawn = if self.glyf.is_some() {
            font.and_then(|font| self.glyf_outline(font, glyph, Transform::IDENTITY, 0, &mut path))
                .is_some()
        } else if self.cff.is_some() {
            font.is_some_and(|font| cff_outline(font, glyph, &mut path))
        } else {
            false
        };
        if !drawn || !path.fits() {
            return false;
        }
        for el in path.elements {
            match el {
                PathElement::MoveTo { x, y } => pen.move_to(x, y),
                PathElement::LineTo { x, y } => pen.line_to(x, y),
                PathElement::QuadTo { cx0, cy0, x, y } => pen.quad_to(cx0, cy0, x, y),
                PathElement::CurveTo {
                    cx0,
                    cy0,
                    cx1,
                    cy1,
                    x,
                    y,
                } => pen.curve_to(cx0, cy0, cx1, cy1, x, y),
                PathElement::Close => pen.close(),
            }
        }
        true
    }

    /// Adds a TrueType glyph to `path`, its points moved by `transform`: a simple glyph drawn by
    /// skrifa, a composite one piece by piece. `None` when it can't be read (nothing of the
    /// letter is then drawn).
    fn glyf_outline(
        &self,
        font: &Font,
        glyph: u16,
        transform: Transform,
        depth: u8,
        path: &mut Path,
    ) -> Option<()> {
        // Composites nest at most this deep.
        if depth >= 32 {
            return None;
        }
        let data = self.glyph_data(glyph)?;
        let contours = i16_at(data, 0)?;
        if contours > 0 {
            // A glyph of a single point draws nothing.
            if u16_at(data.get(10..)?, 2 * (contours as usize - 1)) == Some(0) {
                return Some(());
            }
            let glyphs = font.outline_glyphs_with(GlyphSourceOrder::new(&[GlyphSource::Glyf]));
            let outline = glyphs.get(GlyphId::new(u32::from(glyph)))?;
            let mut drawn = Vec::new();
            let options = DrawOptions::new().with_contour_start(PathContourStart::ScanForward);
            let metrics = outline.draw(&options, DrawMemory::Scratch, &mut drawn).ok()?;
            // skrifa moves the outline so it starts at the glyph's left side bearing; it is
            // drawn where its points are.
            let shift = metrics.x_bearing.unwrap_or(0.0);
            for el in drawn {
                path.push(el, shift, &transform);
            }
        } else if contours < 0 {
            for part in Components::new(data.get(10..)?) {
                if self.glyph_data(part.glyph).is_some() {
                    let combined = transform.then(&part.transform);
                    self.glyf_outline(font, part.glyph, combined, depth + 1, path)?;
                }
            }
        }
        Some(())
    }
}

/// The font file `data`'s face `index` as skrifa reads it, for drawing outlines.
pub(crate) fn outline_font(data: &Arc<[u8]>, index: u32) -> Option<Font> {
    let shared: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(data.clone());
    Font::new(Blob::from(shared), index)
}

/// Draws CFF glyph `glyph` into `path`. `false` when it can't be read.
///
/// The charstring is evaluated by skrifa and its points taken as it gives them, in 16.16:
/// skrifa's own drawing cuts them to whole font units and applies the font's matrix, as FreeType
/// does, where the letters were drawn with their fractions and without the matrix. ttf-parser
/// added a charstring's numbers up in `f32`; the 16.16 sums are the same wherever `f32` holds
/// them exactly (whole font units, as fonts store them, and short fractions), and can differ in
/// the last bit where a long fraction meets a large coordinate.
fn cff_outline(font: &Font, glyph: u16, path: &mut Path) -> bool {
    let glyphs = font.outline_glyphs_with(GlyphSourceOrder::new(&[GlyphSource::Cff]));
    let Some(OutlineGlyph::Cff(cff)) = glyphs.get(GlyphId::new(u32::from(glyph))) else {
        return false;
    };
    let mut drawn = Charstring(Vec::new());
    let evaluated =
        cff.cff_font()
            .evaluate_charstring(cff.subfont(), cff.glyph(), cff.normalized_coords(), &mut drawn);
    if evaluated.is_err() {
        return false;
    }
    for el in drawn.0 {
        path.push(el, 0.0, &Transform::IDENTITY);
    }
    true
}

/// Collects a charstring's path, its 16.16 points as `f32`.
struct Charstring(Vec<PathElement>);

impl CommandSink for Charstring {
    fn move_to(&mut self, x: Fixed, y: Fixed) {
        let (x, y) = (x.to_f32(), y.to_f32());
        self.0.push(PathElement::MoveTo { x, y });
    }

    fn line_to(&mut self, x: Fixed, y: Fixed) {
        let (x, y) = (x.to_f32(), y.to_f32());
        self.0.push(PathElement::LineTo { x, y });
    }

    fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed) {
        self.0.push(PathElement::CurveTo {
            cx0: cx0.to_f32(),
            cy0: cy0.to_f32(),
            cx1: cx1.to_f32(),
            cy1: cy1.to_f32(),
            x: x.to_f32(),
            y: y.to_f32(),
        });
    }

    fn close(&mut self) {
        self.0.push(PathElement::Close);
    }
}

/// The face `index` of `data`: its tables, found by tag. A tag listed twice is the later one.
fn tables(data: &[u8], index: u32) -> Option<Tables<'_>> {
    let magic = u32_at(data, 0)?;
    let directory = if magic == COLLECTION {
        let count = u32_at(data, 8)?;
        let offsets = bytes(data, 12, 4 * count as usize)?;
        if index >= count {
            return None;
        }
        let at = u32_at(offsets, 4 * index as usize)? as usize;
        // A face can't start inside the collection's header, or past the file's end.
        if at < 12 + offsets.len() || at > data.len() {
            return None;
        }
        if !matches!(u32_at(data, at)?, TRUETYPE | OPENTYPE | APPLE_TRUETYPE) {
            return None;
        }
        at + 4
    } else {
        if !matches!(magic, TRUETYPE | OPENTYPE | APPLE_TRUETYPE) || index != 0 {
            return None;
        }
        4
    };
    let count = usize::from(u16_at(data, directory)?);
    let records = bytes(data, directory + 8, 16 * count)?;
    let mut tables = Tables::default();
    for record in records.as_chunks::<16>().0 {
        let at = u32_at(record, 8)? as usize;
        let len = u32_at(record, 12)? as usize;
        let Some(end) = at.checked_add(len) else {
            continue;
        };
        let table = data.get(at..end);
        match &record[..4] {
            b"head" => tables.head = table.unwrap_or_default(),
            b"hhea" => tables.hhea = table.unwrap_or_default(),
            b"maxp" => tables.maxp = table.unwrap_or_default(),
            b"OS/2" => tables.os2 = table,
            b"cmap" => tables.cmap = table,
            b"hmtx" => tables.hmtx = table,
            b"kern" => tables.kern = table,
            b"loca" => tables.loca = table,
            b"glyf" => tables.glyf = table,
            b"CFF " => tables.cff = table,
            _ => {}
        }
    }
    Some(tables)
}

/// Whether an OS/2 table can be read: a known version, long enough for it.
fn os2_is_read(os2: &[u8]) -> bool {
    let len = match u16_at(os2, 0) {
        Some(0) => 78,
        Some(1) => 86,
        Some(2..=4) => 96,
        Some(5) => 100,
        _ => return false,
    };
    os2.len() >= len
}

// ---- Character maps ----

/// One character map subtable, of a format that maps single characters.
enum CharMap<'a> {
    /// Format 0: a glyph for each byte.
    Bytes(&'a [u8]),
    /// Format 2: high bytes to subheaders.
    HighBytes {
        data: &'a [u8],
        keys: &'a [u8],
        headers: &'a [u8],
    },
    /// Format 4: segments of the Basic Multilingual Plane.
    Segments {
        data: &'a [u8],
        ends: &'a [u8],
        starts: &'a [u8],
        deltas: &'a [u8],
        offsets: &'a [u8],
        offsets_at: usize,
    },
    /// Formats 6 and 10: a run of codes from `first`.
    Run { first: u32, glyphs: &'a [u8], bmp: bool },
    /// Format 12 (`same` false: each code the next glyph) and 13 (every code one glyph).
    Groups { groups: &'a [u8], same: bool },
    /// Formats 8 and 14, which map no single character.
    None,
}

impl<'a> CharMap<'a> {
    /// The subtable at the start of `data`, `None` when it can't be read.
    fn parse(data: &'a [u8]) -> Option<Self> {
        Some(match u16_at(data, 0)? {
            0 => CharMap::Bytes(bytes(data, 6, 256)?),
            2 => {
                let keys = bytes(data, 6, 512)?;
                let most = keys.as_chunks::<2>().0.iter().map(|k| u16::from_be_bytes(*k) / 8);
                let headers = usize::from(most.max()?) + 1;
                CharMap::HighBytes {
                    data,
                    keys,
                    headers: bytes(data, 518, 8 * headers)?,
                }
            }
            4 => {
                let doubled = u16_at(data, 6)?;
                if doubled < 2 {
                    return None;
                }
                let n = 2 * usize::from(doubled / 2);
                let offsets_at = 16 + 3 * n;
                CharMap::Segments {
                    data,
                    ends: bytes(data, 14, n)?,
                    starts: bytes(data, 16 + n, n)?,
                    deltas: bytes(data, 16 + 2 * n, n)?,
                    offsets: bytes(data, offsets_at, n)?,
                    offsets_at,
                }
            }
            6 => {
                let count = usize::from(u16_at(data, 8)?);
                CharMap::Run {
                    first: u32::from(u16_at(data, 6)?),
                    glyphs: bytes(data, 10, 2 * count)?,
                    bmp: true,
                }
            }
            8 => CharMap::None,
            10 => {
                let first = u32_at(data, 12)?;
                let count = u32_at(data, 16)? as usize;
                CharMap::Run {
                    first,
                    glyphs: bytes(data, 20, 2 * count)?,
                    bmp: false,
                }
            }
            format @ (12 | 13) => {
                let count = u32_at(data, 12)? as usize;
                CharMap::Groups {
                    groups: bytes(data, 16, 12 * count)?,
                    same: format == 13,
                }
            }
            14 => {
                let count = u32_at(data, 6)? as usize;
                bytes(data, 10, 11 * count)?;
                CharMap::None
            }
            _ => return None,
        })
    }

    fn glyph(&self, code: u32) -> Option<u16> {
        match *self {
            CharMap::Bytes(glyphs) => {
                let g = *glyphs.get(code as usize)?;
                (g != 0).then_some(u16::from(g))
            }
            CharMap::HighBytes { data, keys, headers } => {
                let code = u16::try_from(code).ok()?;
                let (high, low) = (code >> 8, code & 0xFF);
                let i = if code < 0xFF {
                    0
                } else {
                    u16_at(keys, 2 * usize::from(high))? / 8
                };
                let header = bytes(headers, 8 * usize::from(i), 8)?;
                let first = u16_at(header, 0)?;
                let end = first.checked_add(u16_at(header, 2)?)?;
                if low < first || low >= end {
                    return None;
                }
                let delta = i16_at(header, 4)?;
                let range_offset = usize::from(u16_at(header, 6)?);
                let at = 518 + 8 * (usize::from(i) + 1) - 2 + range_offset + 2 * usize::from(low - first);
                let g = u16_at(data, at)?;
                if g == 0 {
                    return None;
                }
                u16::try_from((i32::from(g) + i32::from(delta)) % 65536).ok()
            }
            CharMap::Segments {
                data,
                ends,
                starts,
                deltas,
                offsets,
                offsets_at,
            } => {
                let code = u16::try_from(code).ok()?;
                let (mut lo, mut hi) = (0, starts.len() / 2);
                while hi > lo {
                    let i = (lo + hi) / 2;
                    if u16_at(ends, 2 * i)? < code {
                        lo = i + 1;
                        continue;
                    }
                    let start = u16_at(starts, 2 * i)?;
                    if start > code {
                        hi = i;
                        continue;
                    }
                    let range_offset = u16_at(offsets, 2 * i)?;
                    let delta = i16_at(deltas, 2 * i)?;
                    if range_offset == 0 {
                        return Some(code.wrapping_add(delta as u16));
                    } else if range_offset == 0xFFFF {
                        return None;
                    }
                    let step = u16::try_from((u32::from(code) - u32::from(start)) * 2).ok()?;
                    // Positions wrap at 16 bits, as they were read before.
                    let at = ((offsets_at + 2 * i) as u16)
                        .wrapping_add(step)
                        .wrapping_add(range_offset);
                    let g = u16_at(data, usize::from(at))?;
                    if g == 0 {
                        return None;
                    }
                    return u16::try_from((g as i16).wrapping_add(delta)).ok();
                }
                None
            }
            CharMap::Run { first, glyphs, bmp } => {
                if bmp && code > 0xFFFF {
                    return None;
                }
                let i = code.checked_sub(first)?;
                if bmp && i > 0xFFFF {
                    return None;
                }
                u16_at(glyphs, 2 * usize::try_from(i).ok()?)
            }
            CharMap::Groups { groups, same } => {
                let group = |i: usize| -> Option<(u32, u32, u32)> {
                    let g = bytes(groups, 12 * i, 12)?;
                    Some((u32_at(g, 0)?, u32_at(g, 4)?, u32_at(g, 8)?))
                };
                let count = groups.len() / 12;
                if same {
                    for i in 0..count {
                        let (start, end, glyph) = group(i)?;
                        if code >= start && code <= end {
                            return u16::try_from(glyph).ok();
                        }
                    }
                    return None;
                }
                let (_, (start, _, glyph)) = search(count, group, |&(start, end, _)| {
                    if start > code {
                        Ordering::Greater
                    } else if end < code {
                        Ordering::Less
                    } else {
                        Ordering::Equal
                    }
                })?;
                u16::try_from(glyph.checked_add(code)?.checked_sub(start)?).ok()
            }
            CharMap::None => None,
        }
    }
}

/// A binary search over `count` items: the last one not past what `compare` looks for, if it is
/// the one.
fn search<T>(
    count: usize,
    item: impl Fn(usize) -> Option<T>,
    compare: impl Fn(&T) -> Ordering,
) -> Option<(usize, T)> {
    let mut size = count;
    if size == 0 {
        return None;
    }
    let mut base = 0;
    while size > 1 {
        let half = size / 2;
        let mid = base + half;
        if compare(&item(mid)?) != Ordering::Greater {
            base = mid;
        }
        size -= half;
    }
    let found = item(base)?;
    (compare(&found) == Ordering::Equal).then_some((base, found))
}

// ---- Kerning ----

/// Kerning format 0: sorted pairs of glyphs, each with its value.
fn kern_pair(pairs: &[u8], left: u16, right: u16) -> Option<i16> {
    let wanted = (u32::from(left) << 16) | u32::from(right);
    let pair = |i: usize| -> Option<(u32, i16)> { Some((u32_at(pairs, 6 * i)?, i16_at(pairs, 6 * i + 4)?)) };
    search(pairs.len() / 6, pair, |p| p.0.cmp(&wanted)).map(|(_, p)| p.1)
}

/// Kerning format 2: each glyph's class, and a value for each two classes. Offsets count from
/// the subtable's start, `header` bytes before `data`.
fn kern_classes(data: &[u8], header: usize, left: u16, right: u16) -> Option<i16> {
    let left_table = usize::from(u16_at(data, 2)?).checked_sub(header)?;
    let right_table = usize::from(u16_at(data, 4)?).checked_sub(header)?;
    let array = usize::from(u16_at(data, 6)?).checked_sub(header)?;
    let class = |glyph: u16, at: usize| -> Option<u16> {
        let first = u16_at(data, at)?;
        let i = glyph.checked_sub(first)?;
        let count = u16_at(data, at + 2)?;
        let classes = bytes(data, at + 4, 2 * usize::from(count))?;
        u16_at(classes, 2 * usize::from(i))
    };
    let left_class = class(left, left_table).unwrap_or(0);
    let right_class = class(right, right_table).unwrap_or(0);
    if usize::from(left_class) < array {
        return None;
    }
    let at = (usize::from(left_class) + usize::from(right_class)).checked_sub(header)?;
    i16_at(data, at)
}

/// Kerning format 3 (Apple's): each glyph's left and right class, an index for each two
/// classes, and the values the indexes pick.
fn kern_compact(data: &[u8], left: u16, right: u16) -> Option<i16> {
    let glyphs = usize::from(u16_at(data, 0)?);
    let values = usize::from(u8_at(data, 2)?);
    let (lefts, rights) = (u8_at(data, 3)?, u8_at(data, 4)?);
    let values = bytes(data, 6, 2 * values)?;
    let left_classes = bytes(data, 6 + values.len(), glyphs)?;
    let right_classes = bytes(data, 6 + values.len() + glyphs, glyphs)?;
    let indexes = bytes(
        data,
        6 + values.len() + 2 * glyphs,
        usize::from(lefts) * usize::from(rights),
    )?;
    let left_class = *left_classes.get(usize::from(left))?;
    let right_class = *right_classes.get(usize::from(right))?;
    if left_class > lefts || right_class > rights {
        return None;
    }
    let index = u16::from(left_class) * u16::from(rights) + u16::from(right_class);
    let value = *indexes.get(usize::from(index))?;
    i16_at(values, 2 * usize::from(value))
}

// ---- Composite letters ----

/// How a composite letter's piece is moved: x' = a x + c y + e, y' = b x + d y + f.
#[derive(Clone, Copy, PartialEq)]
struct Transform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Transform {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// This transform after `inner` (a piece's own, inside this one).
    fn then(&self, inner: &Self) -> Self {
        Self {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
}

/// One piece of a composite letter.
struct Component {
    glyph: u16,
    transform: Transform,
}

/// The pieces of a composite letter, from its data after the header.
///
/// A piece placed by matching points (rather than moved by an offset) is read as before: its
/// two point numbers aren't skipped, so what follows them is read from there.
struct Components<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Components<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn read_u16(&mut self) -> Option<u16> {
        let v = u16_at(self.data, self.at)?;
        self.at += 2;
        Some(v)
    }

    fn read_i16(&mut self) -> Option<f32> {
        self.read_u16().map(|v| f32::from(v as i16))
    }

    fn read_i8(&mut self) -> Option<f32> {
        let v = u8_at(self.data, self.at)?;
        self.at += 1;
        Some(f32::from(v as i8))
    }

    /// A 2.14 number.
    fn read_f2dot14(&mut self) -> Option<f32> {
        self.read_u16().map(|v| f32::from(v as i16) / 16384.0)
    }
}

impl Iterator for Components<'_> {
    type Item = Component;

    fn next(&mut self) -> Option<Component> {
        let flags = self.read_u16()?;
        let glyph = self.read_u16()?;
        let mut t = Transform::IDENTITY;
        if flags & 0x0002 != 0 {
            if flags & 0x0001 != 0 {
                t.e = self.read_i16()?;
                t.f = self.read_i16()?;
            } else {
                t.e = self.read_i8()?;
                t.f = self.read_i8()?;
            }
        }
        if flags & 0x0080 != 0 {
            t.a = self.read_f2dot14()?;
            t.b = self.read_f2dot14()?;
            t.c = self.read_f2dot14()?;
            t.d = self.read_f2dot14()?;
        } else if flags & 0x0040 != 0 {
            t.a = self.read_f2dot14()?;
            t.d = self.read_f2dot14()?;
        } else if flags & 0x0008 != 0 {
            t.a = self.read_f2dot14()?;
            t.d = t.a;
        }
        if flags & 0x0020 == 0 {
            // No more pieces after this one.
            self.at = self.data.len();
        }
        Some(Component { glyph, transform: t })
    }
}

// ---- Collected outlines ----

/// An outline collected before it is drawn, and the box around its points (control points
/// included).
#[derive(Default)]
struct Path {
    elements: Vec<PathElement>,
    bounds: Option<[f32; 4]>,
}

impl Path {
    /// Adds `el`, moved right by `shift` and then by `transform`.
    fn push(&mut self, el: PathElement, shift: f32, transform: &Transform) {
        let identity = *transform == Transform::IDENTITY;
        let mut point = |x: f32, y: f32| {
            let x = x + shift;
            let (x, y) = if identity { (x, y) } else { transform.apply(x, y) };
            let b = self.bounds.get_or_insert([x, y, x, y]);
            *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            (x, y)
        };
        let el = match el {
            PathElement::MoveTo { x, y } => {
                let (x, y) = point(x, y);
                PathElement::MoveTo { x, y }
            }
            PathElement::LineTo { x, y } => {
                let (x, y) = point(x, y);
                PathElement::LineTo { x, y }
            }
            PathElement::QuadTo { cx0, cy0, x, y } => {
                let (cx0, cy0) = point(cx0, cy0);
                let (x, y) = point(x, y);
                PathElement::QuadTo { cx0, cy0, x, y }
            }
            PathElement::CurveTo {
                cx0,
                cy0,
                cx1,
                cy1,
                x,
                y,
            } => {
                let (cx0, cy0) = point(cx0, cy0);
                let (cx1, cy1) = point(cx1, cy1);
                let (x, y) = point(x, y);
                PathElement::CurveTo {
                    cx0,
                    cy0,
                    cx1,
                    cy1,
                    x,
                    y,
                }
            }
            PathElement::Close => PathElement::Close,
        };
        self.elements.push(el);
    }

    /// Whether the outline has points, all within 16-bit font units (a letter that reaches
    /// further isn't drawn).
    fn fits(&self) -> bool {
        let fits = |v: f32| v >= i32::MIN as f32 && v < i32::MAX as f32 && i16::try_from(v as i32).is_ok();
        self.bounds.is_some_and(|b| b.iter().all(|&v| fits(v)))
    }
}

// ---- Names ----

/// One name in a font's `name` table.
pub(crate) struct Name<'a> {
    pub platform: u16,
    pub encoding: u16,
    pub language: u16,
    pub id: u16,
    pub bytes: &'a [u8],
}

impl Name<'_> {
    /// The name's text, for names in UTF-16: Unicode's, and Windows' symbol and BMP encodings.
    /// `None` for other encodings, or text that isn't UTF-16.
    pub fn utf16(&self) -> Option<String> {
        let unicode = match self.platform {
            0 => true,
            3 => matches!(self.encoding, 0 | 1),
            _ => false,
        };
        if !unicode {
            return None;
        }
        let units: Vec<u16> = self
            .bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|u| u16::from_be_bytes(*u))
            .collect();
        String::from_utf16(&units).ok()
    }
}

/// The names in a `name` table, in the order stored, until one can't be read.
///
/// A version 1 table (with language tags) is read as before: the count of tags is taken from
/// where the first name's record starts, and the records from after that many tags.
pub(crate) fn names(table: &[u8]) -> impl Iterator<Item = Name<'_>> {
    let parsed = (|| {
        let count = usize::from(u16_at(table, 2)?);
        let storage_at = usize::from(u16_at(table, 4)?);
        let records_at = match u16_at(table, 0)? {
            0 => 6,
            1 => 8 + usize::from(u16_at(table, 6)?.checked_mul(4)?),
            _ => return None,
        };
        let records = bytes(table, records_at, 12 * count)?;
        let storage = table.get((records_at + records.len()).max(storage_at)..)?;
        Some((records, storage))
    })();
    let (records, storage) = parsed.unwrap_or_default();
    records
        .as_chunks::<12>()
        .0
        .iter()
        .map(move |r| {
            let platform = u16_at(r, 0)?;
            if platform > 4 {
                return None;
            }
            let at = usize::from(u16_at(r, 10)?);
            Some(Name {
                platform,
                encoding: u16_at(r, 2)?,
                language: u16_at(r, 4)?,
                id: u16_at(r, 6)?,
                bytes: storage.get(at..at + usize::from(u16_at(r, 8)?))?,
            })
        })
        .map_while(|n| n)
}
