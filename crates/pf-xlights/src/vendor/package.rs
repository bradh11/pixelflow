//! Reading a vendor's sequence: an `.xsq` (or an old `.xml` sequence), a vendor package `.zip`,
//! xLights' packaged `.xsqz` (also a zip), or a folder.
//!
//! A package usually holds the vendor's layout (`xlights_rgbeffects.xml`), the sequence
//! (sometimes in a subfolder), music, and images. Only the sequence, the layout, and the music
//! are ever read. Nothing is extracted: entries are read into memory within limits, and the
//! music is copied, under its own file name only, into a folder the caller chooses. Entries
//! whose names point outside the package (`../`, absolute paths) are never read, and a zip with
//! too many entries, or that would unpack to too much, is refused.

use crate::XlightsError;
use crate::sequence::{MAX_XSQ_BYTES, XsqFile, parse_xsq, too_large};
use crate::xml::MAX_XML_BYTES;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Music file extensions (as PixelFlow plays them).
pub const MUSIC_EXTENSIONS: [&str; 5] = ["mp3", "m4a", "wav", "ogg", "flac"];

/// How much of a package is looked at.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Most entries in a zip (or files looked at in a folder).
    pub entries: usize,
    /// Most bytes a zip's entries add up to, unpacked, as the zip says.
    pub total_bytes: u64,
    /// Largest music file copied.
    pub music_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: 20_000,
            total_bytes: 4 * 1024 * 1024 * 1024,
            music_bytes: 500 * 1024 * 1024,
        }
    }
}

/// How deep in a folder sequences and layouts are looked for.
const MAX_FOLDER_DEPTH: usize = 4;
/// Bytes of an `.xml` file read to tell a sequence from other xLights files.
const SNIFF_BYTES: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    /// One sequence file.
    Sequence,
    Zip,
    Folder,
}

/// A vendor sequence, ready to read.
#[derive(Debug, Clone)]
pub struct Package {
    path: PathBuf,
    kind: PackageKind,
    /// The sequences in it, as paths inside it ('/' between folders), best first.
    sequences: Vec<String>,
    /// The vendor's layout, as a path inside it.
    layout: Option<String>,
    /// Music files, as paths inside it.
    music: Vec<String>,
    limits: Limits,
}

fn package_error(text: impl Into<String>) -> XlightsError {
    XlightsError::Package(text.into())
}

fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn file_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn is_music(name: &str) -> bool {
    MUSIC_EXTENSIONS.contains(&extension(name).as_str())
}

/// A zip entry's name that stays inside the package: relative, without `..`, a drive, or a
/// device name.
fn stays_inside(entry: &zip::read::ZipFile<'_, File>) -> bool {
    let name = entry.name();
    entry.enclosed_name().is_some()
        && !name.is_empty()
        && !name.starts_with(['/', '\\'])
        && !name.contains([':', '\0'])
        && !name.split(['/', '\\']).any(|part| part == "..")
}

/// Files never looked at: macOS' resource forks, hidden files, and xLights' backups.
fn ignored(name: &str) -> bool {
    name.split('/').any(|part| {
        part.starts_with('.') || part.eq_ignore_ascii_case("__MACOSX") || part.eq_ignore_ascii_case("Backup")
    }) || extension(name) == "xbkp"
}

fn is_layout(name: &str) -> bool {
    file_name(name).eq_ignore_ascii_case("xlights_rgbeffects.xml")
}

/// Whether a file may be a sequence, before reading it: `.xsq`, or `.xml` other than xLights'
/// own settings files (old sequences were `.xml`).
fn may_be_sequence(name: &str) -> bool {
    let lower = file_name(name).to_ascii_lowercase();
    match extension(name).as_str() {
        "xsq" => true,
        "xml" => !lower.starts_with("xlights_") && !lower.starts_with("xschedule"),
        _ => false,
    }
}

/// The start of a file says it's an xLights sequence.
fn sniffs_as_sequence(head: &[u8]) -> bool {
    String::from_utf8_lossy(head).contains("<xsequence")
}

