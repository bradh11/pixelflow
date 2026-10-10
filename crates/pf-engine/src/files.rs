//! The files a show refers to: which aren't where they were, finding them again by name in the
//! show's folder, and pointing the show at the right ones.
//!
//! Nothing here that reads the disk runs inside the engine: a [`FileCheck`] or [`FileSearch`]
//! is copied out of it, run, and its result handed back, so a slow or dead network drive never
//! holds up edits.

use crate::edit::Edit;
use crate::error::EngineError;
use pf_model::{SequenceId, Show, file_name_of, path_from_text, path_to_text};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use unicode_normalization::UnicodeNormalization;

/// Folder levels below a searched folder that are looked through.
const MAX_DEPTH: usize = 3;
/// Most folders read in each searched folder.
const MAX_DIRS: usize = 500;
/// How long one search may take before PixelFlow stops looking.
const TIME_LIMIT: Duration = Duration::from_secs(3);

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
    /// A picture one of the open sequence's Picture effects draws.
    Picture,
}

/// A file that isn't where the show (or the open sequence) says it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingFile {
    pub file: FileRole,
    /// The file's name ("Christmas Medley 2017.mp3").
    pub name: String,
    /// Where the show looks for it now (path text, see [`pf_model::path_to_text`]).
    pub path: String,
    /// Where it was when the show was saved (path text): `path`, unless the show file moved
    /// without it.
    pub was_at: String,
    /// What it belongs to ("Music for Medley", "Background photo").
    pub owner: String,
    /// "Christmas Medley 2017.mp3 isn't where it was."
    pub message: String,
}

