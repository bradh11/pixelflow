//! The computer's own fonts, for the Text effect: finding a font family by name on macOS, Windows,
//! and Linux, and drawing its letters at a size as coverage (how much of each pixel a letter
//! covers, so anti-aliased edges become partial brightness).
//!
//! No font file ships with PixelFlow. A sequence names a family ("Arial Narrow"); the computer it
//! plays on either has that family or doesn't, and the Text effect falls back to PixelFlow's own
//! pixel font when it doesn't.
//!
//! # Why this crate and not a font library
//!
//! The choice, made in October 2026 for the Text effect's fonts:
//!
//! | Option | Licence | What comes with it | TrueType, CFF, collections | Kept up |
//! |---|---|---|---|---|
//! | `ttf-parser` alone (with only `std`) | MIT OR Apache-2.0 | nothing else | yes, yes, yes | no: its author declared it unmaintained in June 2026 (RUSTSEC-2026-0192) |
//! | `fontdue` | MIT OR Apache-2.0 OR Zlib | `ttf-parser`, `hashbrown`, `foldhash`, `equivalent`, `allocator-api2`, `core_maths`, `libm` | yes, yes (through `ttf-parser`), yes | reads fonts through `ttf-parser` |
//! | `ab_glyph` | Apache-2.0 | `ttf-parser`, `owned_ttf_parser`, `ab_glyph_rasterizer`, `core_maths`, `libm` | yes, yes, yes | reads fonts through `ttf-parser` |
//! | `swash` | Apache-2.0 OR MIT | `skrifa`, `read-fonts`, `font-types`, `zeno`, `yazi`, `bytemuck` (and its derive macro, so `syn` at build time) | yes, yes, yes | yes |
//! | `skrifa` alone (what `swash` reads fonts with) | MIT OR Apache-2.0 | `read-fonts`, `font-types`, `bytemuck`'s derive macro, and `yoke` with its derive macro and theirs (`zerofrom`, `stable_deref_trait`, `synstructure`): ten crates new to PixelFlow at 0.49 | yes, yes, yes | yes (Google Fonts' "fontations", used in Chrome) |
//! | `fontdb` (finding fonts only) | MIT | `ttf-parser`, `memmap2`, `libc`, `log`, `slotmap`, `tinyvec` | not applicable | reads fonts through `ttf-parser` |
//!
//! All are pure Rust and permissively licensed. Every one of them but `swash` and `skrifa` reads
//! fonts through `ttf-parser`. What they add on top is a rasterizer (a few hundred lines) and,
//! for `swash`, shaping and hinting that light displays don't need. `fontdb` finds fonts by
//! name, but brings six crates and maps files into memory where reading the two small name
//! tables of each font file is all that's needed.
//!
//! PixelFlow first read fonts with `ttf-parser` (one crate, no dependencies of its own). It gets
//! no more fixes, so since November 2026 PixelFlow reads them with `skrifa`, the kept-up parser,
//! for the hard part: TrueType and CFF outlines, in `.ttc` and `.otc` collections too. The rest is
//! PixelFlow's own, written for this: finding fonts (`find.rs`), reading the tables a letter's
//! glyph, place, and size come from (`sfnt.rs`, which reads them as `ttf-parser` did, so the move
//! changed nothing a show draws), and drawing outlines (`raster.rs`).

mod find;
#[cfg(test)]
mod pinned;
mod raster;
mod sfnt;
mod sized;
#[doc(hidden)]
pub mod testing;

pub use find::{FaceInfo, Found, Library, system_font_dirs};
pub use sized::{Glyph, SizedFont};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

/// The fonts on this computer, found the first time anything asks (see [`system_font_dirs`]),
/// with any added since by [`add_font`].
fn system() -> &'static RwLock<Library> {
    static SYSTEM: OnceLock<RwLock<Library>> = OnceLock::new();
    SYSTEM.get_or_init(|| RwLock::new(Library::scan(&system_font_dirs())))
}

fn read<T>(f: impl FnOnce(&Library) -> T) -> T {
    f(&system().read().unwrap_or_else(|e| e.into_inner()))
}

/// The font families on this computer, sorted by name, as its font menus name them.
pub fn families() -> Vec<String> {
    read(Library::families)
}

/// The family this computer has by `name` (any case, and the names a family also goes by), as its
/// font menus spell it; `None` when it has no such family.
pub fn installed(name: &str) -> Option<String> {
    read(|lib| lib.find(name, false, false).map(|f| f.face.family.clone()))
}

/// How `name` would be drawn bold and/or italic on this computer: which of its faces, and whether
/// the bold or the slant is made up from a lighter or upright face. `None` when the family isn't
/// on this computer.
pub fn style_of(name: &str, bold: bool, italic: bool) -> Option<(String, bool, bool)> {
    read(|lib| {
        lib.find(name, bold, italic)
            .map(|f| (f.face.family.clone(), f.fake_bold, f.fake_italic))
    })
}

/// The characters of `text` that the family `name` (bold and/or italic, as [`sized`] would draw
/// it) has no letter for; `None` when this computer doesn't have the family.
pub fn missing_letters(name: &str, bold: bool, italic: bool, text: &str) -> Option<Vec<char>> {
    let face = read(|lib| lib.find(name, bold, italic).map(|f| f.face.clone()))?;
    sized::missing_letters(&face, text)
}