/// Reads at most `limit` bytes; an error naming `what` when there's more.
fn read_limited(reader: impl Read, limit: u64, what: &str) -> Result<Vec<u8>, XlightsError> {
    let mut out = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| package_error(format!("Couldn't read {what} in the package ({e}).")))?;
    if out.len() as u64 > limit {
        return Err(package_error(format!(
            "{what} in the package is larger than PixelFlow reads ({} MB).",
            limit / (1024 * 1024)
        )));
    }
    Ok(out)
}

/// Best sequence first: `.xsq` before old `.xml`, then the shallowest, then by name.
fn rank(sequences: &mut [String]) {
    sequences.sort_by_key(|s| (extension(s) != "xsq", s.matches('/').count(), s.to_lowercase()));
}

impl Package {
    /// Opens the sequence, zip, or folder at `path` and finds what's in it.
    pub fn open(path: &Path) -> Result<Package, XlightsError> {
        Self::open_with(path, Limits::default())
    }

    pub fn open_with(path: &Path, limits: Limits) -> Result<Package, XlightsError> {
        let meta = std::fs::metadata(path).map_err(|e| XlightsError::Read(path.display().to_string(), e))?;
        let mut package = Package {
            path: path.to_path_buf(),
            kind: PackageKind::Sequence,
            sequences: Vec::new(),
            layout: None,
            music: Vec::new(),
            limits,
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if meta.is_dir() {
            package.kind = PackageKind::Folder;
            package.scan_folder()?;
        } else if may_be_sequence(&name) {
            // A sequence on its own: the layout and music are looked for next to it.
            package.sequences.push(name);
            if let Some(folder) = path.parent() {
                for dir in [Some(folder), folder.parent()].into_iter().flatten() {
                    let layout = dir.join("xlights_rgbeffects.xml");
                    if layout.is_file() {
                        package.layout = Some(pf_model::path_to_text(&layout));
                        break;
                    }
                }
            }
        } else {
            package.kind = PackageKind::Zip;
            package.scan_zip()?;
        }
        if package.sequences.is_empty() {
            return Err(package_error(
                "There's no xLights sequence (.xsq) in this package. Choose the .xsq, the vendor's .zip, or the folder it unpacked to.",
            ));
        }
        rank(&mut package.sequences);
        Ok(package)
    }

    pub fn kind(&self) -> PackageKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The sequences in the package, best first.
    pub fn sequences(&self) -> &[String] {
        &self.sequences
    }

    pub fn has_layout(&self) -> bool {
        self.layout.is_some()
    }

    fn zip(&self) -> Result<zip::ZipArchive<File>, XlightsError> {
        let file =
            File::open(&self.path).map_err(|e| XlightsError::Read(self.path.display().to_string(), e))?;
        zip::ZipArchive::new(file).map_err(|_| {
            package_error(format!(
                "{} isn't a zip file PixelFlow can read. If it's still downloading (or only a placeholder for a file in the cloud), wait for it to finish and try again.",
                self.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            ))
        })
    }

    fn scan_zip(&mut self) -> Result<(), XlightsError> {
        let mut zip = self.zip()?;
        if zip.len() > self.limits.entries {
            return Err(package_error(format!(
                "The package has {} files; PixelFlow reads packages of up to {}.",
                zip.len(),
                self.limits.entries
            )));
        }
        let mut total: u64 = 0;
        let mut candidates = Vec::new();
        for i in 0..zip.len() {
            let entry = zip
                .by_index_raw(i)
                .map_err(|e| package_error(format!("Couldn't read the package ({e}).")))?;
            total = total.saturating_add(entry.size());
            if total > self.limits.total_bytes {
                return Err(package_error(format!(
                    "The package unpacks to more than {} GB, more than PixelFlow reads.",
                    self.limits.total_bytes / (1024 * 1024 * 1024)
                )));
            }
            // Only names that stay inside the package are ever used.
            if entry.is_dir() || !stays_inside(&entry) {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            if ignored(&name) {
                continue;
            }
            if is_layout(&name) {
                let depth = name.matches('/').count();
                if self
                    .layout
                    .as_ref()
                    .is_none_or(|l| l.matches('/').count() > depth)
                {
                    self.layout = Some(name.clone());
                }
            } else if is_music(&name) {
                self.music.push(name.clone());
            } else if may_be_sequence(&name) {
                candidates.push((i, name));
            }
        }
        for (i, name) in candidates {
            let head = {
                let entry = zip
                    .by_index(i)
                    .map_err(|e| package_error(format!("Couldn't read the package ({e}).")))?;
                let mut head = Vec::new();
                // An entry that can't be read (encrypted, an unknown compression) isn't a sequence.
                if entry.take(SNIFF_BYTES).read_to_end(&mut head).is_err() {
                    continue;
                }
                head
            };
            if extension(&name) == "xsq" || sniffs_as_sequence(&head) {
                self.sequences.push(name);
            }
        }
        Ok(())
    }

    fn scan_folder(&mut self) -> Result<(), XlightsError> {
        let mut queue: Vec<(PathBuf, String, usize)> = vec![(self.path.clone(), String::new(), 0)];
        let mut seen = 0;
        while let Some((dir, prefix, depth)) = queue.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                seen += 1;
                if seen > self.limits.entries {
                    return Ok(());
                }
                let file = entry.file_name().to_string_lossy().into_owned();
                let name = format!("{prefix}{file}");
                if ignored(&name) {
                    continue;
                }
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    if depth + 1 < MAX_FOLDER_DEPTH {
                        queue.push((entry.path(), format!("{name}/"), depth + 1));
                    }
                } else if !kind.is_file() {
                    // Links aren't followed: everything read is inside the folder.
                    continue;
                } else if is_layout(&name) {
                    let depth = name.matches('/').count();
                    if self
                        .layout
                        .as_ref()
                        .is_none_or(|l| l.matches('/').count() > depth)
                    {
                        self.layout = Some(name);
                    }
                } else if is_music(&name) {
                    self.music.push(name);
                } else if may_be_sequence(&name) {
                    let mut head = Vec::new();
                    let read =
                        File::open(entry.path()).and_then(|f| f.take(SNIFF_BYTES).read_to_end(&mut head));
                    if read.is_ok() && (extension(&name) == "xsq" || sniffs_as_sequence(&head)) {
                        self.sequences.push(name);
                    }
                }
            }
        }
        Ok(())
    }

    /// The file on disk for a path inside a sequence or folder package.
    pub fn file_path(&self, name: &str) -> Option<PathBuf> {
        match self.kind {
            PackageKind::Zip => None,
            PackageKind::Folder => Some(self.path.join(name)),
            PackageKind::Sequence if self.sequences.iter().any(|s| s == name) => Some(self.path.clone()),
            // The layout next to a sequence is kept as a full path.
            PackageKind::Sequence => Some(pf_model::path_from_text(name)),
        }
    }

    /// Reads one file of the package, at most `limit` bytes.
    fn read(&self, name: &str, limit: u64, what: &str) -> Result<Vec<u8>, XlightsError> {
        match self.file_path(name) {
            Some(path) => {
                let file =
                    File::open(&path).map_err(|e| XlightsError::Read(path.display().to_string(), e))?;
                read_limited(file, limit, what)
            }
            None => {
                let mut zip = self.zip()?;
                let entry = zip
                    .by_name(name)
                    .map_err(|_| package_error(format!("{what} isn't in the package any more.")))?;
                read_limited(entry, limit, what)
            }
        }
    }

    /// The sequence called `name` (one of [`Package::sequences`]), or the best one.
    pub fn read_sequence(&self, name: Option<&str>) -> Result<(String, XsqFile), XlightsError> {
        let name = match name {
            Some(n) if self.sequences.iter().any(|s| s == n) => n.to_string(),
            Some(n) => return Err(package_error(format!("{n} isn't a sequence in this package."))),
            None => self.sequences[0].clone(),
        };
        let bytes = self
            .read(&name, MAX_XSQ_BYTES as u64, "The sequence")
            .map_err(|e| match e {
                XlightsError::Package(text) if text.contains("larger than") => {
                    too_large(MAX_XSQ_BYTES as u64 + 1)
                }
                other => other,
            })?;
        let file = parse_xsq(&String::from_utf8_lossy(&bytes))?;
        Ok((name, file))
    }

    /// The vendor's layout file's text, when the package has one.
    pub fn read_layout(&self) -> Result<Option<String>, XlightsError> {
        let Some(name) = &self.layout else {
            return Ok(None);
        };
        let bytes = self.read(name, MAX_XML_BYTES as u64, "The layout (xlights_rgbeffects.xml)")?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// The package's music for the sequence `sequence`, which names `media`: the file of that
    /// name, else one named like the sequence, else the package's only music file.
    pub fn music_for(&self, sequence: &str, media: Option<&str>) -> Option<&str> {
        let wanted = media.map(file_name).map(str::trim).filter(|m| !m.is_empty());
        let stem = |n: &str| {
            Path::new(file_name(n))
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        };
        wanted
            .and_then(|w| self.music.iter().find(|m| file_name(m).eq_ignore_ascii_case(w)))
            .or_else(|| self.music.iter().find(|m| stem(m) == stem(sequence)))
            .or_else(|| (self.music.len() == 1).then(|| &self.music[0]))
            .map(String::as_str)
    }

    /// Copies the package's music file `name` into `folder` (made when missing), under its
    /// own file name; a file of that name and size already there is used as is, and a
    /// different one is kept (this one is saved as "Song (2).mp3"). The copy is written in a
    /// hidden file and moved into place once whole. Returns where it is.
    pub fn copy_music(&self, name: &str, folder: &Path) -> Result<PathBuf, XlightsError> {
        if !self.music.iter().any(|m| m == name) {
            return Err(package_error(format!("{name} isn't music in this package.")));
        }
        // Only the file's own name, never a path from the package.
        let own = file_name(name).trim();
        if own.is_empty() || own.starts_with('.') || own.contains(['/', '\\', ':']) {
            return Err(package_error(format!(
                "The music's name ({own}) can't be used as a file name."
            )));
        }
        let bytes = self.read(name, self.limits.music_bytes, "The music")?;
        let write_err = |e: std::io::Error| {
            package_error(format!("Couldn't save the music in {} ({e}).", folder.display()))
        };
        std::fs::create_dir_all(folder).map_err(write_err)?;
        let path = Path::new(own);
        let (stem, ext) = (
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path.extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default(),
        );
        let mut n = 1;
        let target = loop {
            let candidate = folder.join(if n == 1 {
                own.to_string()
            } else {
                format!("{stem} ({n}){ext}")
            });
            match std::fs::metadata(&candidate) {
                Ok(meta) if meta.is_file() && meta.len() == bytes.len() as u64 => return Ok(candidate),
                Ok(_) => n += 1,
                Err(_) => break candidate,
            }
            if n > 1000 {
                return Err(package_error(
                    "There are too many copies of the music in its folder already.",
                ));
            }
        };
        let temp = folder.join(format!(".{own}.pixelflow-part"));
        let written = File::create(&temp)
            .and_then(|mut f| f.write_all(&bytes).and_then(|_| f.sync_all()))
            .and_then(|_| std::fs::rename(&temp, &target));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&temp);
            return Err(write_err(e));
        }
        Ok(target)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    pub(crate) const SEQUENCE: &str = r#"<?xml version="1.0"?>
