//! A fixture's shutter: what its strobe channel's values do, so the previews show the light as
//! the fixture would. A shutter that is closed shows nothing; a strobe flashes at the rate its
//! value sets.
//!
//! A strobe channel lists what its values do as ranges, each from a value up to where the next
//! starts (or 255): closed, open, or a strobe from slow to fast across its values. Values below
//! the first range are open. A channel that lists none does what most fixtures' strobe channels
//! do ([`USUAL_SHUTTER`]): open from 0 to 9, a strobe from 10 (slowest) to 255 (fastest).
//!
//! **A strobe's flashes.** A strobe of `hz` flashes a second fires its `n`-th flash at `n / hz`
//! seconds from the start of the sequence. A frame shown at `t` for `frame_ms` shows a flash
//! when one fires within it, `t` to `t + frame_ms`; otherwise it is dark. A strobe faster than
//! half the frame rate is shown at half the frame rate (a flash every other frame), the fastest
//! frames can show. So what a frame shows depends on its moment alone: scrubbing to it and
//! playing through it show the same. The window works it out with the same steps
//! (`app/src/lib/dmx.ts`).

use serde::{Deserialize, Serialize};

/// What a range of a strobe channel's values does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum DmxShutter {
    /// The light is shut out.
    Closed,
    /// The light shows.
    Open,
    /// The light flashes, slowest at the range's first value and fastest at its last.
    Strobe,
}

/// What a strobe channel's values from `from` do, up to where the next range starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DmxShutterRange {
    pub from: u8,
    pub does: DmxShutter,
}

/// What a strobe channel that lists nothing does: open from 0 to 9, then a strobe from slow to
/// fast, as most fixtures' strobe channels are.
pub const USUAL_SHUTTER: &[DmxShutterRange] = &[
    DmxShutterRange {
        from: 0,
        does: DmxShutter::Open,
    },
    DmxShutterRange {
        from: 10,
        does: DmxShutter::Strobe,
    },
];

/// The most ranges one strobe channel may list.
pub const MAX_SHUTTER_RANGES: usize = 32;
/// Flashes a second at the first value of a strobe range, and at its last.
pub const STROBE_SLOWEST_HZ: f64 = 1.0;
pub const STROBE_FASTEST_HZ: f64 = 20.0;

/// What a shutter does at one value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShutterState {
    Closed,
    Open,
    /// Flashing this many times a second.
    Strobe(f64),
}

/// What a shutter whose values do as `ranges` say does at `value`.
pub fn shutter_at(ranges: &[DmxShutterRange], value: u8) -> ShutterState {
    let Some(i) = ranges.iter().rposition(|r| r.from <= value) else {
        return ShutterState::Open;
    };
    let range = ranges[i];
    match range.does {
        DmxShutter::Closed => ShutterState::Closed,
        DmxShutter::Open => ShutterState::Open,
        DmxShutter::Strobe => {
            let last = ranges
                .get(i + 1)
                .map_or(255, |next| next.from.saturating_sub(1).max(range.from));
            let span = f64::from(last - range.from).max(1.0);
            let share = f64::from(value - range.from) / span;
            ShutterState::Strobe(STROBE_SLOWEST_HZ + (STROBE_FASTEST_HZ - STROBE_SLOWEST_HZ) * share)
        }
    }
}

/// When a frame is shown: its time in the sequence and how long it is shown, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    pub t_ms: u64,
    pub frame_ms: u32,
}

impl Moment {
    pub fn new(t_ms: u64, frame_ms: u32) -> Self {
        Self { t_ms, frame_ms }
    }
}

/// Whether a strobe of `hz` flashes a second shows a flash in the frame shown at `at`.
pub fn flashes(hz: f64, at: Moment) -> bool {
    let frame = f64::from(at.frame_ms.max(1));
    let rate = hz.min(500.0 / frame);
    if rate.is_nan() || rate <= 0.0 {
        return false;
    }
    let t = at.t_ms as f64;
    let first = (t * rate / 1000.0).ceil();
    first < (t + frame) * rate / 1000.0
}

