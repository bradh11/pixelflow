//! The music, as effects that follow it read it.
//!
//! A sequence's music is worked out once into an [`AudioTrack`] (`pf_analysis`: levels, bands,
//! spectrum, onsets, and beats at every frame), cached by whoever opens the sequence, and handed
//! to the [`crate::Renderer`] as an [`AudioSource`]. While it's still being worked out (or when
//! there's no music) effects draw as they do in silence, or as xLights does without music, and
//! curves that follow the music sit halfway between their values. Once it's there every frame
//! reads the same features, so playing, scrubbing, and export agree.
//!
//! Effects read the music for the frame they draw through [`Audio`], in the sequence's frames,
//! from the [`RenderContext`] the renderer passes along (see `Shader::in_context`).

use pf_sequence::{CurveInputs, Effect, EffectParams, Mark, Sequence, TendrilMovement, TimingTrack};
use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

pub use pf_analysis::{AudioTrack, NOTES, SPECTRUM_BANDS};

/// Whether the effect follows the music: curves or sparkles that follow it, a VU Meter that
/// reads it, a Tendril moving with it, or a Shape fired by it.
pub fn effect_follows_music(effect: &Effect) -> bool {
    effect.music_sparkles
        || effect.curves.values().any(|c| c.shape.follows_music())
        || match &effect.params {
            EffectParams::VuMeter(p) => p.meter.uses_music(),
            EffectParams::Tendril(p) => {
                matches!(
                    p.movement,
                    TendrilMovement::MusicLine | TendrilMovement::MusicCircle
                )
            }
            EffectParams::Shape(p) => p.fire_on_music && p.timing_track.is_none(),
            _ => false,
        }
}

/// Whether any of the sequence's effects follow the music.
pub fn follows_music(seq: &Sequence) -> bool {
    seq.effects().any(effect_follows_music)
}

/// The music's audio track for a renderer: none, ready, or still on its way (filled in once,
/// from another thread, by [`AudioFill::fill`]).
#[derive(Debug, Clone, Default)]
pub struct AudioSource(Option<Arc<OnceLock<Arc<AudioTrack>>>>);

/// Fills an [`AudioSource`] made by [`AudioSource::pending`].
#[derive(Debug, Clone)]
pub struct AudioFill(Arc<OnceLock<Arc<AudioTrack>>>);

impl AudioFill {
    /// Hands the track to every renderer with the source (the first fill wins).
    pub fn fill(&self, track: Arc<AudioTrack>) {
        let _ = self.0.set(track);
    }
}

impl AudioSource {
    /// No music.
    pub fn none() -> Self {
        Self(None)
    }

    /// Music that's ready.
    pub fn ready(track: Arc<AudioTrack>) -> Self {
        let slot = OnceLock::new();
        let _ = slot.set(track);
        Self(Some(Arc::new(slot)))
    }

    /// Music still being worked out, and what fills it in.
    pub fn pending() -> (Self, AudioFill) {
        let slot = Arc::new(OnceLock::new());
        (Self(Some(slot.clone())), AudioFill(slot))
    }

    /// The track, once it's there.
    pub fn track(&self) -> Option<&Arc<AudioTrack>> {
        self.0.as_ref()?.get()
    }

    /// Whether the source has music (ready or not).
    pub fn has_music(&self) -> bool {
        self.0.is_some()
    }

    /// Whether the two are the same source (the same music, filled in the same place).
    pub fn same(&self, other: &AudioSource) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// An audio track read in a sequence's frames (frame `n` starts at `n × frame_ms`). Past the end
/// of the music everything reads as silence.
#[derive(Debug, Clone, Copy)]
pub struct Audio<'a> {
    track: &'a AudioTrack,
    frame_ms: u32,
}

impl<'a> Audio<'a> {
    /// `track` in a sequence with `frame_ms` frames (a track worked out for another frame time
    /// is read at each frame's start).
    pub fn new(track: &'a AudioTrack, frame_ms: u32) -> Self {
        Self {
            track,
            frame_ms: frame_ms.max(1),
        }
    }

