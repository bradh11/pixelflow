//! PixelFlow sequence documents.
//!
//! A sequence (`*.pfseq.json`) is its own file, separate from the show: timed effects placed on
//! rows that target the show's props and groups by id, plus timing tracks (beats, bars, lyrics)
//! to line effects up with the music. Pure data: JSON conversion with schema migrations, size
//! limits, and validation; rendering lives in `pf-render`.

mod color;
mod document;
mod effect;
mod ids;
mod io;
mod limits;
mod settings;
mod timing;
mod validate;

pub use color::Rgb;
pub use document::{CURRENT_SCHEMA_VERSION, Layer, Mark, Row, Sequence, Target, TimingKind, TimingTrack};
pub use effect::{
    Axis, BarsParams, Blend, ChaseParams, ColorWashParams, Direction, Effect, EffectKind, EffectParams,
    FadeDirection, FadeParams, FireParams, Gradient, MeteorDirection, MeteorsParams, OffParams, OnParams,
    Palette, RippleParams, ShimmerParams, SpiralParams, StrobeParams, TwinkleParams, WaveParams,
};
pub use ids::{EffectId, RowId, TimingTrackId};
pub use io::{SequenceError, check_sequence, sequence_from_json, sequence_to_json};
pub use limits::{
    MAX_DURATION_MS, MAX_EFFECTS, MAX_FRAME_MS, MAX_LAYERS_PER_ROW, MAX_MARKS, MAX_PALETTE_COLORS, MAX_ROWS,
    MAX_SEQUENCE_BYTES, MAX_TEXT_LEN, MAX_TIMING_TRACKS, MIN_FRAME_MS, limit_problems,
};
pub use settings::{
    ChoiceOption, EffectInfo, SettingInfo, SettingRange, SettingSpec, SettingValue, effect_catalog,
};
pub use timing::{
    MAX_TIMING_FILE_BYTES, MIN_MARK_INTERVAL_MS, TidyReport, audacity_labels, check_mark, every_nth_mark,
    fixed_marks, lyric_lines, marks_overlap, parse_audacity_labels, split_words, spread_phrases, tidy_marks,
};
pub use validate::{SequenceIssue, format_ms, validate_sequence};
