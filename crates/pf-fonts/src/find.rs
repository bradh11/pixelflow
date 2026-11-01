//! Finding the computer's fonts by family name. PixelFlow's own: it reads only each font file's
//! table directory and its `name`, `OS/2` and `head` tables, so looking through the few hundred
//! font files a computer has takes a moment, once.

use crate::sfnt::{Name, names};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Platforms and name IDs in a `name` table.
const UNICODE: u16 = 0;
const MACINTOSH: u16 = 1;
const WINDOWS: u16 = 3;
const FAMILY: u16 = 1;
const TYPOGRAPHIC_FAMILY: u16 = 16;

/// Where a face's font file is.
#[derive(Clone)]
pub(crate) enum Source {
    File(PathBuf),
    /// Bytes handed over at run time (see [`crate::add_font`]).
    Memory(Arc<[u8]>),
}

impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::File(path) => write!(f, "{}", path.display()),
            Source::Memory(data) => write!(f, "{} bytes in memory", data.len()),
        }
    }
}

/// One face of a font family: its regular, its bold, its italic, and so on.
#[derive(Debug, Clone)]
pub struct FaceInfo {
    /// The family, as the computer's font menus name it.
    pub family: String,
    /// Every name the family goes by, lowercased: the menu name, the family the font's maker
    /// groups it in ("Avenir Next" for "Avenir Next Demi Bold"), and both in other languages.
    pub(crate) names: Vec<String>,
    /// 100 (thin) to 900 (black); 400 is regular and 700 bold.
    pub weight: u16,
    pub italic: bool,
    pub(crate) source: Source,
    /// Which face of a collection (`.ttc`) it is; 0 in a file with one.
    pub(crate) index: u32,
}

impl FaceInfo {
    /// The font file, when the face came from one.
    pub fn path(&self) -> Option<&Path> {
        match &self.source {
            Source::File(path) => Some(path),
            Source::Memory(_) => None,
        }
    }
}

/// The face picked for a family and style, and what has to be made up because the family has no
/// such face (a bold from its regular, a slant from its upright).
#[derive(Debug, Clone, Copy)]
pub struct Found<'a> {
    pub face: &'a FaceInfo,
    pub fake_bold: bool,
    pub fake_italic: bool,
}

/// A set of fonts, looked up by family name.
#[derive(Debug, Default)]
pub struct Library {
    faces: Vec<FaceInfo>,
    by_name: HashMap<String, Vec<usize>>,
}

/// Font files in folders deeper than this aren't looked for (Linux font folders nest a few
/// levels; nothing nests further).
const MAX_DEPTH: usize = 8;
/// The most font files looked at.
const MAX_FILES: usize = 50_000;
/// The largest `name` table read (real ones are a few kilobytes).
const MAX_NAME_TABLE: u32 = 1 << 20;

impl Library {
    /// The fonts in `dirs` and the folders inside them: TrueType and OpenType files (`.ttf`,
    /// `.otf`) and collections (`.ttc`, `.otc`). A file that can't be read is passed over.
    pub fn scan(dirs: &[PathBuf]) -> Self {
        let mut files = Vec::new();
        let mut seen = HashSet::new();
        for dir in dirs {
            walk(dir, 0, &mut files, &mut seen);
        }
        // The same order every time, so a family found twice resolves the same way.
        files.sort();
        let mut lib = Library::default();
        for path in files {
            let Ok(mut file) = File::open(&path) else {
                continue;
            };
            for (index, face) in faces(&mut file) {
                lib.push(FaceInfo {
                    source: Source::File(path.clone()),
                    index,
                    ..face
                });
            }
        }
        lib
    }

    fn push(&mut self, face: FaceInfo) {
        let at = self.faces.len();
        for name in &face.names {
            self.by_name.entry(name.clone()).or_default().push(at);
        }
        self.faces.push(face);
    }

    /// Adds the faces in a font file's bytes; returns the families they're in.
    pub(crate) fn add_data(&mut self, data: Arc<[u8]>) -> Vec<String> {
        let mut families = Vec::new();
        for (index, face) in faces(&mut Cursor::new(&data[..])) {
            if !families.contains(&face.family) {
                families.push(face.family.clone());
            }
            self.push(FaceInfo {
                source: Source::Memory(data.clone()),
                index,
                ..face
            });
        }
        families
    }

    /// Every face, in the order found.
    pub fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    /// How many faces it has.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Every family, once, sorted by name. Families the system keeps to itself (macOS names
    /// them with a leading dot) are left out.
    pub fn families(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out: Vec<String> = self
            .faces
            .iter()
            .filter(|f| !f.family.starts_with('.') && !f.family.is_empty())
            .filter(|f| seen.insert(f.family.to_lowercase()))
            .map(|f| f.family.clone())
            .collect();
        out.sort_by_key(|f| f.to_lowercase());
        out
    }

