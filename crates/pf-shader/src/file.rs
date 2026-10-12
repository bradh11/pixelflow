//! Reading a shader file from the disk: the one place that decides what is read as one.
//!
//! Whoever reads a shader file (the show's own folder, an import looking for the files a
//! sequence names) reads it through here, so the same things are refused everywhere: what
//! isn't named like a shader, what isn't a plain file, what is longer than a shader may be
//! ([`MAX_SHADER_BYTES`], as [`crate::Limits::source_bytes`] has it), and what isn't text.
//! Which paths may be read at all is the caller's to decide.

use std::fs;
use std::io::Read;
use std::path::Path;

/// The kinds of file read as a shader, by extension (`.fs` is what the format's own files use).
pub const SHADER_EXTENSIONS: [&str; 3] = ["fs", "frag", "glsl"];
/// Larger files are refused rather than read.
pub const MAX_SHADER_BYTES: u64 = 256 * 1024;

/// Whether `path` is named as a shader file is.
pub fn has_shader_extension(path: &Path) -> bool {
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

/// [`read_shader_file`], as the bytes the file holds (to keep a copy of it exactly as it is).
pub fn read_shader_bytes(path: &Path) -> Result<Vec<u8>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_read_as_far_as_a_shader_goes_and_no_further() {
        // No file is opened to find these out.
        for (name, shader) in [
            ("a.fs", true),
            ("A.FRAG", true),
            ("b.glsl", true),
            ("c.txt", false),
            ("fs", false),
        ] {
            assert_eq!(has_shader_extension(Path::new(name)), shader, "{name}");
        }
        assert_eq!(
            read_shader_file(Path::new("/nowhere/at/all.txt")).unwrap_err(),
            "it isn't a shader file (.fs, .frag, or .glsl)"
        );
        assert_eq!(
            read_shader_file(Path::new("/nowhere/at/all.fs")).unwrap_err(),
            "it isn't there"
        );
        // The limit here is the limit a shader is compiled under.
        assert_eq!(MAX_SHADER_BYTES as usize, crate::Limits::default().source_bytes);
        assert_eq!(text_of(b"\xEF\xBB\xBFvoid main() {}"), Some("void main() {}"));
        assert_eq!(text_of(b"void main() {}\0"), None);
        assert_eq!(text_of(b"\xff\xfe"), None);
    }
}
