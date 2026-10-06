//! The files a show refers to: which aren't where they were, finding them again by name in the
//! show's folder, and pointing the show at the right ones.

use crate::edit::Edit;
use crate::error::EngineError;
use pf_model::{SequenceId, Show, file_name_of, path_from_text, path_to_text};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Folder levels below a searched folder that are looked through.
const MAX_DEPTH: usize = 3;
/// Most folders one search reads before giving up.
const MAX_DIRS: usize = 500;

/// Which file a show (or the open sequence) refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileRole {
    /// A sequence's rendered file (`.fseq`).
    Sequence { id: SequenceId },
    /// A sequence's music.
    Music { id: SequenceId },
    /// The layout's background photo.
    Photo,
    /// The 3D house model.
    HouseModel,
    /// The music of the sequence open on the Sequence screen.
    SequenceDocMusic,
}

/// A file that isn't where the show (or the open sequence) says it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingFile {
    pub file: FileRole,
    /// The file's name ("Christmas Medley 2017.mp3").
    pub name: String,
    /// Where it was (path text, see [`pf_model::path_to_text`]).
    pub path: String,
    /// What it belongs to ("Music for Medley", "Background photo").
    pub owner: String,
    /// "Christmas Medley 2017.mp3 isn't where it was."
    pub message: String,
}

impl MissingFile {
    fn new(file: FileRole, path: &str, owner: String) -> Self {
        let name = file_name_of(path);
        Self {
            file,
            message: format!("{name} isn't where it was."),
            name,
            path: path.to_string(),
            owner,
        }
    }
}

/// A missing file found again by a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundFile {
    pub file: FileRole,
    /// The file's name, as it was.
    pub name: String,
    /// Where it was (path text).
    pub from: String,
    /// Where it is now (path text).
    pub to: String,
}

/// Every file the show refers to, with what it belongs to.
fn files_of(show: &Show) -> Vec<(FileRole, String, &str)> {
    let mut files = Vec::new();
    for s in &show.sequences {
        files.push((
            FileRole::Sequence { id: s.id },
            format!("Sequence file for {}", s.name),
            s.path.as_str(),
        ));
        if let Some(audio) = &s.audio {
            files.push((
                FileRole::Music { id: s.id },
                format!("Music for {}", s.name),
                audio,
            ));
        }
    }
    if let Some(background) = &show.background {
        files.push((FileRole::Photo, "Background photo".into(), &background.path));
    }
    if let Some(model) = &show.house_model {
        files.push((FileRole::HouseModel, "House model".into(), &model.path));
    }
    files
}

/// Whether the file at path text `path` is there.
pub(crate) fn exists(path: &str) -> bool {
    path_from_text(path).is_file()
}

/// The show's files that aren't where it says they are, in show order.
pub(crate) fn missing_files(show: &Show) -> Vec<MissingFile> {
    files_of(show)
        .into_iter()
        .filter(|(_, _, path)| !path.trim().is_empty() && !exists(path))
        .map(|(role, owner, path)| MissingFile::new(role, path, owner))
        .collect()
}

/// The missing music of the open sequence (`owner` names the sequence).
pub(crate) fn missing_music(path: &Path, sequence_name: &str) -> Option<MissingFile> {
    if path.is_file() {
        return None;
    }
    let name = match sequence_name.trim() {
        "" => "this sequence",
        name => name,
    };
    Some(MissingFile::new(
        FileRole::SequenceDocMusic,
        &path_to_text(path),
        format!("Music for {name}"),
    ))
}

/// The path text the show holds for `role`.
pub(crate) fn path_of(show: &Show, role: FileRole) -> Option<&str> {
    files_of(show)
        .into_iter()
        .find(|(r, _, _)| *r == role)
        .map(|(_, _, path)| path)
}

