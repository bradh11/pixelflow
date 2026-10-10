//! Running an export: frames drawn on several threads, encoded in order, the sound encoded
//! alongside, and the MP4 written atomically (nothing is left behind if it fails or is
//! cancelled).

use crate::VideoError;
use crate::aac;
use crate::ffmpeg::{self, Ffmpeg};
use crate::h264::H264;
use crate::mp4::{self, VideoTrack};
use crate::raster::{Canvas, Look, Photo, Scene};
use crate::timing::FrameClock;
use crate::yuv::Yuv;
use pf_engine::{DraftRenderer, SequenceExport, preview_props_of};
use pf_model::Background;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Frames each drawing thread takes at a time.
const CHUNK: u64 = 8;

/// What to export, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoOptions {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// The stretch of the sequence, in ms (the end is the sequence's end when `None`, and never
    /// past it).
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    /// Draw the layout photo behind the props.
    pub photo: bool,
    /// The dots' size against the preview's.
    pub pixel_size: f32,
    /// How much each lit dot glows, 0 (crisp dots) to 1.
    pub glow: f32,
    /// Encode with this ffmpeg instead of the built-in encoders.
    pub ffmpeg: Option<Ffmpeg>,
}

/// The steps of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Rendering,
    Sound,
    Writing,
}

impl Stage {
    /// What's being done, for the label by the progress bar.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Rendering => "Rendering frames",
            Stage::Sound => "Encoding the sound",
            Stage::Writing => "Writing the MP4",
        }
    }
}

/// How far an export has got: `fraction` (0–1) of `stage`, or `None` when it can't tell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub stage: Stage,
    pub fraction: Option<f32>,
}

/// What was written.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSummary {
    pub frames: u64,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// The file's size.
    pub bytes: u64,
    /// How the picture and sound were encoded, in words ("H.264 (OpenH264)", "AAC 192 kb/s").
    pub video: String,
    pub sound: Option<String>,
    /// How long the export took.
    pub elapsed_ms: u64,
    /// Things worth knowing (no music, a photo that couldn't be read).
    pub notes: Vec<String>,
}

/// The layout photo, read from its file (turned the way the camera says, as the preview shows
/// it).
pub fn load_photo(background: &Background) -> Result<Photo, String> {
    let path = pf_model::path_from_text(&background.path);
    let name = path
        .file_name()
        .map_or_else(|| background.path.clone(), |n| n.to_string_lossy().into_owned());
    let read = || -> Result<image::DynamicImage, image::ImageError> {
        use image::ImageDecoder;
        let mut decoder = image::ImageReader::open(&path)?
            .with_guessed_format()?
            .into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut picture = image::DynamicImage::from_decoder(decoder)?;
        picture.apply_orientation(orientation);
        Ok(picture)
    };
    let picture =
        read().map_err(|e| format!("The photo {name} couldn't be read, so the video has none: {e}"))?;
    Ok(Photo {
        image: picture.to_rgb8(),
        x: background.x,
        y: background.y,
        width: background.width,
        opacity: background.opacity,
    })
}

/// Removes the files it holds when dropped, unless kept.
struct TempFiles(Vec<PathBuf>);

impl Drop for TempFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

/// Where a temporary file for `path` goes: beside it, hidden, and unique to this export.
fn temp_beside(path: &Path, what: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    dir.join(format!(".{name}.{}.{n}.{what}", std::process::id()))
}

/// Where encoded frames go.
enum Sink {
    BuiltIn {
        encoder: Box<H264>,
        frames: BufWriter<File>,
        sizes: Vec<u32>,
        sync: Vec<bool>,
    },
    Ffmpeg(ffmpeg::Encoding),
}

impl Sink {
    fn frame(
        &mut self,
        picture: &Yuv,
        write_error: &impl Fn(std::io::Error) -> VideoError,
    ) -> Result<(), VideoError> {
        match self {
            Sink::BuiltIn {
                encoder,
                frames,
                sizes,
                sync,
            } => {
                let sample = encoder.encode(picture)?;
                frames.write_all(&sample.data).map_err(write_error)?;
                sizes.push(sample.data.len() as u32);
                sync.push(sample.sync);
                Ok(())
            }
            Sink::Ffmpeg(encoding) => encoding.frame(picture),
        }
    }
}

