//! Finding the music file a sequence was made for.

use std::path::{Path, PathBuf};

/// Audio file types PixelFlow can play.
const AUDIO_EXTENSIONS: [&str; 5] = ["mp3", "m4a", "wav", "ogg", "flac"];
/// How many directories to look through before giving up.
const MAX_DIRS: usize = 200;
/// How many folder levels below the sequence's folder (and its parent) to look.
const MAX_DEPTH: usize = 2;

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// Looks for the audio of the sequence at `sequence`: the file named in the sequence (`media`,
/// usually a path on the computer that made it) when it is still there, else a file with that
/// name or, failing that, an audio file with the sequence's name, in the sequence's folder, its
/// parent, and their subfolders (two levels). The search never reads all of the home folder or a
/// whole drive, and skips hidden and `Library` folders (reading those can make macOS ask for
/// permission, and they're slow).
pub fn find_audio(sequence: &Path, media: Option<&str>) -> Option<PathBuf> {
    find_audio_near(sequence, media, home_dir().as_deref())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// [`find_audio`], with the home folder given (for tests).
fn find_audio_near(sequence: &Path, media: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    let media = media.map(str::trim).filter(|m| !m.is_empty());
    let folder = match sequence.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if let Some(media) = media {
        let exact = Path::new(media);
        let exact = if exact.is_absolute() {
            exact.to_path_buf()
        } else {
            folder.join(exact)
        };
        if exact.is_file() {
            return Some(exact);
        }
    }
    let media_name = media.map(|m| m.rsplit(['/', '\\']).next().unwrap_or(m).to_string());
    let stem = sequence.file_stem()?.to_string_lossy().to_string();
    let named_like_sequence = |path: &Path| {
        is_audio(path)
            && path
                .file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&stem))
    };
    // Home and drive roots are only checked themselves: their folders hold everything else.
    let too_broad = |dir: &Path| dir.parent().is_none() || home.is_some_and(|h| h == dir);
    let mut queue: Vec<(PathBuf, usize)> = vec![(
        folder.to_path_buf(),
        if too_broad(folder) { MAX_DEPTH } else { 0 },
    )];
    if !too_broad(folder)
        && let Some(parent) = folder
            .parent()
            .filter(|p| !p.as_os_str().is_empty() && !too_broad(p))
    {
        queue.push((parent.to_path_buf(), 0));
    }
    let mut by_stem: Option<PathBuf> = None;
    let mut index = 0;
    // Breadth-first, so files closest to the sequence win.
    while index < queue.len() && index < MAX_DIRS {
        let (dir, depth) = queue[index].clone();
        index += 1;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                // Symbolic links aren't followed (no loops); the sequence's own folder is read once.
                if !name.starts_with('.') && name != "Library" && path != folder {
                    dirs.push(path);
                }
            } else if kind.is_file() || (kind.is_symlink() && path.is_file()) {
                files.push(path);
            }
        }
        files.sort();
        for file in files {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if media_name
                .as_deref()
                .is_some_and(|m| m.eq_ignore_ascii_case(&name))
            {
                return Some(file);
            }
            if by_stem.is_none() && named_like_sequence(&file) {
                by_stem = Some(file);
            }
        }
        if depth < MAX_DEPTH {
            dirs.sort();
            queue.extend(dirs.into_iter().map(|p| (p, depth + 1)));
        }
    }
    by_stem
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn finds_the_named_file_in_a_nearby_folder() {
        let dir = tempfile::tempdir().unwrap();
        let sequence = dir.path().join("Haas 2024/Christmas Medley 2017.fseq");
        touch(&sequence);
        let song = dir
            .path()
            .join("Haas 2024/MP3 Christmas Music/Christmas Medley 2017.mp3");
        touch(&song);
        let media = "/Users/someone/Documents/xlights/sequences/Haas 2024/MP3 Christmas Music/Christmas Medley 2017.mp3";
        assert_eq!(find_audio(&sequence, Some(media)), Some(song.clone()));
        assert_eq!(
            find_audio(&sequence, None),
            Some(song),
            "same name as the sequence"
        );
    }

    #[test]
    fn prefers_the_closest_match_and_gives_up_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let sequence = dir.path().join("show/Song.fseq");
        touch(&sequence);
        touch(&dir.path().join("show/deep/a/b/c/Song.mp3"));
        assert_eq!(
            find_audio(&sequence, Some("C:\\Music\\Missing.mp3")),
            None,
            "too deep, wrong name"
        );
        let near = dir.path().join("show/Song.M4A");
        touch(&near);
        assert_eq!(find_audio(&sequence, None), Some(near));
    }

    #[test]
    fn the_exact_media_path_wins_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let sequence = dir.path().join("show/Song.fseq");
        touch(&sequence);
        touch(&dir.path().join("show/Song.mp3"));
        let elsewhere = dir.path().join("music library/Song (radio edit).mp3");
        touch(&elsewhere);
        let media = elsewhere.display().to_string();
        assert_eq!(find_audio(&sequence, Some(&media)), Some(elsewhere));
    }

    #[test]
    fn the_media_name_beats_a_closer_file_named_like_the_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let sequence = dir.path().join("show/Song.fseq");
        touch(&sequence);
        touch(&dir.path().join("show/Song.mp3"));
        let named = dir.path().join("show/music/Real Title.mp3");
        touch(&named);
        assert_eq!(
            find_audio(&sequence, Some("D:\\xlights\\music\\Real Title.mp3")),
            Some(named)
        );
    }

    #[test]
    fn skips_hidden_and_library_folders() {
        let dir = tempfile::tempdir().unwrap();
        let sequence = dir.path().join("show/Song.fseq");
        touch(&sequence);
        touch(&dir.path().join("show/.cache/Song.mp3"));
        touch(&dir.path().join("show/Library/Song.mp3"));
        assert_eq!(find_audio(&sequence, None), None);
    }

    #[test]
    fn never_searches_the_whole_home_folder_or_a_drive() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        // ~/Downloads/Song.fseq: the parent is home, so its other folders stay unread.
        let sequence = home.join("Downloads/Song.fseq");
        touch(&sequence);
        touch(&home.join("Music/Song.mp3"));
        assert_eq!(find_audio_near(&sequence, None, Some(&home)), None);
        let next_to_it = home.join("Downloads/Song.mp3");
        touch(&next_to_it);
        assert_eq!(find_audio_near(&sequence, None, Some(&home)), Some(next_to_it));
        // ~/Song.fseq: only home itself is checked, not its folders.
        let at_home = home.join("Song.fseq");
        touch(&at_home);
        touch(&home.join("Desktop/Song.flac"));
        assert_eq!(find_audio_near(&at_home, None, Some(&home)), None);
    }
}
