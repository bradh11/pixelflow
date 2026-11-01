//! A font face at one size, with its letters drawn once each and kept for every frame after.
//!
//! skrifa reads the face: whether it can be read at all, which glyph draws each character, the
//! line metrics, the advances, the kerning, and each letter's outline (TrueType, CFF or CFF2,
//! composite letters put together and placed as FreeType and macOS place them). PixelFlow's
//! own rasterizer (`raster.rs`) draws the outlines.

use crate::find::{FaceInfo, Source};
use crate::raster::{self, Outline};
use skrifa::font::glyph::outline::DrawOptions;
use skrifa::font::{Blob, Font};
use skrifa::raw::TableProvider;
use skrifa::raw::tables::kern::SubtableKind;
use skrifa::{DrawMemory, GlyphId};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// How far a made-up italic leans: right by this much per pixel of height (about 11 degrees,
/// as obliques usually do).
const SLANT: f32 = 0.2;
/// How much a made-up bold thickens each stroke, as a share of the size (the letter is drawn
/// twice, this far apart).
const EMBOLDEN: f32 = 1.0 / 24.0;

/// One letter of a [`SizedFont`], drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    id: GlyphId,
    /// How far the next letter starts from where this one does, in pixels.
    pub advance: f32,
    /// The drawing's top left, in pixels from where the letter starts on its baseline: right,
    /// and down (so `top` is usually negative).
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    coverage: Box<[u8]>,
}

impl Glyph {
    /// How much of the pixel `x` across and `y` down from the drawing's top left the letter
    /// covers: 0 (none) to 255 (all of it).
    #[inline]
    pub fn coverage(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            0
        } else {
            self.coverage[(y * self.width + x) as usize]
        }
    }
}

/// A font face at one size: its measurements, and its letters, each drawn the first time it's
/// asked for.
pub struct SizedFont {
    font: Font,
    /// Pixels per font unit.
    scale: f32,
    /// How far apart a made-up bold's two drawings are, in pixels (0 for a real face).
    embolden: f32,
    /// A made-up italic's lean (0 for a real face).
    slant: f32,
    /// How far the font's letters reach above the baseline, and below it, in pixels.
    pub ascent: f32,
    pub descent: f32,
    /// The gap the font asks for between lines, in pixels.
    pub line_gap: f32,
    glyphs: RwLock<HashMap<char, Arc<Glyph>>>,
    kerning: RwLock<HashMap<(GlyphId, GlyphId), f32>>,
}

impl fmt::Debug for SizedFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SizedFont")
            .field("scale", &self.scale)
            .field("embolden", &self.embolden)
            .field("slant", &self.slant)
            .field("ascent", &self.ascent)
            .field("descent", &self.descent)
            .field("line_gap", &self.line_gap)
            .finish_non_exhaustive()
    }
}

impl SizedFont {
    fn new(font: Font, size: f32, fake_bold: bool, fake_italic: bool) -> Self {
        let scale = size / f32::from(font.units_per_em().max(1));
        // The line skrifa picks: the typographic one when the font asks for it, else the
        // horizontal header's, the typographic one, or the Windows clipping line.
        let line = font.metrics().h_line();
        let (ascent, descent, line_gap) = line.map_or((0.0, 0.0, 0.0), |l| {
            (
                l.ascender.to_f32() * scale,
                -l.descender.to_f32() * scale,
                l.line_gap.to_f32() * scale,
            )
        });
        Self {
            embolden: if fake_bold { size * EMBOLDEN } else { 0.0 },
            slant: if fake_italic { SLANT } else { 0.0 },
            scale,
            ascent,
            descent,
            line_gap,
            glyphs: RwLock::default(),
            kerning: RwLock::default(),
            font,
        }
    }

    /// The letter `c`, drawn at this size (the font's box for a missing letter). Drawn the first
    /// time and kept: any thread can ask.
    pub fn glyph(&self, c: char) -> Arc<Glyph> {
        if let Some(g) = self.glyphs.read().unwrap_or_else(|e| e.into_inner()).get(&c) {
            return g.clone();
        }
        let drawn = Arc::new(self.draw(c));
        self.glyphs
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .entry(c)
            .or_insert(drawn)
            .clone()
    }

