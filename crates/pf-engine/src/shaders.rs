//! The shader files Shader effects run (see `pf_render::Shaders`): where an effect's `file`
//! setting points, which files may be read, and which the show has.
//!
//! - **Where they live:** a shader file chosen for an effect, or found for an imported sequence,
//!   is copied into a `shaders` folder next to the show file, and the effect stores
//!   `shaders/<name>`, so the show folder can move as a whole. While the show isn't saved there's
//!   no such folder: the effect stores the full path of the file as it was chosen. PixelFlow's
//!   own shaders (`pixelflow:<name>`) are part of the program and aren't files at all.
//! - **What may be read:** only files inside the show's `shaders` folder, and files the user
//!   chose this session ([`ShaderFiles::allow`]). A path an edit put in an effect is not enough
//!   on its own, and only a text file of a shader's size is read. What is read is then run only
//!   under `pf_shader`'s limits: a shader file is a program, and nothing in it is trusted.
//! - **When:** never inside the engine. The renderer's library reads a file on a thread of its
//!   own the first time it's drawn; exports read it where they run; and what the settings show
//!   of a file is read away from the engine too ([`ShaderFiles`] is shared, not borrowed).

use crate::pictures::keep_copy;
use pf_model::{file_name_of, is_full_path_text, path_from_text, path_to_text, relative_text, resolve_text};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The folder next to the show file that shader files are copied into.
pub const SHADERS_FOLDER: &str = "shaders";
/// The kinds of file read as a shader, by extension (`.fs` is what the format's own files use).
pub const SHADER_EXTENSIONS: [&str; 3] = ["fs", "frag", "glsl"];
/// Larger files are refused rather than read (as `pf_shader` refuses a longer shader).
pub const MAX_SHADER_BYTES: u64 = 256 * 1024;
/// The most names listed from the shaders folder.
const MAX_LISTED: usize = 500;

/// The open show's shader files: where they are and which may be read.
#[derive(Debug, Default)]
pub struct ShaderFiles {
    /// The show's folder, once it has been saved.
    folder: Mutex<Option<PathBuf>>,
    /// Files outside the shaders folder that may be read: ones the user chose this session.
    chosen: Mutex<HashSet<PathBuf>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn has_shader_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SHADER_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The text in a file's bytes, if text is what they are: without the byte-order mark some
/// editors start a file with, which isn't part of it.
fn text_of(bytes: &[u8]) -> Option<&str> {
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    std::str::from_utf8(text).ok().filter(|text| !text.contains('\0'))
}

/// Reads a shader file: only a real file with a shader's extension, at most
/// [`MAX_SHADER_BYTES`] of it, that is text. The error is why not, in a few words ("it isn't
/// there").
pub fn read_shader_file(path: &Path) -> Result<String, String> {
    let bytes = read_shader_bytes(path)?;
    Ok(text_of(&bytes).unwrap_or_default().to_string())
}

/// [`read_shader_file`], as the bytes the file holds.
fn read_shader_bytes(path: &Path) -> Result<Vec<u8>, String> {
    const NOT_A_SHADER: &str = "it isn't a shader file (.fs, .frag, or .glsl)";
    if !has_shader_extension(path) {
        return Err(NOT_A_SHADER.into());
    }
    let unreadable = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound => "it isn't there".to_string(),
        std::io::ErrorKind::PermissionDenied => "PixelFlow isn't allowed to read it".to_string(),
        kind => format!("it couldn't be opened ({kind})"),
    };
    // A pipe or device named like a shader is never opened (opening one can wait for ever).
    if !fs::metadata(path).map_err(unreadable)?.is_file() {
        return Err(NOT_A_SHADER.into());
    }
    let file = fs::File::open(path).map_err(unreadable)?;
    let too_large = || format!("it's larger than {} KB", MAX_SHADER_BYTES / 1024);
    let metadata = file.metadata().map_err(unreadable)?;
    if !metadata.is_file() {
        return Err(NOT_A_SHADER.into());
    }
    if metadata.len() > MAX_SHADER_BYTES {
        return Err(too_large());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_SHADER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() as u64 > MAX_SHADER_BYTES {
        return Err(too_large());
    }
    match text_of(&bytes) {
        Some(_) => Ok(bytes),
        None => Err("it isn't a text file".into()),
    }
}

impl ShaderFiles {
    /// The show's folder from now on (`None`: the show isn't saved). True when it changed.
    pub(crate) fn set_folder(&self, folder: Option<&Path>) -> bool {
        let mut current = lock(&self.folder);
        if current.as_deref() == folder {
            return false;
        }
        *current = folder.map(Path::to_path_buf);
        true
    }