impl MissingFile {
    pub(crate) fn new(file: FileRole, path: &str, owner: String, was_at: Option<&str>) -> Self {
        let name = file_name_of(path);
        Self {
            file,
            message: format!("{name} isn't where it was."),
            name,
            path: path.to_string(),
            was_at: was_at.unwrap_or(path).to_string(),
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
    /// Where the show looked for it (path text).
    pub from: String,
    /// Where it is now (path text).
    pub to: String,
    /// Other files that fit just as well (path text), for the user to choose with Locate… if
    /// `to` is the wrong one.
    pub also: Vec<String>,
}

/// Every file the show refers to, with what it belongs to.
pub(crate) fn files_of(show: &Show) -> Vec<(FileRole, String, &str)> {
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

/// Whether the file at path text `path` is there (reads the disk).
pub(crate) fn exists(path: &str) -> bool {
    path_from_text(path).is_file()
}

/// Refuses a file the user chose that isn't there (reads the disk: call it before asking the
/// engine to use the file, not while holding it).
pub fn check_chosen_file(path: &Path) -> Result<(), EngineError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(EngineError::FileGone(file_name_of(&path_to_text(path))))
    }
}

/// The show's files the last check found missing, in show order (reads nothing).
pub(crate) fn missing_from(
    show: &Show,
    status: &HashMap<String, bool>,
    was_at: &HashMap<String, String>,
) -> Vec<MissingFile> {
    files_of(show)
        .into_iter()
        .filter(|(_, _, path)| status.get(*path) == Some(&false))
        .map(|(role, owner, path)| MissingFile::new(role, path, owner, was_at.get(path).map(String::as_str)))
        .collect()
}

/// Whether every file of the show has been checked.
pub(crate) fn all_checked(show: &Show, status: &HashMap<String, bool>) -> bool {
    files_of(show)
        .iter()
        .all(|(_, _, path)| path.trim().is_empty() || status.contains_key(*path))
}

/// Which of the show's files are there: copied out of the engine to [`run`](Self::run) without
/// holding it, then handed back with [`crate::Engine::publish_file_status`].
#[derive(Debug, Clone)]
pub struct FileCheck {
    pub(crate) generation: u64,
    pub(crate) paths: Vec<String>,
}

impl FileCheck {
    /// How many files it will look at.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Looks at each file (reads the disk).
    pub fn run(&self) -> FileStatus {
        FileStatus {
            generation: self.generation,
            there: self.paths.iter().map(|p| (p.clone(), exists(p))).collect(),
        }
    }
}

/// What a [`FileCheck`] found.
#[derive(Debug, Clone)]
pub struct FileStatus {
    pub(crate) generation: u64,
    pub(crate) there: HashMap<String, bool>,
}

/// Whether the open sequence's music is there: copied out of the engine to run without it.
#[derive(Debug, Clone)]
pub struct MusicCheck {
    pub(crate) path: PathBuf,
    pub(crate) sequence_name: String,
}

impl MusicCheck {
    /// The music, when it isn't there (reads the disk).
    pub fn run(&self) -> Option<MissingFile> {
        if self.path.is_file() {
            return None;
        }
        let name = match self.sequence_name.trim() {
            "" => "this sequence",
            name => name,
        };
        Some(MissingFile::new(
            FileRole::SequenceDocMusic,
            &path_to_text(&self.path),
            format!("Music for {name}"),
            None,
        ))
    }
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
        FileRole::Picture => {
            return Err(EngineError::InvalidEdit(
                "The open sequence's pictures aren't part of the show.".into(),
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
/// it, and where the show was saved), copied out of the engine so the search doesn't hold it.
#[derive(Debug, Clone)]
pub struct FileSearch {
    folders: Vec<PathBuf>,
    wanted: Vec<MissingFile>,
    home: Option<PathBuf>,
    generation: u64,
    time_limit: Duration,
}

/// What a [`FileSearch`] found.
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    pub(crate) generation: u64,
    pub found: Vec<FoundFile>,
    /// True when it stopped before looking everywhere (it took too long, or there were too many
    /// folders).
    pub gave_up: bool,
}

impl FileSearch {
    pub(crate) fn new(
        folders: Vec<PathBuf>,
        wanted: Vec<MissingFile>,
        home: Option<PathBuf>,
        generation: u64,
    ) -> Self {
        Self {
            folders,
            wanted,
            home,
            generation,
            time_limit: TIME_LIMIT,
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

    /// The same search, stopping after `limit` (three seconds unless changed).
    pub fn with_time_limit(mut self, limit: Duration) -> Self {
        self.time_limit = limit;
        self
    }

    /// Finds each missing file (reads the disk). A file back where the show looks for it is
    /// found there. Otherwise it is looked for by its name (case and Unicode spelling don't
    /// matter) in the searched folders and up to three folder levels below them, skipping hidden
    /// and `Library` folders and never following links. When several files have the name, the
    /// one in folders named like the old ones wins, then the closest; the others that fit as
    /// well are listed with it. A search starting in the home folder or a drive's top folder only
    /// looks in that folder.
    pub fn run(&self) -> SearchOutcome {
        let started = Instant::now();
        let mut gave_up = false;
        let mut found = Vec::new();
        let mut lost = Vec::new();
        for missing in &self.wanted {
            if exists(&missing.path) {
                found.push(FoundFile {
                    file: missing.file,
                    name: missing.name.clone(),
                    from: missing.path.clone(),
                    to: missing.path.clone(),
                    also: Vec::new(),
                });
            } else {
                lost.push(missing);
            }
        }
        let names: HashSet<String> = lost.iter().map(|w| key(&w.name)).collect();
        let mut candidates: HashMap<String, Vec<(PathBuf, usize)>> = HashMap::new();
        'folders: for folder in self.folders.iter().filter(|_| !names.is_empty()) {
            let too_broad = |dir: &Path| dir.parent().is_none() || self.home.as_deref() == Some(dir);
            let mut queue = vec![(folder.clone(), if too_broad(folder) { MAX_DEPTH } else { 0 })];
            let mut index = 0;
            // Breadth first, so the files closest to the folder come first.
            while index < queue.len() {
                if index >= MAX_DIRS || started.elapsed() >= self.time_limit {
                    gave_up = true;
                    if index >= MAX_DIRS {
                        continue 'folders;
                    }
                    break 'folders;
                }
                let (dir, depth) = queue[index].clone();
                index += 1;
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
        for missing in lost {
            let old = path_from_text(&missing.path);
            let Some(fits) = candidates.get(&key(&missing.name)) else {
                continue;
            };
            let score = |(path, depth): &(PathBuf, usize)| {
                (std::cmp::Reverse(shared_folders(&missing.was_at, path)), *depth)
            };
            let mut fits: Vec<&(PathBuf, usize)> = fits.iter().filter(|(path, _)| *path != old).collect();
            fits.sort_by(|a, b| score(a).cmp(&score(b)).then_with(|| a.0.cmp(&b.0)));
            let Some(best) = fits.first() else {
                continue;
            };
            let also = fits[1..]
                .iter()
                .filter(|f| score(f) == score(best))
                .map(|(path, _)| path_to_text(path))
                .collect();
            found.push(FoundFile {
                file: missing.file,
                name: missing.name.clone(),
                from: missing.path.clone(),
                to: path_to_text(&best.0),
                also,
            });
        }
        // In the order they were asked for.
        let order = |f: &FoundFile| self.wanted.iter().position(|w| w.file == f.file);
        found.sort_by_key(order);
        SearchOutcome {
            generation: self.generation,
            found,
            gave_up,
        }
    }
}

/// A file or folder name for comparing: case and Unicode spelling (composed or not) don't
/// matter.
fn key(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
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
        MissingFile::new(FileRole::Photo, path, "Background photo".into(), None)
    }

    fn search(folders: Vec<PathBuf>, wanted: Vec<MissingFile>) -> FileSearch {
        FileSearch::new(folders, wanted, None, 0)
    }

    #[test]
    fn finds_by_name_preferring_folders_named_like_the_old_ones_then_the_closest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("show");
        touch(&root.join("a/House.JPG"));
        touch(&root.join("photos/house.jpg"));
        touch(&root.join("x/y/z/deep.png"));
        touch(&root.join("x/y/z/w/too-deep.png"));
        let found = search(
            vec![root.clone()],
            vec![
                wanted("/old/place/photos/house.jpg"),
                wanted("C:\\Users\\me\\deep.png"),
                wanted("/old/too-deep.png"),
                wanted("/old/nowhere.png"),
            ],
        )
        .run()
        .found;
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].to, path_to_text(&root.join("photos/house.jpg")));
        assert_eq!(found[0].from, "/old/place/photos/house.jpg");
        assert!(found[0].also.is_empty(), "the photos folder fits best");
        assert_eq!(found[1].name, "deep.png");

        let closest = search(vec![root.clone()], vec![wanted("/elsewhere/House.jpg")])
            .run()
            .found;
        assert_eq!(
            closest[0].to,
            path_to_text(&root.join("a/House.JPG")),
            "case doesn't matter"
        );
    }

    #[test]
    fn files_that_fit_as_well_are_listed_not_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("show");
        touch(&root.join("b/house.jpg"));
        touch(&root.join("a/house.jpg"));
        let found = search(vec![root.clone()], vec![wanted("/old/house.jpg")])
            .run()
            .found;
        assert_eq!(found[0].to, path_to_text(&root.join("a/house.jpg")));
        assert_eq!(found[0].also, [path_to_text(&root.join("b/house.jpg"))]);
    }

    #[test]
    fn names_match_however_their_accents_are_spelled() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("show");
        // Decomposed on disk ("u" + combining accent, as older macOS disks write it).
        touch(&root.join("Mu\u{301}sica.mp3"));
        let found = search(vec![root], vec![wanted("C:\\Music\\M\u{fa}sica.mp3")])
            .run()
            .found;
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_file_back_in_its_place_is_found_there() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("show/house.jpg");
        touch(&photo);
        let found = search(vec![], vec![wanted(&path_to_text(&photo))]).run().found;
        assert_eq!(found[0].to, found[0].from);
    }

    #[test]
    fn each_folder_gets_its_own_budget() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big");
        for i in 0..MAX_DIRS {
            std::fs::create_dir_all(big.join(format!("d{i:04}"))).unwrap();
        }
        let show = dir.path().join("show");
        touch(&show.join("song.mp3"));
        let outcome = search(vec![big, show.clone()], vec![wanted("/old/song.mp3")]).run();
        assert!(outcome.gave_up, "the big folder ran out of budget");
        assert_eq!(outcome.found[0].to, path_to_text(&show.join("song.mp3")));
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
        let found = search(vec![root], vec![wanted("/old/song.mp3")]).run().found;
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn never_searches_the_whole_home_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        touch(&home.join("Music/song.mp3"));
        let search = FileSearch::new(
            vec![home.clone()],
            vec![wanted("/old/song.mp3")],
            Some(home.clone()),
            0,
        );
        assert!(search.run().found.is_empty());
        touch(&home.join("song.mp3"));
        assert_eq!(search.run().found[0].to, path_to_text(&home.join("song.mp3")));
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