    fn draw(&self, c: char) -> Glyph {
        let id = self.font.charmap().map_unicode(c).unwrap_or(GlyphId::NOTDEF);
        let advance = self.font.glyph_metrics().h_advance(id) * self.scale + self.embolden;
        let empty = Glyph {
            id,
            advance,
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            coverage: Box::new([]),
        };
        // In font units, as skrifa draws it unhinted; scaled (and slanted) as it is collected.
        let mut outline = Outline::new(self.scale, self.slant);
        let drawn = self.font.outline_glyphs().get(id).is_some_and(|glyph| {
            glyph
                .draw(&DrawOptions::new(), DrawMemory::Scratch, &mut outline)
                .is_ok()
        });
        if !drawn {
            return empty;
        }
        let mut lines = outline.lines;
        if self.embolden > 0.0 {
            // A made-up bold: the letter twice, side by side, filled as one shape.
            let shifted: Vec<_> = lines
                .iter()
                .map(|l| [l[0] + self.embolden, l[1], l[2] + self.embolden, l[3]])
                .collect();
            lines.extend(shifted);
        }
        let bitmap = raster::fill(&lines);
        Glyph {
            left: bitmap.left,
            top: bitmap.top,
            width: bitmap.width,
            height: bitmap.height,
            coverage: bitmap.coverage.into_boxed_slice(),
            ..empty
        }
    }

    /// How much closer (negative) or further apart the font sets `right` after `left`, in
    /// pixels, from its kerning table (0 when it has none).
    pub fn kerning(&self, left: &Glyph, right: &Glyph) -> f32 {
        let pair = (left.id, right.id);
        if let Some(&k) = self.kerning.read().unwrap_or_else(|e| e.into_inner()).get(&pair) {
            return k;
        }
        let k = kern(&self.font, pair.0, pair.1).map_or(0.0, |units| units as f32 * self.scale);
        self.kerning
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pair, k);
        k
    }
}

/// The `kern` table's value for a pair, in font units: from the first horizontal subtable that
/// has the pair and doesn't vary.
fn kern(font: &Font, left: GlyphId, right: GlyphId) -> Option<i32> {
    let kern = font.tables().kern().ok()?;
    kern.subtables()
        .filter_map(Result::ok)
        .filter(|s| s.is_horizontal() && !s.is_variable())
        .find_map(|s| match s.kind().ok()? {
            SubtableKind::Format0(t) => t.kerning(left, right),
            SubtableKind::Format2(t) => t.kerning(left, right),
            SubtableKind::Format3(t) => t.kerning(left, right),
            // A state machine: no pairs.
            SubtableKind::Format1(_) => None,
        })
}

