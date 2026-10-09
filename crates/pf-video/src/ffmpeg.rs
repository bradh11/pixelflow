//! Encoding with ffmpeg instead, when it's installed: x264 makes a sharper picture for its size,
//! and ffmpeg's own AAC encoder takes the sound straight from the music file. PixelFlow never
//! needs it; it's only used when found and chosen.
//!
//! The frames go to ffmpeg's input as raw YUV 4:2:0; the music is read by ffmpeg itself from the
//! export's start for as long as the video lasts (ffmpeg drops an MP3's encoder delay as playback
//! does, so the sound lines up the same way).

use crate::VideoError;
use crate::yuv::Yuv;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::JoinHandle;

/// An ffmpeg that can encode H.264.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ffmpeg {
    pub path: PathBuf,
    /// Its H.264 encoder: x264, or macOS's own (VideoToolbox) when it wasn't built with x264.
    pub codec: H264Codec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Codec {
    X264,
    VideoToolbox,
}

/// Where ffmpeg is usually installed, besides the folders on `PATH` (an app opened from the
/// Finder doesn't get the shell's `PATH`, so Homebrew's folders are looked in too).
const USUAL_FOLDERS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

/// The first ffmpeg found that can encode H.264, if any.
pub fn find() -> Option<Ffmpeg> {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut folders: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    folders.extend(USUAL_FOLDERS.iter().map(PathBuf::from));
    folders
        .into_iter()
        .map(|folder| folder.join(name))
        .filter(|path| path.is_file())
        .find_map(|path| {
            let output = Command::new(&path)
                .args(["-hide_banner", "-encoders"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            let codec = codec_from_encoders(&String::from_utf8_lossy(&output.stdout))?;
            Some(Ffmpeg { path, codec })
        })
}

/// The H.264 encoder to use, from `ffmpeg -encoders`' list.
fn codec_from_encoders(list: &str) -> Option<H264Codec> {
    let has = |name: &str| {
        list.lines()
            .any(|line| line.split_whitespace().nth(1) == Some(name))
    };
    if has("libx264") {
        Some(H264Codec::X264)
    } else if has("h264_videotoolbox") {
        Some(H264Codec::VideoToolbox)
    } else {
        None
    }
}

/// The music under the video: the file, and the stretch of it (start and length, in µs).
pub struct Music<'a> {
    pub path: &'a Path,
    pub start_us: u64,
    pub duration_us: u64,
}

fn seconds(us: u64) -> String {
    format!("{}.{:06}", us / 1_000_000, us % 1_000_000)
}

/// ffmpeg's arguments for a `width`×`height` video at `fps` written to `out`.
pub fn arguments(
    ffmpeg: &Ffmpeg,
    width: u32,
    height: u32,
    fps: u32,
    music: Option<&Music>,
    out: &Path,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    let color = [
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-colorspace",
        "bt709",
        "-color_range",
        "tv",
    ];
    push(&["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    push(&["-f", "rawvideo", "-pix_fmt", "yuv420p"]);
    push(&[
        "-video_size",
        &format!("{width}x{height}"),
        "-framerate",
        &fps.to_string(),
    ]);
    push(&color);
    push(&["-i", "pipe:0"]);
    if let Some(music) = music {
        push(&[
            "-ss",
            &seconds(music.start_us),
            "-t",
            &seconds(music.duration_us),
            "-i",
        ]);
        args.push(music.path.as_os_str().to_owned());
    }
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["-map", "0:v:0"]);
    if music.is_some() {
        push(&["-map", "1:a:0"]);
    }
    match ffmpeg.codec {
        H264Codec::X264 => push(&[
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "17",
            "-profile:v",
            "high",
        ]),
        H264Codec::VideoToolbox => {
            let rate = crate::h264::bitrate(width, height, fps) * 2;
            push(&["-c:v", "h264_videotoolbox", "-b:v", &rate.to_string()]);
        }
    }
    push(&["-pix_fmt", "yuv420p"]);
    push(&color);
    if music.is_some() {
        push(&["-c:a", "aac", "-b:a", "256k"]);
    }
    push(&["-movflags", "+faststart", "-f", "mp4"]);
    args.push(out.as_os_str().to_owned());
    args
}

/// A running ffmpeg taking frames.
pub struct Encoding {
    child: Child,
    input: Option<ChildStdin>,
    errors: Option<JoinHandle<String>>,
}

impl Encoding {
    pub fn start(ffmpeg: &Ffmpeg, args: &[OsString]) -> Result<Self, VideoError> {
        let mut child = Command::new(&ffmpeg.path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| VideoError::Ffmpeg(format!("ffmpeg couldn't start: {e}")))?;
        let input = child.stdin.take();
        // Read what ffmpeg says as it goes, so it never waits on a full pipe.
        let errors = child.stderr.take().map(|mut stderr| {
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = stderr.read_to_string(&mut text);
                text
            })
        });
        Ok(Self { child, input, errors })
    }

    pub fn frame(&mut self, picture: &Yuv) -> Result<(), VideoError> {
        let Some(input) = self.input.as_mut() else {
            return Err(VideoError::Ffmpeg("ffmpeg stopped taking frames.".into()));
        };
        for plane in picture.planes() {
            if input.write_all(plane).is_err() {
                return Err(self.failure());
            }
        }
        Ok(())
    }

    /// Ends the input and waits for ffmpeg to write the file.
    pub fn finish(mut self) -> Result<(), VideoError> {
        drop(self.input.take());
        match self.child.wait() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(self.failure()),
        }
    }

    /// What went wrong, in ffmpeg's last words.
    fn failure(&mut self) -> VideoError {
        drop(self.input.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        let said = self.errors.take().and_then(|h| h.join().ok()).unwrap_or_default();
        let last = said
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        if last.is_empty() {
            VideoError::Ffmpeg("ffmpeg stopped before the video was done.".into())
        } else {
            VideoError::Ffmpeg(format!("ffmpeg stopped before the video was done: {last}"))
        }
    }
}