/// Renders `job`'s sequence as a video at `path` (written whole or not at all). `progress` hears
/// how it's going and returns `false` to cancel.
pub fn export_video(
    job: &SequenceExport,
    options: &VideoOptions,
    path: &Path,
    mut progress: impl FnMut(Progress) -> bool,
) -> Result<VideoSummary, VideoError> {
    let started = Instant::now();
    let shown = path.display().to_string();
    let write_error = |source| VideoError::Write {
        path: shown.clone(),
        source,
    };
    let sequence = job.sequence();
    let end = options
        .end_ms
        .unwrap_or(sequence.duration_ms)
        .min(sequence.duration_ms);
    let clock = FrameClock::new(options.start_ms, end, options.fps);
    if clock.frames() == 0 {
        return Err(VideoError::EmptyRange);
    }
    let mut notes = Vec::new();
    let photo = match (&job.show().background, options.photo) {
        (Some(background), true) => load_photo(background).map_err(|note| notes.push(note)).ok(),
        _ => None,
    };
    let look = Look {
        width: options.width,
        height: options.height,
        pixel_size: options.pixel_size,
        glow: options.glow,
    };
    let props = preview_props_of(job.show());
    let scene = Scene::new(&props, photo.as_ref(), &look).ok_or(VideoError::NothingToShow)?;
    if !progress(Progress {
        stage: Stage::Rendering,
        fraction: Some(0.0),
    }) {
        return Err(VideoError::Cancelled);
    }
    // What effects that follow the music read (worked out now if it isn't yet).
    let audio_source = job.audio_source();
    let music = job.music().filter(|m| m.is_file());
    if job.music().is_some() && music.is_none() {
        notes.push("The music wasn't found, so the video has no sound.".into());
    } else if job.music().is_none() {
        notes.push("The sequence has no music, so the video has no sound.".into());
    }

    let stop = AtomicBool::new(false);
    let sound_done = AtomicU64::new(0);
    let sound_total = AtomicU64::new(1);
    let out_tmp = temp_beside(path, "tmp");
    let frames_tmp = temp_beside(path, "frames.tmp");
    let _temps = TempFiles(vec![out_tmp.clone(), frames_tmp.clone()]);
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());

    let (sink, sound) = std::thread::scope(|scope| {
        // The sound, encoded alongside the frames (ffmpeg reads the music itself).
        let sound = match (music, &options.ffmpeg) {
            (Some(music), None) => Some(scope.spawn(|| {
                let rate = pf_audio::StereoFrames::open(music)
                    .map_err(|e| VideoError::Music(e.to_string()))?
                    .sample_rate();
                let (first, count) = clock.samples(rate);
                sound_total.store(aac::frame_count(count), Ordering::Relaxed);
                let pcm = aac::read_music(music, first, count, &stop)?;
                aac::encode(&pcm, (cores / 2).max(2), &sound_done, &stop)
            })),
            _ => None,
        };
        let mut frames = || -> Result<Sink, VideoError> {
            let sink = match &options.ffmpeg {
                None => Sink::BuiltIn {
                    encoder: Box::new(H264::new(scene.width(), scene.height(), clock.fps())?),
                    frames: BufWriter::with_capacity(
                        1 << 20,
                        File::create(&frames_tmp).map_err(write_error)?,
                    ),
                    sizes: Vec::with_capacity(clock.frames() as usize),
                    sync: Vec::with_capacity(clock.frames() as usize),
                },
                Some(ff) => {
                    let music = music.map(|path| ffmpeg::Music {
                        path,
                        start_us: clock.start_ms() * 1000,
                        duration_us: clock.duration_us(),
                    });
                    let args = ffmpeg::arguments(
                        ff,
                        scene.width(),
                        scene.height(),
                        clock.fps(),
                        music.as_ref(),
                        &out_tmp,
                    );
                    Sink::Ffmpeg(ffmpeg::Encoding::start(ff, &args)?)
                }
            };
            draw_frames(
                &scene,
                job,
                &audio_source,
                &clock,
                cores,
                &stop,
                sink,
                &write_error,
                &mut progress,
            )
        };
        let sink = frames();
        if sink.is_err() {
            // The sound stops too, so the scope's end doesn't wait for it.
            stop.store(true, Ordering::Relaxed);
        }
        let sound = sound.map(|handle| {
            while !handle.is_finished() {
                let done = sound_done.load(Ordering::Relaxed) as f32;
                let total = sound_total.load(Ordering::Relaxed).max(1) as f32;
                if sink.is_ok()
                    && !progress(Progress {
                        stage: Stage::Sound,
                        fraction: Some((done / total).min(1.0)),
                    })
                {
                    stop.store(true, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            handle
                .join()
                .unwrap_or(Err(VideoError::Encode("The sound couldn't be encoded.".into())))
        });
        (sink, sound)
    });
    let sink = sink?;
    if stop.load(Ordering::Relaxed) {
        return Err(VideoError::Cancelled);
    }

    let (video_name, sound_name) = match sink {
        Sink::BuiltIn {
            encoder,
            frames,
            sizes,
            sync,
        } => {
            frames.into_inner().map_err(|e| write_error(e.into_error()))?;
            let sound = match sound {
                Some(Ok(track)) => Some(track),
                Some(Err(VideoError::Cancelled)) => return Err(VideoError::Cancelled),
                Some(Err(error)) => {
                    notes.push(format!(
                        "The music couldn't be read, so the video has no sound: {error}"
                    ));
                    None
                }
                None => None,
            };
            let (sps, pps) = encoder
                .parameter_sets()
                .ok_or_else(|| VideoError::Encode("The video encoder gave no stream settings.".into()))?;
            let track = VideoTrack {
                width: scene.width(),
                height: scene.height(),
                fps: clock.fps(),
                sps: sps.to_vec(),
                pps: pps.to_vec(),
                sizes,
                sync,
            };
            let mut frames = BufReader::with_capacity(1 << 20, File::open(&frames_tmp).map_err(write_error)?);
            let file = File::create(&out_tmp).map_err(write_error)?;
            let mut out = BufWriter::with_capacity(1 << 20, file);
            let written = mp4::write(
                &mut out,
                &track,
                &mut frames,
                sound.as_ref(),
                &mut |done, total| {
                    progress(Progress {
                        stage: Stage::Writing,
                        fraction: Some(done as f32 / total.max(1) as f32),
                    })
                },
            );
            match written {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => return Err(VideoError::Cancelled),
                other => other.map_err(write_error)?,
            };
            let file = out.into_inner().map_err(|e| write_error(e.into_error()))?;
            file.sync_all().map_err(write_error)?;
            (
                "H.264 (OpenH264)".to_string(),
                sound.map(|_| format!("AAC {} kb/s", aac::BITRATE / 1000)),
            )
        }
        Sink::Ffmpeg(encoding) => {
            progress(Progress {
                stage: Stage::Writing,
                fraction: None,
            });
            encoding.finish()?;
            let codec = match options.ffmpeg.as_ref().map(|f| f.codec) {
                Some(ffmpeg::H264Codec::VideoToolbox) => "H.264 (ffmpeg, VideoToolbox)",
                _ => "H.264 (ffmpeg, x264)",
            };
            (
                codec.to_string(),
                music.map(|_| "AAC 256 kb/s (ffmpeg)".to_string()),
            )
        }
    };
    fs::rename(&out_tmp, path).map_err(write_error)?;
    let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(VideoSummary {
        frames: clock.frames(),
        duration_ms: clock.duration_us() / 1000,
        width: scene.width(),
        height: scene.height(),
        fps: clock.fps(),
        bytes,
        video: video_name,
        sound: sound_name,
        elapsed_ms: started.elapsed().as_millis() as u64,
        notes,
    })
}

/// Draws every frame on several threads, a chunk of frames at a time, and hands them to `sink`
/// in order. Threads stay at most a few chunks ahead of the encoder, so memory stays bounded.
#[allow(clippy::too_many_arguments)]
fn draw_frames(
    scene: &Scene,
    job: &SequenceExport,
    audio: &pf_render::AudioSource,
    clock: &FrameClock,
    cores: usize,
    stop: &AtomicBool,
    mut sink: Sink,
    write_error: &impl Fn(std::io::Error) -> VideoError,
    progress: &mut impl FnMut(Progress) -> bool,
) -> Result<Sink, VideoError> {
    let total = clock.frames();
    let chunks = total.div_ceil(CHUNK);
    let workers = cores.saturating_sub(1).clamp(1, 8);
    let ahead = workers as u64 * 2;
    let next = AtomicU64::new(0);
    // Chunks the encoder has finished with, and a wake-up for threads waiting to go ahead.
    let consumed = Mutex::new(0u64);
    let room = Condvar::new();
    let sequence = job.sequence();
    let frame_ms = sequence.frame_ms;
    let (tx, rx) = mpsc::channel::<(u64, Vec<Yuv>)>();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (next, consumed, room) = (&next, &consumed, &room);
            scope.spawn(move || {
                let mut renderer = DraftRenderer::new(job.show());
                renderer.set_audio(audio.clone());
                let mut canvas = Canvas::new(scene.width(), scene.height());
                let mut last: Option<(u64, Vec<u8>)> = None;
                loop {
                    let chunk = next.fetch_add(1, Ordering::Relaxed);
                    if chunk >= chunks {
                        return;
                    }
                    {
                        let mut done = consumed.lock().unwrap_or_else(PoisonError::into_inner);
                        while chunk >= *done + ahead && !stop.load(Ordering::Relaxed) {
                            done = room
                                .wait_timeout(done, Duration::from_millis(50))
                                .unwrap_or_else(PoisonError::into_inner)
                                .0;
                        }
                    }
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let frames = (chunk * CHUNK..((chunk + 1) * CHUNK).min(total))
                        .map(|n| {
                            let index = clock.sequence_frame(n, frame_ms);
                            if last.as_ref().is_none_or(|(at, _)| *at != index) {
                                let lights = renderer.frame(sequence, index * u64::from(frame_ms.max(1)));
                                last = Some((index, lights));
                            }
                            let lights = last.as_ref().map(|(_, l)| l.as_slice()).unwrap_or_default();
                            scene.draw(lights, &mut canvas);
                            Yuv::from_canvas(&canvas)
                        })
                        .collect();
                    if tx.send((chunk, frames)).is_err() {
                        return;
                    }
                }
            });
        }
        drop(tx);

        let mut waiting: BTreeMap<u64, Vec<Yuv>> = BTreeMap::new();
        let mut wanted = 0u64;
        let mut done = 0u64;
        let finish = |result: Result<Sink, VideoError>| {
            if result.is_err() {
                stop.store(true, Ordering::Relaxed);
            }
            // Wake waiting threads, to go on or to see the stop and end.
            room.notify_all();
            result
        };
        while wanted < chunks {
            let Ok((chunk, frames)) = rx.recv() else {
                return finish(Err(VideoError::Encode("Drawing the frames stopped.".into())));
            };
            waiting.insert(chunk, frames);
            while let Some(frames) = waiting.remove(&wanted) {
                for picture in &frames {
                    if let Err(error) = sink.frame(picture, write_error) {
                        return finish(Err(error));
                    }
                    done += 1;
                    if !progress(Progress {
                        stage: Stage::Rendering,
                        fraction: Some(done as f32 / total as f32),
                    }) {
                        return finish(Err(VideoError::Cancelled));
                    }
                }
                wanted += 1;
                *consumed.lock().unwrap_or_else(PoisonError::into_inner) = wanted;
                room.notify_all();
            }
        }
        finish(Ok(sink))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4::parse::*;
    use pf_engine::{Edit, Engine};
    use pf_model::{Generator, Prop, Rgb, ShapeSource};
    use pf_sequence::{Effect, EffectKind, Row, Target};

    /// A show with one 10-pixel line, and a sequence lighting it red from 0.5 s to 1 s.
    fn job(dir: &Path, music: Option<&Path>) -> SequenceExport {
        let mut engine = Engine::new(dir);
        let prop = Prop::new(
            "Line",
            ShapeSource::Generator(Generator::Line {
                nodes: 10,
                length: 2.0,
            }),
        );
        engine.apply(vec![Edit::AddProp { prop: prop.clone() }]).unwrap();
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0]
            .effects
            .push(Effect::new(EffectKind::On, 500, 1000).with_palette(vec![Rgb::new(255, 0, 0)]));
        let audio = music.map(pf_model::path_to_text);
        engine
            .new_sequence_doc_with_rows("Song", 2000, audio.as_deref(), vec![row])
            .unwrap();
        engine.sequence_export().unwrap()
    }

    fn options() -> VideoOptions {
        VideoOptions {
            width: 160,
            height: 96,
            fps: 30,
            start_ms: 0,
            end_ms: None,
            photo: false,
            pixel_size: 1.0,
            glow: 0.0,
            ffmpeg: None,
        }
    }

    /// A WAV of a 440 Hz tone, as 16-bit stereo at 48 kHz.
    fn tone_wav(path: &Path, seconds: u32) {
        let rate = 48_000u32;
        let n = rate * seconds;
        let mut data = Vec::with_capacity(n as usize * 4);
        for i in 0..n {
            let v = ((i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 12_000.0) as i16;
            data.extend_from_slice(&v.to_le_bytes());
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        fs::write(path, wav).unwrap();
    }

    #[test]
    fn a_sequence_with_music_becomes_an_mp4_with_both_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("tone.wav");
        tone_wav(&music, 3);
        let job = job(dir.path(), Some(&music));
        let out = dir.path().join("Song.mp4");
        let mut stages = Vec::new();
        let summary = export_video(&job, &options(), &out, |p| {
            if stages.last() != Some(&p.stage) {
                stages.push(p.stage);
            }
            true
        })
        .unwrap();
        assert_eq!(stages, vec![Stage::Rendering, Stage::Sound, Stage::Writing]);
        assert_eq!(summary.frames, 60);
        assert_eq!(summary.duration_ms, 2000);
        assert_eq!(summary.sound.as_deref(), Some("AAC 192 kb/s"));
        assert!(summary.notes.is_empty(), "{:?}", summary.notes);
        let file = fs::read(&out).unwrap();
        assert_eq!(summary.bytes, file.len() as u64);
        let top = atoms(&file, 0, file.len());
        let traks = top[1].all("trak");
        assert_eq!(traks.len(), 2);
        let video = samples(&file, traks[0].find("mdia/minf/stbl").unwrap());
        assert_eq!(video.len(), 60);
        // Two seconds of 48 kHz sound: 94 frames of 1024 and the encoder's delay.
        let sound = samples(&file, traks[1].find("mdia/minf/stbl").unwrap());
        assert_eq!(sound.len(), 95);
        let elst = traks[1].find("edts/elst").unwrap();
        assert_eq!(u32_at(&file, elst.start + 8), 2000, "sound as long as the video");
        // Only the finished file is left.
        let left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left.len(), 2, "{left:?}");
    }

    #[test]
    fn the_lights_change_on_the_frame_they_should() {
        // Decode the video back: frames before 0.5 s are dark, from frame 15 (0.5 s) red.
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), None);
        let out = dir.path().join("Song.mp4");
        let big = VideoOptions {
            pixel_size: 4.0,
            glow: 0.0,
            ..options()
        };
        let summary = export_video(&job, &big, &out, |_| true).unwrap();
        assert_eq!(
            summary.notes,
            vec!["The sequence has no music, so the video has no sound.".to_string()]
        );
        let file = fs::read(&out).unwrap();
        let top = atoms(&file, 0, file.len());
        let stbl = top[1].find("trak/mdia/minf/stbl").unwrap();
        let avcc = stbl.find("stsd/avc1/avcC").unwrap();
        let body = &file[avcc.start..avcc.end];
        let sps_len = usize::from(u16::from_be_bytes([body[6], body[7]]));
        let sps = &body[8..8 + sps_len];
        let pps = &body[8 + sps_len + 3..];
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        let annexb = |nal: &[u8]| [&[0, 0, 0, 1][..], nal].concat();
        decoder.decode(&[annexb(sps), annexb(pps)].concat()).unwrap();
        let mut red = Vec::new();
        for (offset, size) in samples(&file, stbl) {
            let sample = &file[offset as usize..][..size as usize];
            let mut stream = Vec::new();
            let mut at = 0;
            while at < sample.len() {
                let len = u32::from_be_bytes(sample[at..at + 4].try_into().unwrap()) as usize;
                stream.extend(annexb(&sample[at + 4..at + 4 + len]));
                at += 4 + len;
            }
            use openh264::formats::YUVSource;
            let picture = decoder.decode(&stream).unwrap().expect("a picture per sample");
            // Red shows as strong V (Cr) along the line.
            red.push(picture.v().iter().any(|&v| v > 180));
        }
        let first = red.iter().position(|&r| r);
        assert_eq!(first, Some(15), "lit from 0.5 s");
        assert_eq!(red.iter().rposition(|&r| r), Some(29), "dark again from 1 s");
    }

    #[test]
    fn a_range_exports_just_that_stretch() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), None);
        let out = dir.path().join("Part.mp4");
        let range = VideoOptions {
            start_ms: 500,
            end_ms: Some(1000),
            fps: 60,
            ..options()
        };
        let summary = export_video(&job, &range, &out, |_| true).unwrap();
        assert_eq!((summary.frames, summary.duration_ms), (30, 500));
        let empty = VideoOptions {
            start_ms: 1500,
            end_ms: Some(1500),
            ..options()
        };
        assert!(matches!(
            export_video(&job, &empty, &out, |_| true),
            Err(VideoError::EmptyRange)
        ));
        let past = VideoOptions {
            start_ms: 5000,
            ..options()
        };
        assert!(matches!(
            export_video(&job, &past, &out, |_| true),
            Err(VideoError::EmptyRange)
        ));
    }

    #[test]
    fn cancelling_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("tone.wav");
        tone_wav(&music, 3);
        let job = job(dir.path(), Some(&music));
        let out = dir.path().join("Song.mp4");
        for stop_at in [Stage::Rendering, Stage::Sound, Stage::Writing] {
            let mut calls = 0;
            let result = export_video(&job, &options(), &out, |p| {
                calls += 1;
                // Stop partway into the stage (the first rendering report is the start).
                !(p.stage == stop_at && calls > 3)
            });
            assert!(
                matches!(result, Err(VideoError::Cancelled)),
                "stopped in {stop_at:?}: {result:?}"
            );
            let left: Vec<_> = fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            assert_eq!(
                left,
                vec![std::ffi::OsString::from("tone.wav")],
                "stopped in {stop_at:?}"
            );
        }
    }

    #[test]
    fn nothing_to_show_without_props() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(dir.path());
        engine
            .new_sequence_doc_with_rows("Empty", 1000, None, vec![])
            .unwrap();
        let job = engine.sequence_export().unwrap();
        let err = export_video(&job, &options(), &dir.path().join("x.mp4"), |_| true).unwrap_err();
        assert!(matches!(err, VideoError::NothingToShow));
    }

    #[test]
    fn missing_music_and_photo_are_noted() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(dir.path());
        let prop = Prop::new(
            "Line",
            ShapeSource::Generator(Generator::Line {
                nodes: 4,
                length: 1.0,
            }),
        );
        engine.apply(vec![Edit::AddProp { prop }]).unwrap();
        let mut show = engine.show().clone();
        show.background = Some(Background {
            path: dir.path().join("gone.jpg").to_string_lossy().into_owned(),
            x: -1.0,
            y: 1.0,
            width: 2.0,
            opacity: 1.0,
        });
        engine.adopt_show(pf_engine::CheckedShow::new(show).unwrap());
        let gone = dir.path().join("gone.mp3");
        engine
            .new_sequence_doc_with_rows("Song", 500, Some(&pf_model::path_to_text(&gone)), vec![])
            .unwrap();
        let job = engine.sequence_export().unwrap();
        let photo = VideoOptions {
            photo: true,
            ..options()
        };
        let summary = export_video(&job, &photo, &dir.path().join("x.mp4"), |_| true).unwrap();
        assert_eq!(summary.notes.len(), 2, "{:?}", summary.notes);
        assert!(summary.notes[0].starts_with("The photo gone.jpg couldn't be read"));
        assert_eq!(
            summary.notes[1],
            "The music wasn't found, so the video has no sound."
        );
        assert_eq!(summary.sound, None);
    }

    #[test]
    fn photos_are_read_and_turned() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("house.png");
        image::RgbImage::from_pixel(8, 4, image::Rgb([1, 2, 3]))
            .save(&path)
            .unwrap();
        let photo = load_photo(&Background {
            path: path.to_string_lossy().into_owned(),
            x: 1.0,
            y: 2.0,
            width: 3.0,
            opacity: 0.5,
        })
        .unwrap();
        assert_eq!(photo.image.dimensions(), (8, 4));
        assert_eq!(photo.image.get_pixel(0, 0).0, [1, 2, 3]);
        assert_eq!(
            (photo.x, photo.y, photo.width, photo.opacity),
            (1.0, 2.0, 3.0, 0.5)
        );
    }
}