/// Which font data a face is, for sharing it between sizes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DataKey {
    File(PathBuf),
    Memory(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SizeKey {
    data: DataKey,
    index: u32,
    size: u32,
    fake_bold: bool,
    fake_italic: bool,
}

/// Every font file read so far, kept for the run (a face is read once whatever its sizes).
fn data(source: &Source) -> Option<(DataKey, Arc<[u8]>)> {
    static FILES: OnceLock<Mutex<HashMap<PathBuf, Arc<[u8]>>>> = OnceLock::new();
    match source {
        Source::Memory(data) => Some((
            DataKey::Memory(Arc::as_ptr(data) as *const u8 as usize),
            data.clone(),
        )),
        Source::File(path) => {
            let mut files = FILES
                .get_or_init(Mutex::default)
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(data) = files.get(path) {
                return Some((DataKey::File(path.clone()), data.clone()));
            }
            let data: Arc<[u8]> = Arc::from(std::fs::read(path).ok()?);
            files.insert(path.clone(), data.clone());
            Some((DataKey::File(path.clone()), data))
        }
    }
}

/// Face `index` of the font file `data`, as skrifa reads it; `None` when it can't.
pub(crate) fn read(data: &Arc<[u8]>, index: u32) -> Option<Font> {
    let shared: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(data.clone());
    Font::new(Blob::from(shared), index)
}

/// The characters of `text` that `face` has no letter for (line breaks aside); `None` when its
/// file can't be read.
pub(crate) fn missing_letters(face: &FaceInfo, text: &str) -> Option<Vec<char>> {
    let (_, bytes) = data(&face.source)?;
    let font = read(&bytes, face.index)?;
    let charmap = font.charmap();
    let mut missing: Vec<char> = Vec::new();
    for c in text.chars().filter(|c| !c.is_control()) {
        if charmap.map_unicode(c).is_none() && !missing.contains(&c) {
            missing.push(c);
        }
    }
    Some(missing)
}

/// `face` at `size` pixels, set up the first time and kept for the run.
pub(crate) fn sized(
    face: &FaceInfo,
    size: f32,
    fake_bold: bool,
    fake_italic: bool,
) -> Option<Arc<SizedFont>> {
    static SIZES: OnceLock<Mutex<HashMap<SizeKey, Arc<SizedFont>>>> = OnceLock::new();
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let (key, bytes) = data(&face.source)?;
    let key = SizeKey {
        data: key,
        index: face.index,
        size: size.to_bits(),
        fake_bold,
        fake_italic,
    };
    let mut sizes = SIZES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(font) = sizes.get(&key) {
        return Some(font.clone());
    }
    let font = Arc::new(SizedFont::new(
        read(&bytes, face.index)?,
        size,
        fake_bold,
        fake_italic,
    ));
    sizes.insert(key, font.clone());
    Some(font)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{UNITS_PER_EM, tiny_font};

    fn font(bold: bool, italic: bool, size: f32, fake_bold: bool, fake_italic: bool) -> SizedFont {
        let data: Arc<[u8]> = Arc::from(tiny_font("Sized", bold, italic));
        SizedFont::new(read(&data, 0).unwrap(), size, fake_bold, fake_italic)
    }

    #[test]
    fn letters_are_drawn_at_the_size_asked_for() {
        // The tiny font's box is 0.1 to 0.5 em across and 0 to 0.7 em up; at 20 pixels, 2 to 10
        // across and 14 up from the baseline.
        let f = font(false, false, 20.0, false, false);
        assert_eq!((f.ascent, f.descent), (16.0, 4.0));
        let a = f.glyph('A');
        assert_eq!((a.left, a.top, a.width, a.height), (2, -14, 8, 14));
        assert_eq!(a.advance, 12.0);
        assert!((0..8).all(|x| (0..14).all(|y| a.coverage(x, y) == 255)));
        assert_eq!(a.coverage(8, 0), 0);
        assert_eq!(a.coverage(-1, 0), 0);
        // A space has no drawing, only an advance.
        let space = f.glyph(' ');
        assert_eq!((space.width, space.advance), (0, 6.0));
        // The same letter again is the one already drawn.
        assert!(Arc::ptr_eq(&a, &f.glyph('A')));
        assert_eq!(UNITS_PER_EM, 1000);
    }

    #[test]
    fn a_ring_keeps_its_hole() {
        let f = font(false, false, 20.0, false, false);
        let o = f.glyph('O');
        // The ring is 0.1 to 0.6 em across with a hole from 0.25 to 0.45.
        assert_eq!((o.left, o.width), (2, 10));
        let row = o.height / 2;
        assert_eq!(o.coverage(0, row), 255);
        assert_eq!(o.coverage(4, row), 0);
        assert_eq!(o.coverage(9, row), 255);
    }

    #[test]
    fn made_up_bold_is_wider_and_made_up_italic_leans() {
        let plain = font(false, false, 24.0, false, false).glyph('I');
        let bold = font(false, false, 24.0, true, false).glyph('I');
        assert_eq!(bold.width, plain.width + 1);
        assert_eq!(bold.advance, plain.advance + 1.0);
        let italic = font(false, false, 24.0, false, true).glyph('I');
        assert!(italic.width > plain.width + 2);
        // Its top row starts further right than its bottom row.
        let first_lit = |g: &Glyph, y: i32| (0..g.width).find(|&x| g.coverage(x, y) > 128).unwrap();
        assert!(first_lit(&italic, 0) > first_lit(&italic, italic.height - 1));
    }

    #[test]
    fn kerning_comes_from_the_font() {
        let f = font(false, false, 20.0, false, false);
        let (a, v) = (f.glyph('A'), f.glyph('V'));
        // The tiny font kerns "AV" by -100 units: 2 pixels at 20.
        assert_eq!(f.kerning(&a, &v), -2.0);
        assert_eq!(f.kerning(&v, &a), 0.0);
    }
}