/// Whether the light shows at `at`, for a shutter whose values do as `ranges` say, at `value`.
pub fn shutter_shows(ranges: &[DmxShutterRange], value: u8, at: Moment) -> bool {
    match shutter_at(ranges, value) {
        ShutterState::Closed => false,
        ShutterState::Open => true,
        ShutterState::Strobe(hz) => flashes(hz, at),
    }
}

/// The value that sets a shutter whose values do as `ranges` say flashing `hz` times a second:
/// within its first strobe range, as near that pace as its values go (1 to 20 a second). `None`
/// when it has no strobe range.
pub fn strobe_value(ranges: &[DmxShutterRange], hz: f64) -> Option<u8> {
    let i = ranges.iter().position(|r| r.does == DmxShutter::Strobe)?;
    let from = ranges[i].from;
    let last = ranges
        .get(i + 1)
        .map_or(255, |next| next.from.saturating_sub(1).max(from));
    let share = (hz - STROBE_SLOWEST_HZ) / (STROBE_FASTEST_HZ - STROBE_SLOWEST_HZ);
    let share = if share.is_nan() {
        0.0
    } else {
        share.clamp(0.0, 1.0)
    };
    Some(from + (share * f64::from(last - from)).round() as u8)
}

/// A value that opens a shutter whose values do as `ranges` say: `held` when it does, else the
/// lowest that does. `None` when no value opens it.
pub fn open_value(ranges: &[DmxShutterRange], held: u8) -> Option<u8> {
    let open = |v: u8| shutter_at(ranges, v) == ShutterState::Open;
    if open(held) {
        return Some(held);
    }
    (0..=255).find(|&v| open(v))
}