/// An ffmpeg dropped before it finished (the export was cancelled or failed) is stopped.
impl Drop for Encoding {
    fn drop(&mut self) {
        drop(self.input.take());
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENCODERS: &str = "Encoders:\n V..... = Video\n ------\n V....D libx264              libx264 H.264 / AVC\n V....D h264_videotoolbox    VideoToolbox H.264 Encoder\n A....D aac                  AAC (Advanced Audio Coding)\n";

    #[test]
    fn x264_is_preferred_then_videotoolbox() {
        assert_eq!(codec_from_encoders(ENCODERS), Some(H264Codec::X264));
        let without = ENCODERS.replace("libx264 ", "libx265 ");
        assert_eq!(codec_from_encoders(&without), Some(H264Codec::VideoToolbox));
        assert_eq!(codec_from_encoders(" A....D aac  AAC\n"), None);
        // A name in a description doesn't count.
        assert_eq!(codec_from_encoders(" V....D mpeg4  not libx264 at all\n"), None);
    }

    #[test]
    fn arguments_pipe_frames_and_read_the_music_stretch() {
        let ffmpeg = Ffmpeg {
            path: PathBuf::from("/opt/homebrew/bin/ffmpeg"),
            codec: H264Codec::X264,
        };
        let music = Music {
            path: Path::new("/songs/ghost busters.mp3"),
            start_us: 12_500_000,
            duration_us: 20_033_333,
        };
        let args = arguments(&ffmpeg, 1920, 1080, 30, Some(&music), Path::new("/out/v.mp4.tmp"));
        let text: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let joined = text.join(" ");
        assert!(joined.contains("-f rawvideo -pix_fmt yuv420p -video_size 1920x1080 -framerate 30"));
        assert!(joined.contains("-i pipe:0 -ss 12.500000 -t 20.033333 -i /songs/ghost busters.mp3"));
        assert!(joined.contains("-map 0:v:0 -map 1:a:0 -c:v libx264"));
        assert!(joined.contains("-c:a aac"));
        assert!(joined.contains("-movflags +faststart"));
        assert_eq!(text.last().unwrap(), "/out/v.mp4.tmp");
        // The music's path is one argument, spaces and all.
        assert!(text.contains(&"/songs/ghost busters.mp3".to_string()));

        let silent = arguments(&ffmpeg, 1280, 720, 60, None, Path::new("o.mp4"));
        let joined: String = silent.iter().map(|a| a.to_string_lossy() + " ").collect();
        assert!(!joined.contains("1:a:0") && !joined.contains("-c:a"));
        let vt = Ffmpeg {
            codec: H264Codec::VideoToolbox,
            ..ffmpeg
        };
        let joined: String = arguments(&vt, 1280, 720, 60, None, Path::new("o.mp4"))
            .iter()
            .map(|a| a.to_string_lossy() + " ")
            .collect();
        assert!(joined.contains("-c:v h264_videotoolbox -b:v 11059200"));
    }
}