    /// The face of the family `name` (any case) nearest to bold or regular, italic or upright:
    /// the slant first, then the weight. `None` when there's no such family.
    pub fn find(&self, name: &str, bold: bool, italic: bool) -> Option<Found<'_>> {
        let list = self.by_name.get(&name.trim().to_lowercase())?;
        let want: i32 = if bold { 700 } else { 400 };
        let face = list.iter().map(|&i| &self.faces[i]).min_by_key(|f| {
            u32::from(f.italic != italic) * 10_000 + (i32::from(f.weight) - want).unsigned_abs()
        })?;
        Some(Found {
            face,
            fake_bold: bold && face.weight < 600,
            fake_italic: italic && !face.italic,
        })
    }
}

/// The folders this computer keeps its fonts in: the system's, everyone's, and the user's own.
pub fn system_font_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        dirs.extend(
            [
                "/System/Library/Fonts",
                "/Library/Fonts",
                "/Network/Library/Fonts",
            ]
            .into_iter()
            .map(PathBuf::from),
        );
        dirs.extend(home.map(|h| h.join("Library/Fonts")));
    }
    #[cfg(windows)]
    {
        let _ = home;
        let windows = std::env::var_os("WINDIR")
            .or_else(|| std::env::var_os("SystemRoot"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        dirs.push(windows.join("Fonts"));
        // Fonts installed for one user only.
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
        dirs.extend(data_home.map(|d| d.join("fonts")));
        dirs.extend(home.map(|h| h.join(".fonts")));
        let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
        let data_dirs = if data_dirs.trim().is_empty() {
            "/usr/local/share:/usr/share".to_string()
        } else {
            data_dirs
        };
        dirs.extend(
            data_dirs
                .split(':')
                .filter(|d| !d.is_empty())
                .map(|d| PathBuf::from(d).join("fonts")),
        );
    }
    dirs
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_FILES {
        return;
    }
    // Folders linked from more than one place are looked through once.
    let Ok(real) = dir.canonicalize() else {
        return;
    };
    if !seen.insert(real) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            walk(&path, depth + 1, out, seen);
        } else if is_font_file(&path)
            && out.len() < MAX_FILES
            && path.canonicalize().is_ok_and(|real| seen.insert(real))
        {
            out.push(path);
        }
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc"))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn read_at<R: Read + Seek>(r: &mut R, offset: u64, len: usize) -> Option<Vec<u8>> {
    r.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0; len];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// The faces in a font file (one, or each of a collection's), with their index. Only the
/// family, weight, and slant are filled in.
fn faces<R: Read + Seek>(r: &mut R) -> Vec<(u32, FaceInfo)> {
    let Some(head) = read_at(r, 0, 12) else {
        return Vec::new();
    };
    let offsets: Vec<u32> = if &head[..4] == b"ttcf" {
        let count = be32(&head, 8).unwrap_or(0).min(1024) as usize;
        match read_at(r, 12, count * 4) {
            Some(table) => (0..count).filter_map(|i| be32(&table, i * 4)).collect(),
            None => Vec::new(),
        }
    } else {
        vec![0]
    };
    offsets
        .into_iter()
        .enumerate()
        .filter_map(|(i, offset)| Some((i as u32, face(r, u64::from(offset))?)))
        .collect()
}

/// One face, from its table directory at `offset`.
fn face<R: Read + Seek>(r: &mut R, offset: u64) -> Option<FaceInfo> {
    let header = read_at(r, offset, 12)?;
    // TrueType outlines, CFF outlines ("OTTO"), or an older Mac TrueType ("true").
    if !matches!(be32(&header, 0)?, 0x0001_0000 | 0x4F54_544F | 0x7472_7565) {
        return None;
    }
    let tables = usize::from(be16(&header, 4)?);
    let records = read_at(r, offset + 12, tables * 16)?;
    let table = |tag: &[u8; 4]| -> Option<(u64, u32)> {
        (0..tables).find_map(|i| {
            let at = i * 16;
            (records.get(at..at + 4)? == tag)
                .then(|| Some((u64::from(be32(&records, at + 8)?), be32(&records, at + 12)?)))?
        })
    };
    let (name_at, name_len) = table(b"name")?;
    let name_data = read_at(r, name_at, name_len.min(MAX_NAME_TABLE) as usize)?;
    let (family, all) = family_names(names(&name_data))?;

    // Bold and italic from the OS/2 table, or from the older `head` table's style bits.
    let mac_style = table(b"head")
        .and_then(|(at, len)| read_at(r, at, len.min(54) as usize))
        .and_then(|head| be16(&head, 44))
        .unwrap_or(0);
    let os2 = table(b"OS/2").and_then(|(at, len)| read_at(r, at, len.min(64) as usize));
    let weight = os2.as_ref().and_then(|t| be16(t, 4)).filter(|&w| w > 0);
    let selection = os2.as_ref().and_then(|t| be16(t, 62));
    let weight = match weight {
        // Some older fonts count weights 1 to 9.
        Some(w) if w < 10 => w * 100,
        Some(w) => w.min(1000),
        None if mac_style & 1 != 0 => 700,
        None => 400,
    };
    let italic = match selection {
        // Italic, or oblique.
        Some(s) => s & 1 != 0 || s & (1 << 9) != 0,
        None => mac_style & 2 != 0,
    };
    Some(FaceInfo {
        family,
        names: all,
        weight,
        italic,
        source: Source::Memory(Arc::from(Vec::new())),
        index: 0,
    })
}