    /// The folder shader files are copied into: `shaders` next to the show file, once it's
    /// saved.
    pub fn shaders_folder(&self) -> Option<PathBuf> {
        lock(&self.folder).as_ref().map(|f| f.join(SHADERS_FOLDER))
    }

    /// Lets `path` be read: a file the user chose in a dialog.
    pub fn allow(&self, path: &Path) {
        lock(&self.chosen).insert(path.to_path_buf());
    }

    /// The file an effect's `file` setting names: a full path as it is, anything else in the
    /// show's folder. `None` when nothing is named, for one of PixelFlow's own shaders (which is
    /// no file), or when the show isn't saved (so has no folder).
    pub fn resolve(&self, file: &str) -> Option<PathBuf> {
        let file = file.trim();
        if file.is_empty() || pf_render::is_built_in_shader(file) {
            return None;
        }
        if is_full_path_text(file) {
            return Some(path_from_text(file));
        }
        let folder = lock(&self.folder).clone()?;
        Some(path_from_text(&resolve_text(file, &folder)))
    }

    /// Whether `path` may be read: it's in the show's shaders folder (or a folder below it), or
    /// the user chose it.
    pub fn allowed(&self, path: &Path) -> bool {
        if lock(&self.chosen).contains(path) {
            return true;
        }
        let Some(shaders) = self.shaders_folder() else {
            return false;
        };
        path.strip_prefix(&shaders).is_ok_and(|inside| {
            let mut parts = inside.components().peekable();
            parts.peek().is_some() && parts.all(|c| matches!(c, Component::Normal(_)))
        })
    }

    /// What an effect stores for the shader file at `path`: `shaders/<name>` when it's in the
    /// show's shaders folder, its full path otherwise.
    pub fn stored(&self, path: &Path) -> String {
        let text = path_to_text(path);
        let folder = lock(&self.folder).clone();
        match folder {
            Some(folder) if path.starts_with(folder.join(SHADERS_FOLDER)) => relative_text(&text, &folder),
            _ => text,
        }
    }

    /// The text of the shader file an effect's `file` setting names, when it may be read (reads
    /// the disk). The error is why not, in a few words. One of PixelFlow's own is never asked
    /// for here: the renderer has those.
    pub fn read(&self, file: &str) -> Result<String, String> {
        if file.trim().is_empty() {
            return Err("no shader is chosen".into());
        }
        let Some(path) = self.resolve(file) else {
            return Err("the show isn't saved yet, so it has no shaders folder to look in".into());
        };
        // One on a network share the user hasn't chosen from is never looked at.
        if let Some(refusal) = pf_model::network_path_refusal_of(&path) {
            return Err(refusal);
        }
        if !self.allowed(&path) {
            return Err(
                "it isn't in the show's shaders folder; choose it again in the effect's settings".into(),
            );
        }
        read_shader_file(&path)
    }