<xsequence BaseChannel="0" ChanCtrlBasic="0" ChanCtrlColor="0" FixedPointTiming="1">
  <head><version>2024.01</version><song>Made Up Song</song><mediaFile>C:\Vendor\Made Up Song.mp3</mediaFile>
    <sequenceType>Media</sequenceType><sequenceTiming>50 ms</sequenceTiming><sequenceDuration>10.000</sequenceDuration></head>
  <ColorPalettes><ColorPalette>C_BUTTON_Palette1=#FF0000,C_CHECKBOX_Palette1=1</ColorPalette></ColorPalettes>
  <EffectDB><Effect>E_SLIDER_Speed=10</Effect></EffectDB>
  <DisplayElements/>
  <ElementEffects>
    <Element type="model" name="MegaTree 16x50">
      <EffectLayer><Effect ref="0" name="On" startTime="0" endTime="1000" palette="0"/><Effect ref="0" name="On" startTime="1000" endTime="2000" palette="0"/></EffectLayer>
    </Element>
    <Element type="model" name="Roofline"><EffectLayer><Effect ref="0" name="On" startTime="0" endTime="500" palette="0"/></EffectLayer></Element>
  </ElementEffects>
</xsequence>"#;

    pub(crate) const LAYOUT: &str = r#"<?xml version="1.0"?>