    pub fn track(&self) -> &'a AudioTrack {
        self.track
    }

    /// The track's frame for the sequence's frame `frame`.
    fn at(&self, frame: u64) -> u64 {
        if self.track.frame_ms() == self.frame_ms {
            frame
        } else {
            self.track
                .frame_at(frame.saturating_mul(u64::from(self.frame_ms)))
        }
    }

    /// The frame playing at `t_ms`.
    pub fn frame_at(&self, t_ms: u64) -> u64 {
        t_ms / u64::from(self.frame_ms)
    }

    /// How loud the frame is, 0–1 (in dB from the song's loudest).
    pub fn level(&self, frame: u64) -> f32 {
        self.track.level(self.at(frame))
    }

    /// The frame's highest sample as a share of the song's (xLights' `max`).
    pub fn peak(&self, frame: u64) -> f32 {
        self.track.peak(self.at(frame))
    }

    /// The frame's lowest sample as a share of the song's (xLights' `min`).
    pub fn trough(&self, frame: u64) -> f32 {
        self.track.trough(self.at(frame))
    }

    pub fn bass(&self, frame: u64) -> f32 {
        self.track.bass(self.at(frame))
    }

    pub fn low_mid(&self, frame: u64) -> f32 {
        self.track.low_mid(self.at(frame))
    }

    pub fn high_mid(&self, frame: u64) -> f32 {
        self.track.high_mid(self.at(frame))
    }

    pub fn treble(&self, frame: u64) -> f32 {
        self.track.treble(self.at(frame))
    }

    /// The spectrum, lowest band first, each 0–1.
    pub fn spectrum(&self, frame: u64) -> [f32; SPECTRUM_BANDS] {
        self.track.spectrum(self.at(frame))
    }

    /// xLights' spectrogram (each MIDI note's strength, 0–1); `None` past the music's end.
    pub fn notes(&self, frame: u64) -> Option<[f32; NOTES]> {
        self.track.notes(self.at(frame))
    }

    /// How much new sound starts in the frame, 0–1.
    pub fn onset(&self, frame: u64) -> f32 {
        self.track.onset(self.at(frame))
    }

    /// How loud the drums are, 0–1.
    pub fn percussive(&self, frame: u64) -> f32 {
        self.track.percussive(self.at(frame))
    }

    /// Whether a new note starts in the frame.
    pub fn is_note_on(&self, frame: u64) -> bool {
        self.track.is_note_on(self.at(frame))
    }

    /// Whether a beat falls in the frame.
    pub fn is_beat(&self, frame: u64) -> bool {
        self.track.is_beat(self.at(frame))
    }
}

/// What effects read besides their own settings while a frame is drawn: the music (when it's
/// there) and the sequence's timing tracks.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderContext<'a> {
    pub audio: Option<Audio<'a>>,
    pub tracks: &'a [TimingTrack],
    /// The sequence's frame time.
    pub frame_ms: u32,
}

impl<'a> RenderContext<'a> {
    pub fn new(audio: Option<Audio<'a>>, tracks: &'a [TimingTrack], frame_ms: u32) -> Self {
        Self {
            audio,
            tracks,
            frame_ms,
        }
    }

    /// `effect` as it plays at `t_ms`, its curves (and music sparkles) reading the music and
    /// timing tracks.
    pub fn effect_at<'e>(&self, effect: &'e Effect, t_ms: u64) -> Cow<'e, Effect> {
        let audio = self.audio;
        let peak = move |frame: u64| audio.map_or(0.0, |a| a.peak(frame));
        let inputs = CurveInputs {
            peak: audio.is_some().then_some(&peak as &dyn Fn(u64) -> f32),
            frame_ms: self.frame_ms,
            tracks: self.tracks,
        };
        effect.at_with(t_ms, &inputs)
    }

    /// The marks of the timing track `track`, if the sequence has it.
    pub fn marks(&self, track: Option<pf_sequence::TimingTrackId>) -> Option<&'a [Mark]> {
        let track = track?;
        self.tracks
            .iter()
            .find(|t| t.id == track)
            .map(|t| t.marks.as_slice())
    }
}