    /// Takes the shader file at `path` (a file the user chose, or one an import found) for the
    /// show (reads and writes the disk): copied into the show's shaders folder under its own
    /// name, unless it's there already. The same file already there is used as it is; a
    /// different one of that name is kept, and this one saved as "name (2).fs". While the show
    /// isn't saved there's nowhere to copy it: the file itself is used. Returns what an effect
    /// stores for it.
    pub fn adopt(&self, path: &Path) -> Result<String, String> {
        let name = file_name_of(&path_to_text(path));
        let bytes = read_shader_bytes(path).map_err(|why| format!("{name} can't be used: {why}."))?;
        let Some(shaders) = self.shaders_folder() else {
            self.allow(path);
            return Ok(path_to_text(path));
        };
        if self.allowed(path) && path.starts_with(&shaders) {
            return Ok(self.stored(path));
        }
        let target = keep_copy(&shaders, path, &bytes, "shader files", "shaders")?;
        Ok(self.stored(&target))
    }

    /// Takes every shader file `doc`'s Shader effects name by a full path (an import found them
    /// on this computer) for the show, as [`ShaderFiles::adopt`] does, and points the effects at
    /// what they store now (reads and writes the disk). Returns what was done, as notes: the
    /// files copied, and any that couldn't be taken (those effects keep the path they had).
    pub fn adopt_all(&self, doc: &mut pf_sequence::Sequence) -> Vec<String> {
        let mut taken: HashMap<String, Result<String, String>> = HashMap::new();
        let mut notes = Vec::new();
        let saved = self.shaders_folder().is_some();
        let effects = doc
            .rows
            .iter_mut()
            .flat_map(|row| &mut row.layers)
            .flat_map(|layer| &mut layer.effects);
        for effect in effects {
            let pf_sequence::EffectParams::Shader(p) = &mut effect.params else {
                continue;
            };
            if !is_full_path_text(&p.file) || pf_model::is_refused_network_path(&p.file) {
                continue;
            }
            let path = path_from_text(&p.file);
            if !path.is_file() {
                continue;
            }
            match taken.entry(p.file.clone()).or_insert_with(|| self.adopt(&path)) {
                Ok(stored) => p.file = stored.clone(),
                Err(why) if !notes.contains(why) => notes.push(why.clone()),
                Err(_) => {}
            }
        }
        let mut copied: Vec<String> = taken
            .values()
            .filter_map(|stored| stored.as_ref().ok())
            .map(|stored| file_name_of(stored))
            .collect();
        copied.sort();
        copied.dedup();
        if !copied.is_empty() {
            notes.insert(
                0,
                match saved {
                    true => format!(
                        "Copied into the show's shaders folder, so the show keeps them wherever it goes: {}.",
                        copied.join(", ")
                    ),
                    false => format!(
                        "The show isn't saved yet, so the sequence's shader files were left where they are instead of being copied into the show's shaders folder: {}.",
                        copied.join(", ")
                    ),
                },
            );
        }
        notes
    }

    /// The shader files in the show's shaders folder, as effects store them (`shaders/<name>`),
    /// by name (reads the disk). None while the show isn't saved.
    pub fn listed(&self) -> Vec<String> {
        let Some(shaders) = self.shaders_folder() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(&shaders) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| has_shader_extension(p) && p.is_file())
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|n| !n.starts_with('.'))
            .take(MAX_LISTED)
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
            .into_iter()
            .map(|n| format!("{SHADERS_FOLDER}/{n}"))
            .collect()
    }
}

