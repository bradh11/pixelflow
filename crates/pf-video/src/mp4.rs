//! Writing the MP4 file: an H.264 video track and, when there's music, an AAC sound track.
//!
//! The file's index (`moov`) goes before the media data (`mdat`), so a video starts playing
//! before it has all downloaded ("fast start"). Video and sound are stored in alternating
//! one-second chunks, so a player reading from the front finds each second's picture and sound
//! together. The video's frames come from a file written while encoding (a full song is hundreds
//! of megabytes), read through once, in order.

use crate::aac::{AacTrack, PRIMING, SAMPLES_PER_FRAME};
use std::io::{self, Read, Write};

/// Ticks per second in the video track (the usual MPEG clock: whole ticks at 30 and 60 fps).
pub const VIDEO_TIMESCALE: u32 = 90_000;
/// Ticks per second in the file's own times (track lengths, edits).
const MOVIE_TIMESCALE: u32 = 1000;

/// The encoded video, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTrack {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// The H.264 sequence and picture parameter sets.
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
    /// Each frame's size in bytes, in order.
    pub sizes: Vec<u32>,
    /// Each frame: whether playback can start there.
    pub sync: Vec<bool>,
}

impl VideoTrack {
    fn delta(&self) -> u32 {
        VIDEO_TIMESCALE / self.fps.max(1)
    }

    fn duration_ms(&self) -> u64 {
        self.sizes.len() as u64 * 1000 / u64::from(self.fps.max(1))
    }
}

/// A run of one track's samples stored together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Chunk {
    audio: bool,
    first: usize,
    count: usize,
}