<xrgb><models>
  <model name="MegaTree 16x50" DisplayAs="Tree 360" parm1="16" parm2="50" StringType="RGB Nodes" StartChannel="1"/>
  <model name="Roofline" DisplayAs="Single Line" parm1="1" parm2="100" StringType="RGB Nodes" StartChannel="1"/>
</models><modelGroups/></xrgb>"#;

    /// A zip in memory of `(name, contents)`.
    pub(crate) fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            out.start_file(*name, SimpleFileOptions::default()).unwrap();
            out.write_all(bytes).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_vendor_zip_lists_its_sequences_layout_and_music() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "Made Up Song.zip",
            &zip_of(&[
                ("Show/xlights_rgbeffects.xml", LAYOUT.as_bytes()),
                ("Show/xlights_networks.xml", b"<Networks/>"),
                ("Show/Sequences/Made Up Song.xsq", SEQUENCE.as_bytes()),
                ("Show/Old Song.xml", SEQUENCE.as_bytes()),
                ("Show/notes.xml", b"<notes/>"),
                ("Show/Music/Made Up Song.mp3", b"ID3 not really music"),
                ("Show/images/photo.jpg", b"jpeg"),
                ("__MACOSX/Show/._Made Up Song.xsq", b"fork"),
            ]),
        );
        let package = Package::open(&path).unwrap();
        assert_eq!(package.kind(), PackageKind::Zip);
        assert_eq!(
            package.sequences(),
            ["Show/Sequences/Made Up Song.xsq", "Show/Old Song.xml"]
        );
        assert!(package.has_layout());
        let (name, file) = package.read_sequence(None).unwrap();
        assert_eq!(name, "Show/Sequences/Made Up Song.xsq");
        assert_eq!(file.head.song, "Made Up Song");
        assert!(package.read_layout().unwrap().unwrap().contains("MegaTree 16x50"));
        assert_eq!(
            package.music_for(&name, Some(&file.head.media_file)),
            Some("Show/Music/Made Up Song.mp3")
        );
        assert!(package.read_sequence(Some("Show/notes.xml")).is_err());

        let music = dir.path().join("show/music");
        let copied = package.copy_music("Show/Music/Made Up Song.mp3", &music).unwrap();
        assert_eq!(copied, music.join("Made Up Song.mp3"));
        assert_eq!(std::fs::read(&copied).unwrap(), b"ID3 not really music");
        // Again: the same file is used, not copied twice.
        assert_eq!(
            package.copy_music("Show/Music/Made Up Song.mp3", &music).unwrap(),
            copied
        );
        // A different file of that name is kept.
        std::fs::write(&copied, b"something else").unwrap();
        assert_eq!(
            package.copy_music("Show/Music/Made Up Song.mp3", &music).unwrap(),
            music.join("Made Up Song (2).mp3")
        );
        let names: Vec<_> = std::fs::read_dir(&music)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names.len(), 2, "no temporary files left: {names:?}");
    }

    #[test]
    fn entries_pointing_outside_the_package_are_never_read_or_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "evil.zip",
            &zip_of(&[
                ("../../escape.xsq", SEQUENCE.as_bytes()),
                ("/abs/escape.xsq", SEQUENCE.as_bytes()),
                ("ok/../../escape.mp3", b"music"),
                ("Song.xsq", SEQUENCE.as_bytes()),
            ]),
        );
        let package = Package::open(&path).unwrap();
        assert_eq!(package.sequences(), ["Song.xsq"]);
        assert!(package.music_for("Song.xsq", None).is_none());
        let into = dir.path().join("into");
        assert!(package.copy_music("ok/../../escape.mp3", &into).is_err());
        assert!(!dir.path().join("escape.mp3").exists());

        let only_escapes = write(
            dir.path(),
            "worse.zip",
            &zip_of(&[("../x.xsq", SEQUENCE.as_bytes())]),
        );
        let error = Package::open(&only_escapes).unwrap_err().to_string();
        assert!(error.contains("There's no xLights sequence"), "{error}");
    }

    #[test]
    fn too_many_entries_or_too_many_bytes_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<(String, Vec<u8>)> = (0..12).map(|i| (format!("f{i}.txt"), vec![b'x'; 100])).collect();
        let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        let path = write(dir.path(), "many.zip", &zip_of(&refs));
        let few = Limits {
            entries: 10,
            ..Limits::default()
        };
        assert!(
            Package::open_with(&path, few)
                .unwrap_err()
                .to_string()
                .contains("12 files")
        );
        let small = Limits {
            total_bytes: 1000,
            ..Limits::default()
        };
        assert!(
            Package::open_with(&path, small)
                .unwrap_err()
                .to_string()
                .contains("unpacks to more")
        );

        // Music larger than the limit isn't copied, whatever the zip says its size is.
        let song = write(
            dir.path(),
            "big.zip",
            &zip_of(&[("Song.xsq", SEQUENCE.as_bytes()), ("Song.mp3", &[0u8; 5000])]),
        );
        let tight = Limits {
            music_bytes: 1000,
            ..Limits::default()
        };
        let package = Package::open_with(&song, tight).unwrap();
        assert!(
            package
                .copy_music("Song.mp3", &dir.path().join("m"))
                .unwrap_err()
                .to_string()
                .contains("larger")
        );
    }

    #[test]
    fn placeholders_and_other_files_say_what_to_do() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "Cloud.zip",
            b"{\"url\": \"https://drive.example/placeholder\"}",
        );
        let error = Package::open(&path).unwrap_err().to_string();
        assert!(error.contains("isn't a zip file PixelFlow can read"), "{error}");
    }

    #[test]
    fn a_folder_and_a_lone_sequence_find_their_layout() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Vendor Song");
        write(&folder, "xlights_rgbeffects.xml", LAYOUT.as_bytes());
        write(&folder, "Sequence/Song.xsq", SEQUENCE.as_bytes());
        write(&folder, "Sequence/Song.xbkp", SEQUENCE.as_bytes());
        write(&folder, "Music/Song.mp3", b"music");
        let package = Package::open(&folder).unwrap();
        assert_eq!(package.kind(), PackageKind::Folder);
        assert_eq!(package.sequences(), ["Sequence/Song.xsq"]);
        assert!(package.has_layout());
        assert_eq!(
            package.file_path("Sequence/Song.xsq"),
            Some(folder.join("Sequence/Song.xsq"))
        );

        let lone = Package::open(&folder.join("Sequence/Song.xsq")).unwrap();
        assert_eq!(lone.kind(), PackageKind::Sequence);
        assert!(lone.has_layout(), "the layout in the folder above");
        assert!(lone.read_layout().unwrap().unwrap().contains("Roofline"));
    }
}