/// Points `role` in `show` at `to` (path text).
fn repoint(show: &mut Show, role: FileRole, to: String) -> Result<(), EngineError> {
    fn sequence(show: &mut Show, id: SequenceId) -> Result<&mut pf_model::SequenceEntry, EngineError> {
        show.sequences
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(EngineError::NotFound { kind: "sequence" })
    }
    match role {
        FileRole::Sequence { id } => sequence(show, id)?.path = to,
        FileRole::Music { id } => sequence(show, id)?.audio = Some(to),
        FileRole::Photo => {
            show.background
                .as_mut()
                .ok_or_else(|| EngineError::InvalidEdit("The show has no background photo.".into()))?
                .path = to
        }
        FileRole::HouseModel => {
            show.house_model
                .as_mut()
                .ok_or_else(|| EngineError::InvalidEdit("The show has no house model.".into()))?
                .path = to
        }
        FileRole::SequenceDocMusic => {
            return Err(EngineError::InvalidEdit(
                "The open sequence's music isn't part of the show.".into(),
            ));
        }
    }
    Ok(())
}

/// The edits that point the show's files as `changes` say: `(role, new path text)`. All of them
/// together are one undo step.
pub(crate) fn repoint_edits(show: &Show, changes: &[(FileRole, String)]) -> Result<Vec<Edit>, EngineError> {
    let mut next = show.clone();
    for (role, to) in changes {
        repoint(&mut next, *role, to.clone())?;
    }
    let mut edits: Vec<Edit> = next
        .sequences
        .iter()
        .zip(&show.sequences)
        .filter(|(after, before)| after != before)
        .map(|(after, _)| Edit::UpdateSequence {
            sequence: after.clone(),
        })
        .collect();
    if next.background != show.background {
        edits.push(Edit::SetBackground {
            background: next.background.clone(),
        });
    }
    if next.house_model != show.house_model {
        edits.push(Edit::SetHouseModel {
            house_model: next.house_model.clone(),
        });
    }
    Ok(edits)
}

/// Looks for missing files by name in a few folders (the show's folder and the folders below
/// it), copied out of the engine so the search doesn't hold it.
#[derive(Debug, Clone)]
pub struct FileSearch {
    folders: Vec<PathBuf>,
    wanted: Vec<MissingFile>,
    home: Option<PathBuf>,
}

impl FileSearch {
    pub(crate) fn new(folders: Vec<PathBuf>, wanted: Vec<MissingFile>) -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        Self {
            folders,
            wanted,
            home,
        }
    }

    /// The folders searched, the first one first. Only files inside them (or folders below them)
    /// are ever found.
    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    /// What is looked for.
    pub fn wanted(&self) -> &[MissingFile] {
        &self.wanted
    }

    /// The same search, for `file` alone ("Find again" on one file).
    pub fn only(mut self, file: FileRole) -> Self {
        self.wanted.retain(|w| w.file == file);
        self
    }

    /// Finds each missing file by its name: in the searched folders and up to three folder levels
    /// below them, skipping hidden and `Library` folders and never following links. When several
    /// files have the name, the one in folders named like the old ones wins, then the closest.
    /// A search starting in the home folder or a drive's top folder only looks in that folder.
    pub fn run(&self) -> Vec<FoundFile> {
        let names: HashSet<String> = self.wanted.iter().map(|w| key(&w.name)).collect();
        if names.is_empty() {
            return Vec::new();
        }
        let mut candidates: HashMap<String, Vec<(PathBuf, usize)>> = HashMap::new();
        let mut read = 0;
        for folder in &self.folders {
            let too_broad = |dir: &Path| dir.parent().is_none() || self.home.as_deref() == Some(dir);
            let mut queue = vec![(folder.clone(), if too_broad(folder) { MAX_DEPTH } else { 0 })];
            let mut index = 0;
            // Breadth first, so the files closest to the folder come first.
            while index < queue.len() && read < MAX_DIRS {
                let (dir, depth) = queue[index].clone();
                index += 1;
                read += 1;
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                let mut dirs = Vec::new();
                for entry in entries.flatten() {
                    let Ok(kind) = entry.file_type() else {
                        continue;
                    };
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if kind.is_dir() {
                        if !name.starts_with('.') && name != "Library" {
                            dirs.push(entry.path());
                        }
                    } else if kind.is_file() && names.contains(&key(&name)) {
                        candidates
                            .entry(key(&name))
                            .or_default()
                            .push((entry.path(), depth));
                    }
                }
                if depth < MAX_DEPTH {
                    dirs.sort();
                    queue.extend(dirs.into_iter().map(|d| (d, depth + 1)));
                }
            }
        }
        self.wanted
            .iter()
            .filter_map(|missing| {
                let old = path_from_text(&missing.path);
                let best = candidates
                    .get(&key(&missing.name))?
                    .iter()
                    .filter(|(path, _)| *path != old)
                    .min_by_key(|(path, depth)| {
                        (std::cmp::Reverse(shared_folders(&missing.path, path)), *depth)
                    })?;
                Some(FoundFile {
                    file: missing.file,
                    name: missing.name.clone(),
                    from: missing.path.clone(),
                    to: path_to_text(&best.0),
                })
            })
            .collect()
    }
}