/// What [`sized`] has been asked for so far, by the name as asked: answered from here after the
/// first time, so a frame costs one lookup. Cleared when fonts are added.
type Asked = HashMap<String, Vec<(u32, bool, bool, Option<Arc<SizedFont>>)>>;

fn asked() -> MutexGuard<'static, Asked> {
    static ASKED: OnceLock<Mutex<Asked>> = OnceLock::new();
    ASKED
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// The family `name`, bold and/or italic, ready to draw at `size` pixels (its em, as a font size
/// in points draws on a Mac); `None` when this computer doesn't have the family or its file can't
/// be read. Each size is set up once, and each letter drawn once at it (see [`SizedFont::glyph`]).
pub fn sized(name: &str, size: f32, bold: bool, italic: bool) -> Option<Arc<SizedFont>> {
    let key = (size.to_bits(), bold, italic);
    if let Some(list) = asked().get(name)
        && let Some(hit) = list.iter().find(|a| (a.0, a.1, a.2) == key)
    {
        return hit.3.clone();
    }
    let found = read(|lib| {
        lib.find(name, bold, italic)
            .map(|f| (f.face.clone(), f.fake_bold, f.fake_italic))
    });
    let font =
        found.and_then(|(face, fake_bold, fake_italic)| sized::sized(&face, size, fake_bold, fake_italic));
    asked()
        .entry(name.to_string())
        .or_default()
        .push((key.0, key.1, key.2, font.clone()));
    font
}

/// Adds the faces of a font file's bytes to this computer's fonts for the rest of the run, and
/// returns the families they're in. For tests, which can't count on any font being installed.
pub fn add_font(data: Vec<u8>) -> Vec<String> {
    let families = system()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .add_data(Arc::from(data));
    asked().clear();
    families
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn added_fonts_are_found_drawn_and_listed() {
        let added = add_font(testing::tiny_font("PixelFlow Lib Test", false, false));
        assert_eq!(added, vec!["PixelFlow Lib Test".to_string()]);
        assert_eq!(
            installed("pixelflow lib test").as_deref(),
            Some("PixelFlow Lib Test")
        );
        assert!(families().contains(&"PixelFlow Lib Test".to_string()));
        assert_eq!(
            style_of("PixelFlow Lib Test", true, false),
            Some(("PixelFlow Lib Test".to_string(), true, false))
        );
        let font = sized("PixelFlow Lib Test", 20.0, false, false).unwrap();
        assert_eq!(font.glyph('A').width, 8);
        // The same size again is the one already set up.
        assert!(Arc::ptr_eq(
            &font,
            &sized("PixelFlow Lib Test", 20.0, false, false).unwrap()
        ));
        assert!(installed("No Such Family For PixelFlow").is_none());
        assert!(sized("No Such Family For PixelFlow", 20.0, false, false).is_none());
        assert!(sized("PixelFlow Lib Test", f32::NAN, false, false).is_none());
        // The tiny font has letters and digits, not accents.
        assert_eq!(
            missing_letters("PixelFlow Lib Test", false, false, "No\u{eb}l 2026\n"),
            Some(vec!['\u{eb}'])
        );
        assert_eq!(
            missing_letters("No Such Family For PixelFlow", false, false, "a"),
            None
        );
    }

    /// What kind of outlines a face's file holds: "OTTO" for CFF, anything else TrueType.
    fn outlines(face: &FaceInfo) -> Option<[u8; 4]> {
        let mut file = std::fs::File::open(face.path()?).ok()?;
        let mut head = [0u8; 4];
        file.read_exact(&mut head).ok()?;
        Some(head)
    }

    #[test]
    fn this_computers_own_fonts_draw_when_it_has_them() {
        // Which fonts there are depends on the computer, so each kind is drawn when one is
        // found and passed over, saying so, when not.
        let started = std::time::Instant::now();
        let lib = Library::scan(&system_font_dirs());
        eprintln!("{} faces found in {:?}", lib.len(), started.elapsed());
        type Is = fn(&FaceInfo, Option<[u8; 4]>) -> bool;
        let kinds: [(&str, Is); 3] = [
            ("TrueType", |_, h| h == Some([0, 1, 0, 0])),
            ("CFF (.otf)", |_, h| h == Some(*b"OTTO")),
            ("collection (.ttc)", |f, h| h == Some(*b"ttcf") && f.index > 0),
        ];
        for (kind, is) in kinds {
            let Some(face) = lib
                .faces()
                .iter()
                .filter(|f| !f.family.starts_with('.') && is(f, outlines(f)))
                .find(|f| sized::sized(f, 24.0, false, false).is_some_and(|s| s.glyph('A').width > 0))
            else {
                eprintln!("no {kind} font on this computer draws an 'A': not checked");
                continue;
            };
            let font = sized::sized(face, 24.0, false, false).unwrap();
            let a = font.glyph('A');
            let lit = (0..a.height)
                .flat_map(|y| (0..a.width).map(move |x| (x, y)))
                .filter(|&(x, y)| a.coverage(x, y) > 0)
                .count();
            eprintln!(
                "{kind}: {} draws 'A' {}x{} with {lit} pixels lit",
                face.family, a.width, a.height
            );
            // Any real letter at 24 pixels lights some pixels and is not many times that size.
            assert!(
                lit > 4 && a.height > 2 && a.height <= 96,
                "{kind}: {}",
                face.family
            );
        }
    }
}