/// The family as the font menus show it (its English name where the font has one), and every
/// family name the face goes by, lowercased.
fn family_names<'a>(names: impl Iterator<Item = Name<'a>>) -> Option<(String, Vec<String>)> {
    let mut menu: Option<(u8, String)> = None;
    let mut grouped: Option<(u8, String)> = None;
    let mut all: Vec<String> = Vec::new();
    for n in names {
        if n.id != FAMILY && n.id != TYPOGRAPHIC_FAMILY {
            continue;
        }
        let Some(text) = decode(&n) else {
            continue;
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        // English names first: Windows' US English, then a Mac's English, then any.
        let rank = match (n.platform, n.language) {
            (WINDOWS, 0x0409) => 0,
            (MACINTOSH, 0) | (UNICODE, _) => 1,
            _ => 2,
        };
        let slot = if n.id == FAMILY { &mut menu } else { &mut grouped };
        if slot.as_ref().is_none_or(|(r, _)| rank < *r) {
            *slot = Some((rank, text.clone()));
        }
        let lower = text.to_lowercase();
        if !all.contains(&lower) {
            all.push(lower);
        }
    }
    let (_, family) = menu.or(grouped)?;
    Some((family, all))
}

/// A name's text: UTF-16 for Unicode and Windows names, Mac Roman (its ASCII part) for a Mac's.
fn decode(n: &Name<'_>) -> Option<String> {
    if let Some(text) = n.utf16() {
        return Some(text);
    }
    if n.platform == MACINTOSH && n.encoding == 0 {
        return Some(
            n.bytes
                .iter()
                .map(|&b| if b.is_ascii() { char::from(b) } else { '?' })
                .collect(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{tiny_collection, tiny_font};

    #[test]
    fn faces_are_found_by_family_in_any_case() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.ttf"), tiny_font("Test Sans", false, false)).unwrap();
        std::fs::write(dir.path().join("b.TTF"), tiny_font("Test Sans", true, false)).unwrap();
        std::fs::create_dir(dir.path().join("more")).unwrap();
        std::fs::write(dir.path().join("more/c.otf"), tiny_font("Other", false, true)).unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not a font").unwrap();
        std::fs::write(dir.path().join("broken.ttf"), "not a font either").unwrap();
        let lib = Library::scan(&[dir.path().to_path_buf()]);
        assert_eq!(lib.len(), 3);
        assert_eq!(lib.families(), vec!["Other".to_string(), "Test Sans".to_string()]);

        let regular = lib.find("test sans", false, false).unwrap();
        assert_eq!(
            (regular.face.weight, regular.fake_bold, regular.fake_italic),
            (400, false, false)
        );
        assert!(regular.face.path().unwrap().ends_with("a.ttf"));
        let bold = lib.find("TEST SANS", true, false).unwrap();
        assert_eq!((bold.face.weight, bold.fake_bold), (700, false));
        // No italic face: the upright one, slanted.
        let italic = lib.find("Test Sans", false, true).unwrap();
        assert_eq!((italic.face.weight, italic.fake_italic), (400, true));
        // An italic-only family is drawn upright from its italic (nothing upright to use).
        let other = lib.find("other", true, false).unwrap();
        assert!(other.face.italic && other.fake_bold);
        assert!(lib.find("Nope", false, false).is_none());
    }

    #[test]
    fn a_collections_faces_are_each_found() {
        let dir = tempfile::tempdir().unwrap();
        let ttc = tiny_collection(&[("Pair", false, false), ("Pair", true, true)]);
        std::fs::write(dir.path().join("pair.ttc"), ttc).unwrap();
        let lib = Library::scan(&[dir.path().to_path_buf()]);
        assert_eq!(lib.len(), 2);
        let bold_italic = lib.find("Pair", true, true).unwrap();
        assert_eq!(
            (
                bold_italic.face.index,
                bold_italic.fake_bold,
                bold_italic.fake_italic
            ),
            (1, false, false)
        );
        assert_eq!(lib.find("Pair", false, false).unwrap().face.index, 0);
    }

    #[test]
    fn missing_and_looping_folders_are_passed_over() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.ttf"), tiny_font("Loop Sans", false, false)).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path(), dir.path().join("again")).unwrap();
        let lib = Library::scan(&[dir.path().to_path_buf(), dir.path().join("no such folder")]);
        assert_eq!(lib.len(), 1);
    }

    #[test]
    fn this_computers_fonts_can_be_listed() {
        // Which fonts there are depends on the computer: this only checks that looking doesn't
        // fail, and says what it found.
        let lib = Library::scan(&system_font_dirs());
        let families = lib.families();
        eprintln!(
            "{} faces in {} families on this computer",
            lib.len(),
            families.len()
        );
        assert!(
            families
                .windows(2)
                .all(|w| w[0].to_lowercase() <= w[1].to_lowercase())
        );
    }
}