/// A file name for comparing: case doesn't matter.
fn key(name: &str) -> String {
    name.to_lowercase()
}

/// How many folders, counting up from the file, `old` (path text, `/` or `\` between folders)
/// and `new` have in common by name ("MP3 Music/Song.mp3" and "…/MP3 Music/Song.mp3": 1).
fn shared_folders(old: &str, new: &Path) -> usize {
    let old_folders = old.split(['/', '\\']).rev().skip(1).map(key);
    let new_folders = new
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .rev()
        .map(|c| key(&c.as_os_str().to_string_lossy()));
    old_folders.zip(new_folders).take_while(|(a, b)| a == b).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    fn wanted(path: &str) -> MissingFile {
        MissingFile::new(FileRole::Photo, path, "Background photo".into())
    }

    #[test]
    fn finds_by_name_preferring_folders_named_like_the_old_ones_then_the_closest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("show");
        touch(&root.join("a/House.JPG"));
        touch(&root.join("photos/house.jpg"));
        touch(&root.join("x/y/z/deep.png"));
        touch(&root.join("x/y/z/w/too-deep.png"));
        let search = FileSearch::new(
            vec![root.clone()],
            vec![
                wanted("/old/place/photos/house.jpg"),
                wanted("C:\\Users\\me\\deep.png"),
                wanted("/old/too-deep.png"),
                wanted("/old/nowhere.png"),
            ],
        );
        let found = search.run();
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].to, path_to_text(&root.join("photos/house.jpg")));
        assert_eq!(found[0].from, "/old/place/photos/house.jpg");
        assert_eq!(found[1].name, "deep.png");

        let closest = FileSearch::new(vec![root.clone()], vec![wanted("/elsewhere/House.jpg")]).run();
        assert_eq!(
            closest[0].to,
            path_to_text(&root.join("a/House.JPG")),
            "case doesn't matter"
        );
    }

    #[test]
    fn skips_hidden_and_library_folders_and_links() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("show");
        touch(&root.join(".cache/song.mp3"));
        touch(&root.join("Library/song.mp3"));
        touch(&dir.path().join("outside/song.mp3"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("outside"), root.join("linked")).unwrap();
            std::os::unix::fs::symlink(dir.path().join("outside/song.mp3"), root.join("song.mp3")).unwrap();
        }
        let found = FileSearch::new(vec![root], vec![wanted("/old/song.mp3")]).run();
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn never_searches_the_whole_home_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        touch(&home.join("Music/song.mp3"));
        let mut search = FileSearch::new(vec![home.clone()], vec![wanted("/old/song.mp3")]);
        search.home = Some(home.clone());
        assert!(search.run().is_empty());
        touch(&home.join("song.mp3"));
        assert_eq!(search.run()[0].to, path_to_text(&home.join("song.mp3")));
    }

    #[test]
    fn shared_folders_count_up_from_the_file() {
        assert_eq!(
            shared_folders("/a/MP3 Music/s.mp3", Path::new("/b/mp3 music/s.mp3")),
            1
        );
        assert_eq!(shared_folders("C:\\x\\a\\b\\s.mp3", Path::new("/y/a/b/s.mp3")), 2);
        assert_eq!(shared_folders("s.mp3", Path::new("/y/s.mp3")), 0);
    }
}