/// Every shader a Shader effect of the show can use, as its settings list them: PixelFlow's
/// own, then the files in the show's shaders folder by name (reads the disk, and compiles what
/// hasn't been: not for a thread that draws frames, nor with the engine held).
pub fn list_shaders(files: &ShaderFiles, shaders: &pf_render::Shaders) -> Vec<pf_render::ShaderInfo> {
    pf_render::built_in_shaders()
        .into_iter()
        .chain(files.listed())
        .map(|file| shaders.describe(&file))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADER: &str = "void main() { gl_FragColor = vec4(1.0); }";

    fn files_in(folder: &Path) -> ShaderFiles {
        let files = ShaderFiles::default();
        files.set_folder(Some(folder));
        files
    }

    #[test]
    fn only_the_shaders_folder_and_chosen_files_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let show = dir.path().join("show");
        fs::create_dir_all(show.join("shaders/sub")).unwrap();
        fs::write(show.join("shaders/rings.fs"), SHADER).unwrap();
        fs::write(show.join("shaders/sub/waves.fs"), SHADER).unwrap();
        fs::write(show.join("secret.fs"), SHADER).unwrap();
        fs::write(dir.path().join("outside.fs"), SHADER).unwrap();
        let files = files_in(&show);
        // In the shaders folder, by what an effect stores.
        assert_eq!(files.read("shaders/rings.fs").unwrap(), SHADER);
        assert_eq!(files.read("shaders/sub/waves.fs").unwrap(), SHADER);
        assert_eq!(
            files.read(&path_to_text(&show.join("shaders/rings.fs"))).unwrap(),
            SHADER
        );
        // Anywhere else, by a relative or a full path, is refused, and so is climbing out.
        let outside = path_to_text(&dir.path().join("outside.fs"));
        for file in [
            "secret.fs",
            "shaders/../secret.fs",
            "../outside.fs",
            "shaders/../../outside.fs",
            outside.as_str(),
        ] {
            let why = files.read(file).unwrap_err();
            assert!(
                why.contains("shaders folder") || why.contains("isn't there"),
                "{file}: {why}"
            );
        }
        assert!(
            !files.allowed(&show.join("shaders")),
            "the folder itself isn't a shader"
        );
        assert!(!files.allowed(&show.join("shaders/../secret.fs")));
        // Until the user chooses it.
        files.allow(&dir.path().join("outside.fs"));
        assert_eq!(files.read(&outside).unwrap(), SHADER);
        // What isn't there, or isn't chosen, says so plainly; a network share is never looked at.
        assert_eq!(files.read("shaders/gone.fs").unwrap_err(), "it isn't there");
        assert_eq!(files.read("  ").unwrap_err(), "no shader is chosen");
        let share = files.read("\\\\attacker\\drop\\rings.fs").unwrap_err();
        assert_eq!(
            Some(share),
            pf_model::network_path_refusal("\\\\attacker\\drop\\rings.fs")
        );
        // PixelFlow's own shaders aren't files, so there is nothing to resolve.
        assert_eq!(files.resolve("pixelflow:aurora"), None);
        // Without a saved show there's no folder: only chosen files are read.
        let unsaved = ShaderFiles::default();
        assert!(
            unsaved
                .read("shaders/rings.fs")
                .unwrap_err()
                .contains("isn't saved")
        );
        assert!(unsaved.read(&outside).unwrap_err().contains("shaders folder"));
        unsaved.allow(&dir.path().join("outside.fs"));
        assert_eq!(unsaved.read(&outside).unwrap(), SHADER);
    }

    #[test]
    fn only_text_files_of_a_shaders_size_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let shaders = dir.path().join("shaders");
        fs::create_dir_all(&shaders).unwrap();
        fs::write(shaders.join("notes.txt"), SHADER).unwrap();
        fs::create_dir_all(shaders.join("folder.fs")).unwrap();
        fs::write(shaders.join("RINGS.FS"), SHADER).unwrap();
        fs::write(
            shaders.join("marked.frag"),
            [b"\xEF\xBB\xBF".as_slice(), SHADER.as_bytes()].concat(),
        )
        .unwrap();
        fs::write(shaders.join("picture.fs"), b"\x89PNG\r\n\x1a\n\x00\x00\xff\xfe").unwrap();
        fs::write(shaders.join("nul.glsl"), b"void main() {}\0").unwrap();
        fs::write(shaders.join("huge.fs"), vec![b' '; MAX_SHADER_BYTES as usize + 1]).unwrap();
        fs::write(shaders.join("full.fs"), vec![b' '; MAX_SHADER_BYTES as usize]).unwrap();
        let files = files_in(dir.path());
        for file in ["shaders/notes.txt", "shaders/folder.fs"] {
            let why = files.read(file).unwrap_err();
            assert!(why.contains("isn't a shader file"), "{file}: {why}");
        }
        assert_eq!(files.read("shaders/RINGS.FS").unwrap(), SHADER);
        assert_eq!(
            files.read("shaders/marked.frag").unwrap(),
            SHADER,
            "less its byte-order mark"
        );
        assert_eq!(
            files.read("shaders/picture.fs").unwrap_err(),
            "it isn't a text file"
        );
        assert_eq!(
            files.read("shaders/nul.glsl").unwrap_err(),
            "it isn't a text file"
        );
        assert_eq!(
            files.read("shaders/huge.fs").unwrap_err(),
            "it's larger than 256 KB"
        );
        assert_eq!(
            files.read("shaders/full.fs").unwrap().len(),
            MAX_SHADER_BYTES as usize
        );
    }

    #[test]
    fn a_chosen_shader_is_copied_into_the_shows_shaders_folder() {
        let dir = tempfile::tempdir().unwrap();
        let show = dir.path().join("show");
        fs::create_dir_all(&show).unwrap();
        let picked = dir.path().join("Downloads");
        fs::create_dir_all(&picked).unwrap();
        fs::write(picked.join("wavy lines.fs"), SHADER).unwrap();
        let files = files_in(&show);
        // Copied under its own name (the folder is made), and stored relative to the show.
        let stored = files.adopt(&picked.join("wavy lines.fs")).unwrap();
        assert_eq!(stored, "shaders/wavy lines.fs");
        assert_eq!(
            fs::read_to_string(show.join("shaders/wavy lines.fs")).unwrap(),
            SHADER
        );
        assert_eq!(files.read(&stored).unwrap(), SHADER);
        assert!(picked.join("wavy lines.fs").is_file(), "the original stays");
        // The same file again is the same file; one already in the folder is used where it is.
        assert_eq!(files.adopt(&picked.join("wavy lines.fs")).unwrap(), stored);
        assert_eq!(files.adopt(&show.join("shaders/wavy lines.fs")).unwrap(), stored);
        assert_eq!(fs::read_dir(show.join("shaders")).unwrap().count(), 1);
        // A different file of the same name is kept beside it.
        let other = format!("{SHADER}\n// changed");
        fs::write(picked.join("wavy lines.fs"), &other).unwrap();
        let second = files.adopt(&picked.join("wavy lines.fs")).unwrap();
        assert_eq!(second, "shaders/wavy lines (2).fs");
        assert_eq!(
            fs::read_to_string(show.join("shaders/wavy lines (2).fs")).unwrap(),
            other
        );
        assert_eq!(
            fs::read_to_string(show.join("shaders/wavy lines.fs")).unwrap(),
            SHADER
        );
        // The folder lists its shader files by name, and nothing else.
        fs::write(show.join("shaders/readme.txt"), "x").unwrap();
        fs::write(show.join("shaders/Aurora.FRAG"), SHADER).unwrap();
        assert_eq!(
            files.listed(),
            [
                "shaders/Aurora.FRAG",
                "shaders/wavy lines (2).fs",
                "shaders/wavy lines.fs"
            ]
        );
        // What isn't a shader file is refused, by name.
        fs::write(picked.join("list.txt"), "not a shader").unwrap();
        let err = files.adopt(&picked.join("list.txt")).unwrap_err();
        assert!(
            err.starts_with("list.txt can't be used: it isn't a shader file"),
            "{err}"
        );
        let err = files.adopt(&picked.join("gone.fs")).unwrap_err();
        assert_eq!(err, "gone.fs can't be used: it isn't there.");
        // While the show isn't saved, the file is used where it is.
        let unsaved = ShaderFiles::default();
        let stored = unsaved.adopt(&picked.join("wavy lines.fs")).unwrap();
        assert_eq!(stored, path_to_text(&picked.join("wavy lines.fs")));
        assert_eq!(unsaved.read(&stored).unwrap(), other);
        assert!(unsaved.listed().is_empty());
    }

    #[test]
    fn the_shows_shaders_are_listed_after_pixelflows_own() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("shaders")).unwrap();
        let rings = r#"/*{ "DESCRIPTION": "Rings.", "INPUTS": [ { "NAME": "rate", "TYPE": "float" } ] }*/ void main() { gl_FragColor = vec4(rate); }"#;
        fs::write(dir.path().join("shaders/rings.fs"), rings).unwrap();
        fs::write(dir.path().join("shaders/broken.fs"), "void main() { nothing(); }").unwrap();
        let files = std::sync::Arc::new(files_in(dir.path()));
        let reader = std::sync::Arc::clone(&files);
        let shaders = pf_render::Shaders::new(move |file| reader.read(file));
        let listed = list_shaders(&files, &shaders);
        let own = pf_render::built_in_shaders();
        assert!(own.len() >= 6);
        assert!(
            listed[..own.len()]
                .iter()
                .all(|s| s.built_in && s.problem.is_none())
        );
        let show: Vec<(&str, &str, bool)> = listed[own.len()..]
            .iter()
            .map(|s| (s.file.as_str(), s.name.as_str(), s.problem.is_some()))
            .collect();
        assert_eq!(
            show,
            [
                ("shaders/broken.fs", "broken", true),
                ("shaders/rings.fs", "rings", false)
            ]
        );
        let rings = listed.last().unwrap();
        assert_eq!((rings.description.as_str(), rings.inputs.len()), ("Rings.", 1));
        assert_eq!(
            listed[own.len()].problem.as_deref(),
            Some("line 1: there is no function `nothing`")
        );
    }

    #[test]
    fn an_imports_shader_files_are_taken_for_the_show_and_the_effects_pointed_at_them() {
        let dir = tempfile::tempdir().unwrap();
        let show = dir.path().join("show");
        fs::create_dir_all(&show).unwrap();
        let found = dir.path().join("xLights/Shaders");
        fs::create_dir_all(&found).unwrap();
        fs::write(found.join("rings.fs"), SHADER).unwrap();
        fs::write(found.join("broken.fs"), b"\xff\xfe\x00").unwrap();
        let effect = |file: String| {
            pf_sequence::Effect::new(pf_sequence::EffectKind::Shader, 0, 1000).with_params(
                pf_sequence::EffectParams::Shader(pf_sequence::ShaderParams {
                    file,
                    ..Default::default()
                }),
            )
        };
        let mut doc = pf_sequence::Sequence::new("s", 1000);
        let mut row = pf_sequence::Row::new(pf_sequence::Target::Prop(pf_model::PropId::new()));
        row.layers[0].effects = vec![
            effect(path_to_text(&found.join("rings.fs"))),
            effect(path_to_text(&found.join("rings.fs"))),
            effect(path_to_text(&found.join("broken.fs"))),
            effect(path_to_text(&found.join("missing.fs"))),
            effect("pixelflow:aurora".to_string()),
            effect("\\\\attacker\\drop\\rings.fs".to_string()),
        ];
        doc.rows.push(row);
        let files = files_in(&show);
        let notes = files.adopt_all(&mut doc);
        let stored: Vec<String> = doc.rows[0].layers[0]
            .effects
            .iter()
            .map(|e| match &e.params {
                pf_sequence::EffectParams::Shader(p) => p.file.clone(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(&stored[..2], ["shaders/rings.fs", "shaders/rings.fs"]);
        assert_eq!(
            stored[2],
            path_to_text(&found.join("broken.fs")),
            "left as it was"
        );
        assert_eq!(stored[3], path_to_text(&found.join("missing.fs")));
        assert_eq!(stored[4], "pixelflow:aurora");
        assert_eq!(
            stored[5], "\\\\attacker\\drop\\rings.fs",
            "a network share is never looked at"
        );
        assert_eq!(fs::read_dir(show.join("shaders")).unwrap().count(), 1);
        assert_eq!(
            notes,
            [
                "Copied into the show's shaders folder, so the show keeps them wherever it goes: rings.fs.",
                "broken.fs can't be used: it isn't a text file.",
            ]
        );
    }
}