/// Why these ranges can't be used, in plain words.
pub fn shutter_problem(ranges: &[DmxShutterRange]) -> Option<String> {
    if ranges.len() > MAX_SHUTTER_RANGES {
        return Some(format!(
            "A strobe channel lists at most {MAX_SHUTTER_RANGES} ranges of values; this one lists {}.",
            ranges.len()
        ));
    }
    if ranges.windows(2).any(|w| w[0].from >= w[1].from) {
        return Some(
            "A strobe channel's ranges of values go from the lowest up, each starting after the one before."
                .into(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(from: u8, does: DmxShutter) -> DmxShutterRange {
        DmxShutterRange { from, does }
    }

    #[test]
    fn the_usual_shutter_is_open_low_then_strobes_from_slow_to_fast() {
        assert_eq!(shutter_at(USUAL_SHUTTER, 0), ShutterState::Open);
        assert_eq!(shutter_at(USUAL_SHUTTER, 9), ShutterState::Open);
        assert_eq!(shutter_at(USUAL_SHUTTER, 10), ShutterState::Strobe(1.0));
        assert_eq!(shutter_at(USUAL_SHUTTER, 255), ShutterState::Strobe(20.0));
        let ShutterState::Strobe(mid) = shutter_at(USUAL_SHUTTER, 132) else {
            panic!("a strobe");
        };
        assert!((mid - 10.46).abs() < 0.01, "{mid}");
    }

    #[test]
    fn a_fixture_s_own_ranges_say_where_it_is_closed_open_and_strobing() {
        // A head's shutter: closed, open, a strobe, then open again at the top.
        let head = [
            range(0, DmxShutter::Closed),
            range(8, DmxShutter::Open),
            range(16, DmxShutter::Strobe),
            range(132, DmxShutter::Open),
        ];
        assert_eq!(shutter_at(&head, 3), ShutterState::Closed);
        assert_eq!(shutter_at(&head, 8), ShutterState::Open);
        assert_eq!(shutter_at(&head, 16), ShutterState::Strobe(1.0));
        assert_eq!(shutter_at(&head, 131), ShutterState::Strobe(20.0));
        assert_eq!(shutter_at(&head, 200), ShutterState::Open);
        // Below the first range, open.
        assert_eq!(
            shutter_at(&[range(20, DmxShutter::Closed)], 5),
            ShutterState::Open
        );
        assert_eq!(shutter_at(&[], 77), ShutterState::Open);
        // A one-value strobe range strobes slowest.
        assert_eq!(
            shutter_at(&[range(0, DmxShutter::Strobe), range(1, DmxShutter::Open)], 0),
            ShutterState::Strobe(1.0)
        );
    }

    #[test]
    fn a_strobe_flashes_at_its_rate_from_the_moment_alone() {
        let at = |t_ms| Moment::new(t_ms, 25);
        // Once a second: one 25 ms frame in 40 shows the flash.
        let lit: Vec<u64> = (0..120)
            .map(|n| n * 25)
            .filter(|&t| flashes(1.0, at(t)))
            .collect();
        assert_eq!(lit, [0, 1000, 2000]);
        // Ten a second at 40 frames a second: every fourth frame.
        let count = (0..40).filter(|&n| flashes(10.0, at(n * 25))).count();
        assert_eq!(count, 10);
        // Twenty, and faster: every other frame, the fastest 40 frames a second show.
        for hz in [20.0, 35.0] {
            let pattern: Vec<bool> = (0..6).map(|n| flashes(hz, at(n * 25))).collect();
            assert_eq!(pattern, [true, false, true, false, true, false], "{hz}");
        }
        // The same moment always shows the same.
        assert_eq!(flashes(7.3, at(4975)), flashes(7.3, at(4975)));
        assert!(!flashes(0.0, at(0)));
    }

    #[test]
    fn a_shutter_shows_the_light_when_open_or_flashing() {
        let at = Moment::new(25, 25);
        let closed = [range(0, DmxShutter::Closed), range(128, DmxShutter::Open)];
        assert!(!shutter_shows(&closed, 0, at));
        assert!(shutter_shows(&closed, 200, at));
        assert!(!shutter_shows(USUAL_SHUTTER, 255, at), "between flashes");
        assert!(shutter_shows(USUAL_SHUTTER, 255, Moment::new(50, 25)));
    }

    #[test]
    fn a_pace_and_an_open_shutter_become_values() {
        // The usual shutter: 1 a second at 10, 20 at 255, open below 10.
        assert_eq!(strobe_value(USUAL_SHUTTER, 1.0), Some(10));
        assert_eq!(strobe_value(USUAL_SHUTTER, 20.0), Some(255));
        assert_eq!(strobe_value(USUAL_SHUTTER, 50.0), Some(255));
        assert_eq!(strobe_value(USUAL_SHUTTER, 0.2), Some(10));
        for hz in [2.0, 7.5, 13.0] {
            let v = strobe_value(USUAL_SHUTTER, hz).unwrap();
            let ShutterState::Strobe(back) = shutter_at(USUAL_SHUTTER, v) else {
                panic!("a strobe at {v}");
            };
            assert!((back - hz).abs() < 0.04, "{hz}: {v} flashes {back}");
        }
        assert_eq!(open_value(USUAL_SHUTTER, 0), Some(0));
        assert_eq!(
            open_value(USUAL_SHUTTER, 5),
            Some(5),
            "the value held, when it opens"
        );
        assert_eq!(open_value(USUAL_SHUTTER, 200), Some(0));
        // A head's own: closed low, open from 8, a strobe from 16 to 131.
        let head = [
            range(0, DmxShutter::Closed),
            range(8, DmxShutter::Open),
            range(16, DmxShutter::Strobe),
            range(132, DmxShutter::Open),
        ];
        assert_eq!(open_value(&head, 0), Some(8));
        assert_eq!(strobe_value(&head, 20.0), Some(131));
        assert_eq!(strobe_value(&[range(0, DmxShutter::Open)], 5.0), None);
        assert_eq!(open_value(&[range(0, DmxShutter::Closed)], 3), None);
    }

    #[test]
    fn ranges_out_of_order_are_a_problem() {
        assert_eq!(shutter_problem(USUAL_SHUTTER), None);
        assert!(shutter_problem(&[range(10, DmxShutter::Open), range(10, DmxShutter::Closed)]).is_some());
        assert!(shutter_problem(&[range(0, DmxShutter::Open); 33]).is_some());
    }

    #[test]
    fn ranges_save_as_their_first_value_and_what_they_do() {
        let json = serde_json::to_value(USUAL_SHUTTER).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{ "from": 0, "does": "open" }, { "from": 10, "does": "strobe" }])
        );
    }
}
