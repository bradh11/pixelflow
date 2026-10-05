//! Finding the music file a sequence was made for.

use std::path::{Path, PathBuf};

/// Audio file types PixelFlow can play.
const AUDIO_EXTENSIONS: [&str; 5] = ["mp3", "m4a", "wav", "ogg", "flac"];
/// How many directories to look through before giving up.
const MAX_DIRS: usize = 200;

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// Looks for the audio of the sequence at `sequence`: the file named in the sequence (`media`,
/// usually a path on the computer that made it), or an audio file with the sequence's name,
/// in the sequence's folder, its parent, and their subfolders (two levels).
pub fn find_audio(sequence: &Path, media: Option<&str>) -> Option<PathBuf> {
    let media_name = media
        .map(|m| m.rsplit(['/', '\\']).next().unwrap_or(m).trim().to_string())
        .filter(|m| !m.is_empty());
    let stem = sequence.file_stem()?.to_string_lossy().to_string();
    let wanted = |path: &Path| {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
            return false;
        };
        if media_name
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case(&name))
        {
            return true;
        }
        is_audio(path)
            && path
                .file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&stem))
    };
    let folder = sequence.parent()?;
    let mut queue: Vec<(PathBuf, usize)> = vec![(folder.to_path_buf(), 0)];
    if let Some(parent) = folder.parent() {
        queue.push((parent.to_path_buf(), 0));
    }
    let mut visited = 0;
    let mut index = 0;
    // Breadth-first, so files closest to the sequence win.
    while index < queue.len() && visited < MAX_DIRS {
        let (dir, depth) = queue[index].clone();
        index += 1;
        visited += 1;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        if let Some(found) = entries.iter().find(|p| p.is_file() && wanted(p)) {
            return Some(found.clone());
        }
        if depth < 2 {
            queue.extend(entries.into_iter().filter(|p| p.is_dir()).map(|p| (p, depth + 1)));
        }
    }
    None
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
}