/// The chunks in file order: a second of video, then the sound for that second, and so on.
fn chunks(video: &VideoTrack, audio: Option<&AacTrack>) -> Vec<Chunk> {
    let split = |total: usize, per: usize, audio: bool| -> Vec<Chunk> {
        (0..total)
            .step_by(per.max(1))
            .map(|first| Chunk {
                audio,
                first,
                count: per.max(1).min(total - first),
            })
            .collect()
    };
    let video_chunks = split(video.sizes.len(), video.fps as usize, false);
    let Some(audio) = audio else { return video_chunks };
    let per_second = (u64::from(audio.rate).div_ceil(SAMPLES_PER_FRAME)) as usize;
    let audio_chunks = split(audio.frames.len(), per_second, true);
    // Merge by start time (a video chunk first when they start together).
    let start = |c: &Chunk| -> f64 {
        if c.audio {
            c.first as f64 * SAMPLES_PER_FRAME as f64 / f64::from(audio.rate)
        } else {
            c.first as f64 / f64::from(video.fps)
        }
    };
    let mut merged = Vec::with_capacity(video_chunks.len() + audio_chunks.len());
    let (mut v, mut a) = (
        video_chunks.into_iter().peekable(),
        audio_chunks.into_iter().peekable(),
    );
    loop {
        let take_video = match (v.peek(), a.peek()) {
            (Some(vc), Some(ac)) => start(vc) <= start(ac),
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        merged.extend(if take_video { v.next() } else { a.next() });
    }
    merged
}

/// Writes the MP4 to `out`, reading the video's frames in order from `frames`. `progress` gets
/// (bytes written, bytes in all) as it goes and returns `false` to stop (the error is then
/// [`io::ErrorKind::Interrupted`]). Returns the file's size.
pub fn write(
    out: &mut impl Write,
    video: &VideoTrack,
    frames: &mut impl Read,
    audio: Option<&AacTrack>,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> io::Result<u64> {
    let payload: u64 = video.sizes.iter().map(|&s| u64::from(s)).sum::<u64>()
        + audio.map_or(0, |a| a.frames.iter().map(|f| f.len() as u64).sum());
    // 64-bit chunk offsets only when the file passes 4 GB.
    let co64 = payload > u64::from(u32::MAX) - (64 << 20);
    write_with(out, video, frames, audio, co64, payload, progress)
}

fn write_with(
    out: &mut impl Write,
    video: &VideoTrack,
    frames: &mut impl Read,
    audio: Option<&AacTrack>,
    co64: bool,
    payload: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> io::Result<u64> {
    let plan = chunks(video, audio);
    let ftyp = ftyp();
    let mdat_header = if co64 { 16 } else { 8 };
    // The index's size doesn't depend on the offsets in it: work it out, then fill them in.
    let blank = moov(video, audio, &plan, 0, co64);
    let data_start = (ftyp.len() + blank.len()) as u64 + mdat_header;
    let moov = moov(video, audio, &plan, data_start, co64);
    debug_assert_eq!(moov.len(), blank.len());
    let total = data_start + payload;

    out.write_all(&ftyp)?;
    out.write_all(&moov)?;
    if co64 {
        out.write_all(&1u32.to_be_bytes())?;
        out.write_all(b"mdat")?;
        out.write_all(&(payload + 16).to_be_bytes())?;
    } else {
        out.write_all(&((payload + 8) as u32).to_be_bytes())?;
        out.write_all(b"mdat")?;
    }
    let mut written = data_start;
    let mut buffer = Vec::new();
    for chunk in &plan {
        if chunk.audio {
            let Some(audio) = audio else { continue };
            for frame in &audio.frames[chunk.first..chunk.first + chunk.count] {
                out.write_all(frame)?;
                written += frame.len() as u64;
            }
        } else {
            let bytes: u64 = video.sizes[chunk.first..chunk.first + chunk.count]
                .iter()
                .map(|&s| u64::from(s))
                .sum();
            buffer.resize(bytes as usize, 0);
            frames.read_exact(&mut buffer)?;
            out.write_all(&buffer)?;
            written += bytes;
        }
        if !progress(written, total) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"));
        }
    }
    Ok(written)
}

/// Builds boxes: each is opened, filled, and closed (its size is written then).
#[derive(Default)]
struct Boxes {
    buf: Vec<u8>,
    open: Vec<usize>,
}

impl Boxes {
    fn begin(&mut self, kind: &[u8; 4]) -> &mut Self {
        self.open.push(self.buf.len());
        self.buf.extend_from_slice(&[0; 4]);
        self.buf.extend_from_slice(kind);
        self
    }

    /// A "full box": a version and 24 bits of flags after the kind.
    fn full(&mut self, kind: &[u8; 4], version: u8, flags: u32) -> &mut Self {
        self.begin(kind);
        self.u32((u32::from(version) << 24) | (flags & 0x00ff_ffff))
    }

    fn end(&mut self) -> &mut Self {
        if let Some(at) = self.open.pop() {
            let size = (self.buf.len() - at) as u32;
            self.buf[at..at + 4].copy_from_slice(&size.to_be_bytes());
        }
        self
    }

    fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    fn zeros(&mut self, n: usize) -> &mut Self {
        self.buf.resize(self.buf.len() + n, 0);
        self
    }

    /// The identity transform, as track and movie headers carry it.
    fn matrix(&mut self) -> &mut Self {
        for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
            self.u32(v);
        }
        self
    }
}

fn ftyp() -> Vec<u8> {
    let mut b = Boxes::default();
    b.begin(b"ftyp").bytes(b"isom").u32(0x200);
    for brand in [b"isom", b"iso2", b"avc1", b"mp41"] {
        b.bytes(brand);
    }
    b.end();
    b.buf
}

/// The index: the movie's header and a track for the video and one for the sound, with the
/// chunks' places in the file counted from `data_start`.
fn moov(
    video: &VideoTrack,
    audio: Option<&AacTrack>,
    plan: &[Chunk],
    data_start: u64,
    co64: bool,
) -> Vec<u8> {
    let mut video_offsets = Vec::new();
    let mut audio_offsets = Vec::new();
    let mut at = data_start;
    for chunk in plan {
        let range = chunk.first..chunk.first + chunk.count;
        if chunk.audio {
            audio_offsets.push(at);
            at += audio.map_or(0, |a| a.frames[range].iter().map(|f| f.len() as u64).sum());
        } else {
            video_offsets.push(at);
            at += video.sizes[range].iter().map(|&s| u64::from(s)).sum::<u64>();
        }
    }
    let audio_ms = audio.map_or(0, |a| a.samples * 1000 / u64::from(a.rate.max(1)));
    let mut b = Boxes::default();
    b.begin(b"moov");
    b.full(b"mvhd", 0, 0)
        .u32(0)
        .u32(0)
        .u32(MOVIE_TIMESCALE)
        .u32(video.duration_ms().max(audio_ms) as u32)
        .u32(0x0001_0000)
        .u16(0x0100)
        .zeros(10)
        .matrix()
        .zeros(24)
        .u32(if audio.is_some() { 3 } else { 2 })
        .end();
    video_trak(&mut b, video, &video_offsets, co64);
    if let Some(audio) = audio {
        audio_trak(&mut b, audio, &audio_offsets, co64);
    }
    b.end();
    b.buf
}

fn tkhd(b: &mut Boxes, id: u32, duration_ms: u64, audio: bool, size: (u32, u32)) {
    // Flags: enabled, in the movie.
    b.full(b"tkhd", 0, 3)
        .u32(0)
        .u32(0)
        .u32(id)
        .u32(0)
        .u32(duration_ms as u32)
        .zeros(8)
        .u16(0)
        .u16(0)
        .u16(if audio { 0x0100 } else { 0 })
        .u16(0)
        .matrix()
        .u32(size.0 << 16)
        .u32(size.1 << 16)
        .end();
}

fn mdhd_hdlr(b: &mut Boxes, timescale: u32, duration: u64, handler: &[u8; 4], name: &str) {
    let (version, wide) = if duration > u64::from(u32::MAX) {
        (1, true)
    } else {
        (0, false)
    };
    b.full(b"mdhd", version, 0);
    if wide {
        b.u64(0).u64(0).u32(timescale).u64(duration);
    } else {
        b.u32(0).u32(0).u32(timescale).u32(duration as u32);
    }
    // Language "und".
    b.u16(0x55c4).u16(0).end();
    b.full(b"hdlr", 0, 0)
        .u32(0)
        .bytes(handler)
        .zeros(12)
        .bytes(name.as_bytes())
        .u8(0)
        .end();
}

fn dinf(b: &mut Boxes) {
    b.begin(b"dinf").full(b"dref", 0, 0).u32(1);
    // The media is in this file.
    b.full(b"url ", 0, 1).end();
    b.end().end();
}

/// The sample-to-chunk, size, and chunk-offset tables shared by both tracks.
fn sample_tables(
    b: &mut Boxes,
    plan_counts: &[usize],
    sizes: impl Iterator<Item = u32>,
    offsets: &[u64],
    co64: bool,
) {
    // Runs of chunks with the same sample count: (first chunk, samples each).
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for (i, &count) in plan_counts.iter().enumerate() {
        if runs.last().is_none_or(|&(_, c)| c != count as u32) {
            runs.push((i as u32 + 1, count as u32));
        }
    }
    b.full(b"stsc", 0, 0).u32(runs.len() as u32);
    for (first, count) in runs {
        b.u32(first).u32(count).u32(1);
    }
    b.end();
    let sizes: Vec<u32> = sizes.collect();
    b.full(b"stsz", 0, 0).u32(0).u32(sizes.len() as u32);
    for size in sizes {
        b.u32(size);
    }
    b.end();
    if co64 {
        b.full(b"co64", 0, 0).u32(offsets.len() as u32);
        for &o in offsets {
            b.u64(o);
        }
    } else {
        b.full(b"stco", 0, 0).u32(offsets.len() as u32);
        for &o in offsets {
            b.u32(o as u32);
        }
    }
    b.end();
}

fn chunk_counts(total: usize, offsets: usize, per: usize) -> Vec<usize> {
    (0..offsets).map(|i| per.min(total - i * per)).collect()
}

fn video_trak(b: &mut Boxes, video: &VideoTrack, offsets: &[u64], co64: bool) {
    let frames = video.sizes.len();
    b.begin(b"trak");
    tkhd(b, 1, video.duration_ms(), false, (video.width, video.height));
    b.begin(b"mdia");
    mdhd_hdlr(
        b,
        VIDEO_TIMESCALE,
        frames as u64 * u64::from(video.delta()),
        b"vide",
        "VideoHandler",
    );
    b.begin(b"minf");
    b.full(b"vmhd", 0, 1).zeros(8).end();
    dinf(b);
    b.begin(b"stbl");
    b.full(b"stsd", 0, 0).u32(1);
    b.begin(b"avc1")
        .zeros(6)
        .u16(1)
        .zeros(16)
        .u16(video.width as u16)
        .u16(video.height as u16)
        .u32(0x0048_0000)
        .u32(0x0048_0000)
        .u32(0)
        .u16(1);
    let mut name = [0u8; 32];
    let label = b"PixelFlow H.264";
    name[0] = label.len() as u8;
    name[1..=label.len()].copy_from_slice(label);
    b.bytes(&name).u16(0x18).u16(0xffff);
    avcc(b, &video.sps, &video.pps);
    // BT.709 colors, limited range.
    b.begin(b"colr").bytes(b"nclx").u16(1).u16(1).u16(1).u8(0).end();
    b.end().end();
    b.full(b"stts", 0, 0)
        .u32(1)
        .u32(frames as u32)
        .u32(video.delta())
        .end();
    b.full(b"stss", 0, 0);
    let syncs: Vec<u32> = (0..frames)
        .filter(|&i| video.sync[i])
        .map(|i| i as u32 + 1)
        .collect();
    b.u32(syncs.len() as u32);
    for s in syncs {
        b.u32(s);
    }
    b.end();
    let counts = chunk_counts(frames, offsets.len(), video.fps as usize);
    sample_tables(b, &counts, video.sizes.iter().copied(), offsets, co64);
    b.end().end().end().end();
}

fn avcc(b: &mut Boxes, sps: &[u8], pps: &[u8]) {
    let at = |i: usize| sps.get(i).copied().unwrap_or(0);
    let profile = at(1);
    b.begin(b"avcC")
        .u8(1)
        .u8(profile)
        .u8(at(2))
        .u8(at(3))
        // 4-byte NAL lengths; one parameter set of each kind.
        .u8(0xff)
        .u8(0xe1)
        .u16(sps.len() as u16)
        .bytes(sps)
        .u8(1)
        .u16(pps.len() as u16)
        .bytes(pps);
    if matches!(profile, 100 | 110 | 122 | 144) {
        // 4:2:0, 8 bits, no extended parameter sets.
        b.u8(0xfd).u8(0xf8).u8(0xf8).u8(0);
    }
    b.end();
}

fn audio_trak(b: &mut Boxes, audio: &AacTrack, offsets: &[u64], co64: bool) {
    let frames = audio.frames.len();
    let edit_ms = audio.samples * u64::from(MOVIE_TIMESCALE) / u64::from(audio.rate.max(1));
    b.begin(b"trak");
    tkhd(b, 2, edit_ms, true, (0, 0));
    // The sound starts after the encoder's delay, and lasts as long as the real samples.
    b.begin(b"edts")
        .full(b"elst", 0, 0)
        .u32(1)
        .u32(edit_ms as u32)
        .u32(PRIMING as u32)
        .u16(1)
        .u16(0)
        .end()
        .end();
    b.begin(b"mdia");
    mdhd_hdlr(
        b,
        audio.rate,
        frames as u64 * SAMPLES_PER_FRAME,
        b"soun",
        "SoundHandler",
    );
    b.begin(b"minf");
    b.full(b"smhd", 0, 0).u16(0).u16(0).end();
    dinf(b);
    b.begin(b"stbl");
    b.full(b"stsd", 0, 0).u32(1);
    b.begin(b"mp4a")
        .zeros(6)
        .u16(1)
        .zeros(8)
        .u16(audio.channels)
        .u16(16)
        .u16(0)
        .u16(0)
        .u32(audio.rate << 16);
    esds(b, audio);
    b.end().end();
    b.full(b"stts", 0, 0)
        .u32(1)
        .u32(frames as u32)
        .u32(SAMPLES_PER_FRAME as u32)
        .end();
    let per = u64::from(audio.rate).div_ceil(SAMPLES_PER_FRAME) as usize;
    let counts = chunk_counts(frames, offsets.len(), per);
    sample_tables(
        b,
        &counts,
        audio.frames.iter().map(|f| f.len() as u32),
        offsets,
        co64,
    );
    // Each frame needs the one before it decoded first ("roll" of -1). Without this, Apple's
    // players assume a longer encoder delay and cut the sound short.
    b.full(b"sgpd", 1, 0)
        .bytes(b"roll")
        .u32(2)
        .u32(1)
        .u16(0xffff)
        .end();
    b.full(b"sbgp", 0, 0)
        .bytes(b"roll")
        .u32(1)
        .u32(frames as u32)
        .u32(1)
        .end();
    b.end().end().end().end();
}

/// The MPEG-4 elementary stream descriptor: AAC (object type 0x40) and its setup.
fn esds(b: &mut Boxes, audio: &AacTrack) {
    let largest = audio.frames.iter().map(Vec::len).max().unwrap_or(0) as u32;
    let total: u64 = audio.frames.iter().map(|f| f.len() as u64).sum();
    let seconds =
        (audio.frames.len() as u64 * SAMPLES_PER_FRAME).max(1) as f64 / f64::from(audio.rate.max(1));
    let average = (total as f64 * 8.0 / seconds) as u32;
    let descriptor = |tag: u8, body: &[u8]| -> Vec<u8> {
        let mut d = vec![tag];
        // The length in 7-bit groups, high first.
        let mut groups = Vec::new();
        let mut len = body.len();
        loop {
            groups.push((len & 0x7f) as u8);
            len >>= 7;
            if len == 0 {
                break;
            }
        }
        for (i, g) in groups.iter().rev().enumerate() {
            d.push(if i + 1 < groups.len() { g | 0x80 } else { *g });
        }
        d.extend_from_slice(body);
        d
    };
    let mut config = vec![0x40, 0x15];
    config.extend_from_slice(&largest.to_be_bytes()[1..]);
    config.extend_from_slice(&average.max(largest * 8).to_be_bytes());
    config.extend_from_slice(&average.to_be_bytes());
    config.extend(descriptor(0x05, &audio.config));
    let mut es = vec![0, 2, 0];
    es.extend(descriptor(0x04, &config));
    es.extend(descriptor(0x06, &[0x02]));
    b.full(b"esds", 0, 0).bytes(&descriptor(0x03, &es)).end();
}

/// Reading MP4 boxes back, to check what was written.
#[cfg(test)]
pub(crate) mod parse {
    /// A box: its kind, where its contents are, and the boxes inside it.
    #[derive(Debug, Clone)]
    pub struct Atom {
        pub kind: [u8; 4],
        pub start: usize,
        pub end: usize,
        pub children: Vec<Atom>,
    }

    /// Boxes whose contents are only other boxes (after `skip` bytes).
    fn inner(kind: &[u8; 4]) -> Option<usize> {
        match kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts" | b"dinf" => Some(0),
            b"stsd" | b"dref" => Some(8),
            b"avc1" => Some(78),
            b"mp4a" => Some(28),
            _ => None,
        }
    }

    pub fn atoms(data: &[u8], mut at: usize, end: usize) -> Vec<Atom> {
        let mut out = Vec::new();
        while at + 8 <= end {
            let size = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = data[at + 4..at + 8].try_into().unwrap();
            let (header, size) = if size == 1 {
                (
                    16,
                    u64::from_be_bytes(data[at + 8..at + 16].try_into().unwrap()) as usize,
                )
            } else {
                (8, size)
            };
            assert!(
                size >= header && at + size <= end,
                "box {:?} overruns",
                std::str::from_utf8(&kind)
            );
            let start = at + header;
            let children = inner(&kind).map_or_else(Vec::new, |skip| atoms(data, start + skip, at + size));
            out.push(Atom {
                kind,
                start,
                end: at + size,
                children,
            });
            at += size;
        }
        assert_eq!(at, end, "boxes fill their parent exactly");
        out
    }

    impl Atom {
        /// The first box down `path` ("mdia/minf/stbl").
        pub fn find(&self, path: &str) -> Option<&Atom> {
            let mut atom = self;
            for part in path.split('/') {
                atom = atom.children.iter().find(|c| c.kind == part.as_bytes())?;
            }
            Some(atom)
        }

        pub fn all(&self, kind: &str) -> Vec<&Atom> {
            self.children
                .iter()
                .filter(|c| c.kind == kind.as_bytes())
                .collect()
        }
    }

    pub fn u32_at(data: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(data[at..at + 4].try_into().unwrap())
    }

    pub fn u64_at(data: &[u8], at: usize) -> u64 {
        u64::from_be_bytes(data[at..at + 8].try_into().unwrap())
    }

    /// A full box's table of `width`-byte entries after its version, flags, and count.
    pub fn table(data: &[u8], atom: &Atom, width: usize) -> Vec<u64> {
        let count = u32_at(data, atom.start + 4) as usize;
        (0..count)
            .map(|i| {
                let at = atom.start + 8 + i * width;
                if width == 8 {
                    u64_at(data, at)
                } else {
                    u64::from(u32_at(data, at))
                }
            })
            .collect()
    }

    /// A track's samples, as (offset, size), read through its sample-to-chunk table.
    pub fn samples(data: &[u8], stbl: &Atom) -> Vec<(u64, u32)> {
        let stsz = stbl.find("stsz").unwrap();
        let count = u32_at(data, stsz.start + 8) as usize;
        let sizes: Vec<u32> = (0..count)
            .map(|i| u32_at(data, stsz.start + 12 + i * 4))
            .collect();
        let offsets = match stbl.find("stco") {
            Some(stco) => table(data, stco, 4),
            None => table(data, stbl.find("co64").unwrap(), 8),
        };
        let stsc = stbl.find("stsc").unwrap();
        let runs: Vec<(usize, usize)> = (0..u32_at(data, stsc.start + 4) as usize)
            .map(|i| {
                let at = stsc.start + 8 + i * 12;
                (u32_at(data, at) as usize, u32_at(data, at + 4) as usize)
            })
            .collect();
        let mut out = Vec::new();
        let mut sample = 0;
        for (chunk, &offset) in offsets.iter().enumerate() {
            let per = runs
                .iter()
                .rev()
                .find(|(first, _)| *first <= chunk + 1)
                .unwrap()
                .1;
            let mut at = offset;
            for _ in 0..per {
                out.push((at, sizes[sample]));
                at += u64::from(sizes[sample]);
                sample += 1;
            }
        }
        assert_eq!(sample, sizes.len(), "every sample is in a chunk");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::parse::*;
    use super::*;

    fn video(frames: usize, fps: u32) -> (VideoTrack, Vec<u8>) {
        let mut data = Vec::new();
        let mut sizes = Vec::new();
        for i in 0..frames {
            let size = 20 + (i % 7) * 3;
            data.extend(std::iter::repeat_n((i % 251) as u8, size));
            sizes.push(size as u32);
        }
        let track = VideoTrack {
            width: 1280,
            height: 720,
            fps,
            sps: vec![0x67, 66, 0xc0, 31, 1, 2],
            pps: vec![0x68, 0xce, 0x3c],
            sizes,
            sync: (0..frames).map(|i| i % 60 == 0).collect(),
        };
        (track, data)
    }

    fn audio(samples: u64, rate: u32) -> AacTrack {
        let n = crate::aac::frame_count(samples) as usize;
        AacTrack {
            rate,
            channels: 2,
            config: vec![0x12, 0x10],
            // Audio frames are marked 0xA0 + (n % 16), so they're told apart from video bytes.
            frames: (0..n).map(|i| vec![0xa0 + (i % 16) as u8; 10 + i % 5]).collect(),
            samples,
        }
    }

    fn written(video: &VideoTrack, data: &[u8], audio: Option<&AacTrack>, co64: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let payload = video.sizes.iter().map(|&s| u64::from(s)).sum::<u64>()
            + audio.map_or(0, |a| a.frames.iter().map(|f| f.len() as u64).sum());
        let size = write_with(
            &mut out,
            video,
            &mut &data[..],
            audio,
            co64,
            payload,
            &mut |_, _| true,
        )
        .unwrap();
        assert_eq!(size, out.len() as u64);
        out
    }

    #[test]
    fn index_first_then_interleaved_media() {
        let (v, data) = video(95, 30);
        let a = audio(3 * 44_100 + 500, 44_100);
        let file = written(&v, &data, Some(&a), false);
        let top = atoms(&file, 0, file.len());
        let kinds: Vec<&[u8]> = top.iter().map(|a| &a.kind[..]).collect();
        assert_eq!(kinds, vec![&b"ftyp"[..], b"moov", b"mdat"], "fast start");
        let moov = &top[1];
        let traks = moov.all("trak");
        assert_eq!(traks.len(), 2);

        // Every sample is where the index says, holding what was written for it.
        let vstbl = traks[0].find("mdia/minf/stbl").unwrap();
        let vs = samples(&file, vstbl);
        assert_eq!(vs.len(), 95);
        for (i, &(offset, size)) in vs.iter().enumerate() {
            assert_eq!(size, v.sizes[i]);
            assert!(
                file[offset as usize..][..size as usize]
                    .iter()
                    .all(|&b| b == (i % 251) as u8)
            );
        }
        let astbl = traks[1].find("mdia/minf/stbl").unwrap();
        let aud = samples(&file, astbl);
        assert_eq!(aud.len(), a.frames.len());
        for (i, &(offset, size)) in aud.iter().enumerate() {
            assert_eq!(&file[offset as usize..][..size as usize], &a.frames[i][..]);
        }
        // Chunks alternate: the second second's video comes after the first second's sound.
        let vchunks = table(&file, vstbl.find("stco").unwrap(), 4);
        let achunks = table(&file, astbl.find("stco").unwrap(), 4);
        assert_eq!(vchunks.len(), 4);
        assert!(vchunks[0] < achunks[0] && achunks[0] < vchunks[1] && vchunks[1] < achunks[1]);
        // All of the media data is used, once.
        let mdat = &top[2];
        let used: u64 = vs.iter().chain(&aud).map(|&(_, s)| u64::from(s)).sum();
        assert_eq!(used, (mdat.end - mdat.start) as u64);
    }

    #[test]
    fn track_lengths_and_timing() {
        let (v, data) = video(300, 30);
        let a = audio(10 * 48_000, 48_000);
        let file = written(&v, &data, Some(&a), false);
        let top = atoms(&file, 0, file.len());
        let moov = &top[1];
        let mvhd = moov.find("mvhd").unwrap();
        assert_eq!(u32_at(&file, mvhd.start + 12), 1000, "movie in ms");
        assert_eq!(u32_at(&file, mvhd.start + 16), 10_000);
        let traks = moov.all("trak");
        // Video: 300 frames at 3000 ticks each (90 kHz), 10 s.
        let vmdhd = traks[0].find("mdia/mdhd").unwrap();
        assert_eq!(u32_at(&file, vmdhd.start + 12), 90_000);
        assert_eq!(u32_at(&file, vmdhd.start + 16), 300 * 3000);
        let vtkhd = traks[0].find("tkhd").unwrap();
        assert_eq!(u32_at(&file, vtkhd.start + 20), 10_000);
        assert_eq!(u32_at(&file, vtkhd.start + 76), 1280 << 16);
        let stts = traks[0].find("mdia/minf/stbl/stts").unwrap();
        assert_eq!(
            (u32_at(&file, stts.start + 8), u32_at(&file, stts.start + 12)),
            (300, 3000)
        );
        let stss = table(&file, traks[0].find("mdia/minf/stbl/stss").unwrap(), 4);
        assert_eq!(stss, vec![1, 61, 121, 181, 241]);
        // Sound: the encoder's delay is skipped by the edit, which lasts exactly 10 s.
        let elst = traks[1].find("edts/elst").unwrap();
        assert_eq!(u32_at(&file, elst.start + 4), 1);
        assert_eq!(u32_at(&file, elst.start + 8), 10_000);
        assert_eq!(u32_at(&file, elst.start + 12), 1024);
        let amdhd = traks[1].find("mdia/mdhd").unwrap();
        assert_eq!(u32_at(&file, amdhd.start + 12), 48_000);
        assert_eq!(u32_at(&file, amdhd.start + 16), a.frames.len() as u32 * 1024);
        let astts = traks[1].find("mdia/minf/stbl/stts").unwrap();
        assert_eq!(u32_at(&file, astts.start + 8), a.frames.len() as u32);
        assert_eq!(u32_at(&file, astts.start + 12), 1024);
    }

    #[test]
    fn sample_entries_describe_the_codecs() {
        let (v, data) = video(10, 30);
        let a = audio(4096, 44_100);
        let file = written(&v, &data, Some(&a), false);
        let top = atoms(&file, 0, file.len());
        let traks = top[1].all("trak");
        let avc1 = traks[0].find("mdia/minf/stbl/stsd/avc1").unwrap();
        assert_eq!(u32_at(&file, avc1.start + 24) >> 16, 1280);
        let avcc = avc1.find("avcC").unwrap();
        let body = &file[avcc.start..avcc.end];
        assert_eq!(
            &body[..4],
            &[1, 66, 0xc0, 31],
            "version and the SPS's profile and level"
        );
        assert_eq!(body[4], 0xff, "4-byte lengths");
        assert_eq!(&body[8..14], &v.sps[..]);
        assert!(avc1.find("colr").is_some());
        let mp4a = traks[1].find("mdia/minf/stbl/stsd/mp4a").unwrap();
        assert_eq!(u32_at(&file, mp4a.start + 16) >> 16, 2, "two channels, 16 bits");
        assert_eq!(u32_at(&file, mp4a.start + 24) >> 16, 44_100);
        let esds = mp4a.find("esds").unwrap();
        let body = &file[esds.start + 4..esds.end];
        assert_eq!(body[0], 0x03);
        // The decoder config: AAC (0x40), an audio stream, and the setup bytes at the end.
        let config = body.iter().position(|&b| b == 0x04).unwrap();
        assert_eq!(&body[config + 2..config + 4], &[0x40, 0x15]);
        let setup = body.windows(4).position(|w| w == [0x05, 2, 0x12, 0x10]);
        assert!(setup.is_some(), "{body:02x?}");
        assert_eq!(&body[body.len() - 3..], &[0x06, 1, 0x02]);
        let hdlr = traks[1].find("mdia/hdlr").unwrap();
        assert_eq!(&file[hdlr.start + 8..hdlr.start + 12], b"soun");
    }

    #[test]
    fn video_alone_without_music() {
        let (v, data) = video(45, 30);
        let file = written(&v, &data, None, false);
        let top = atoms(&file, 0, file.len());
        assert_eq!(top[1].all("trak").len(), 1);
        let stbl = top[1].find("trak/mdia/minf/stbl").unwrap();
        // Chunks of a second: 30 then 15, as two runs.
        let stsc = stbl.find("stsc").unwrap();
        assert_eq!(u32_at(&file, stsc.start + 4), 2);
        assert_eq!(samples(&file, stbl).len(), 45);
    }

    #[test]
    fn big_files_use_64_bit_offsets() {
        let (v, data) = video(40, 30);
        let a = audio(50_000, 44_100);
        let file = written(&v, &data, Some(&a), true);
        let top = atoms(&file, 0, file.len());
        assert_eq!(u32_at(&file, top[2].start - 16), 1, "a 64-bit mdat size");
        let traks = top[1].all("trak");
        for trak in traks {
            let stbl = trak.find("mdia/minf/stbl").unwrap();
            assert!(stbl.find("stco").is_none());
            for (offset, size) in samples(&file, stbl) {
                assert!(offset as usize + size as usize <= file.len());
            }
        }
        let vs = samples(&file, top[1].find("trak/mdia/minf/stbl").unwrap());
        assert_eq!(file[vs[39].0 as usize], 39);
    }

    #[test]
    fn stopping_stops_writing() {
        let (v, data) = video(90, 30);
        let mut out = Vec::new();
        let mut calls = 0;
        let err = write(&mut out, &v, &mut &data[..], None, &mut |_, _| {
            calls += 1;
            calls < 2
        })
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn descriptor_lengths_past_127_bytes() {
        let mut a = audio(4096, 44_100);
        a.config = vec![7; 200];
        let (v, data) = video(2, 30);
        let file = written(&v, &data, Some(&a), false);
        let top = atoms(&file, 0, file.len());
        let esds = top[1].all("trak")[1]
            .find("mdia/minf/stbl/stsd/mp4a/esds")
            .unwrap();
        // The ES descriptor's length takes two bytes: 0x81 then the rest.
        let body = &file[esds.start + 4..esds.end];
        assert_eq!(body[0], 0x03);
        assert_eq!(body[1] & 0x80, 0x80);
        let len = (usize::from(body[1] & 0x7f) << 7) | usize::from(body[2]);
        assert_eq!(len, body.len() - 3);
    }
}
