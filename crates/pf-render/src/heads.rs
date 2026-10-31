//! Where the Moving Head and Servo effects point each head and servo at a moment.
//!
//! Everything here is worked out from the effect's settings and the moment alone (no state from
//! frame to frame), so a frame scrubbed to and the same frame played agree. The movement's
//! pace is added up from the effect's start: with a curve on the speed, the head speeds up and
//! slows down smoothly instead of jumping. The window's browser stand-in takes the same steps
//! (`app/src/lib/headEffects.ts`).
//!
//! Positions are degrees from Home; the fixture's motors keep them within their limits
//! (`DmxAxis::values`). On a group, the heads (or servos) fan out in member order, spread evenly
//! about the middle of their fan, and each can run a set time behind the one before.

use pf_model::{cos_deg, sin_deg};
use pf_sequence::{HeadPattern, ServoPattern};

/// The fractional part of `x` (0 to 1).
fn frac(x: f64) -> f64 {
    x - x.floor()
}

/// Where a head sits in its fan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FanPlace {
    /// Heads before it in its fan.
    pub index: usize,
    /// Its place from the fan's middle: -1.5, -0.5, 0.5, 1.5 for four.
    pub place: f64,
    /// It is in the fan's right half.
    pub right: bool,
}

/// Where the `index`-th of `count` heads sits in its fan when they are split into `fans` fans
/// side by side (the last may have fewer heads).
pub(crate) fn fan_place(index: usize, count: usize, fans: u32) -> FanPlace {
    let count = count.max(1);
    let size = count.div_ceil((fans.max(1) as usize).min(count));
    let (fan, i) = (index / size, index % size);
    let in_fan = size.min(count.saturating_sub(fan * size)).max(1);
    let middle = (in_fan - 1) as f64 / 2.0;
    FanPlace {
        index: i,
        place: i as f64 - middle,
        right: i as f64 > middle,
    }
}

/// A small, well-mixed hash (the same steps as the window's `hash32`).
fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// The `n`-th of the numbers (0 to 1) that seed `seed` gives head `head`.
fn chance(seed: u32, head: u32, n: u32) -> f64 {
    let key = seed.wrapping_mul(0x9e37_79b9) ^ head.wrapping_mul(16).wrapping_add(n);
    f64::from(hash32(key)) / 4_294_967_296.0
}

/// How far a head is from its centre `cycles` times round its movement, in degrees of pan and
/// tilt: `width` across and `height` up and down, end to end. Wander's paths are picked by
/// `seed`, each head (`head`) its own.
pub(crate) fn head_offset(
    pattern: HeadPattern,
    cycles: f64,
    width: f64,
    height: f64,
    seed: u32,
    head: u32,
) -> (f64, f64) {
    let (w, h) = (width / 2.0, height / 2.0);
    let turn = 360.0 * frac(cycles);
    match pattern {
        HeadPattern::Still => (0.0, 0.0),
        HeadPattern::Sweep => {
            let s = sin_deg(turn);
            (w * s, h * s)
        }
        HeadPattern::Circle => (w * sin_deg(turn), h * cos_deg(turn)),
        HeadPattern::Eight => (w * sin_deg(turn), h * sin_deg(2.0 * turn)),
        HeadPattern::Wave => (w * sin_deg(turn), h * sin_deg(3.0 * turn)),
        HeadPattern::Wander => {
            // Two waves on each axis at chance paces and starting points: never quite repeats.
            let r = |n: u32| chance(seed, head, n);
            let wave = |pace: f64, start: f64| sin_deg(360.0 * frac(pace * cycles + start));
            let pan = 0.65 * wave(0.6 + 0.8 * r(0), r(1)) + 0.35 * wave(1.3 + 1.4 * r(2), r(3));
            let tilt = 0.65 * wave(0.6 + 0.8 * r(4), r(5)) + 0.35 * wave(1.3 + 1.4 * r(6), r(7));
            (w * pan, h * tilt)
        }
    }
}

/// How far a servo is from its position `cycles` times through its movement, in degrees:
/// `size` end to end, `steps` places for Steps.
pub(crate) fn servo_offset(pattern: ServoPattern, cycles: f64, size: f64, steps: u32) -> f64 {
    let half = size / 2.0;
    let f = frac(cycles);
    match pattern {
        ServoPattern::Still => 0.0,
        // Out to one end, back through the middle to the other, and home: a steady pace.
        ServoPattern::Sweep => {
            half * if f < 0.25 {
                4.0 * f
            } else if f < 0.75 {
                2.0 - 4.0 * f
            } else {
                4.0 * f - 4.0
            }
        }
        ServoPattern::Wave => half * sin_deg(360.0 * f),
        ServoPattern::Steps => {
            let n = steps.max(2);
            let k = ((f * f64::from(n)) as u32).min(n - 1);
            -half + size * f64::from(k) / f64::from(n - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    fn at(index: usize, place: f64, right: bool) -> FanPlace {
        FanPlace { index, place, right }
    }

    #[test]
    fn heads_fan_about_the_middle_of_their_fan() {
        let places: Vec<f64> = (0..4).map(|i| fan_place(i, 4, 1).place).collect();
        assert_eq!(places, [-1.5, -0.5, 0.5, 1.5]);
        assert_eq!(
            fan_place(1, 3, 1),
            at(1, 0.0, false),
            "the middle head is the middle"
        );
        assert_eq!(fan_place(2, 3, 1), at(2, 1.0, true));
        // Eight heads in two fans: heads 1 and 5 move alike.
        assert_eq!(fan_place(0, 8, 2), fan_place(4, 8, 2));
        assert_eq!(fan_place(4, 8, 2), at(0, -1.5, false));
        // Five in two fans: three, then two.
        assert_eq!(fan_place(3, 5, 2), at(0, -0.5, false));
        assert_eq!(fan_place(4, 5, 2), at(1, 0.5, true));
        // More fans than heads: each head alone.
        assert_eq!(fan_place(2, 3, 9), at(0, 0.0, false));
        assert_eq!(fan_place(0, 1, 1), at(0, 0.0, false));
    }

    #[test]
    fn patterns_move_about_the_centre() {
        use HeadPattern::*;
        assert!(close(head_offset(Still, 0.3, 90.0, 40.0, 1, 0), (0.0, 0.0)));
        // A quarter of the way round: a sweep at its right end, a circle at its right.
        assert!(close(head_offset(Sweep, 0.25, 90.0, 40.0, 1, 0), (45.0, 20.0)));
        assert!(close(head_offset(Circle, 0.0, 90.0, 40.0, 1, 0), (0.0, 20.0)));
        assert!(close(head_offset(Circle, 0.25, 90.0, 40.0, 1, 0), (45.0, 0.0)));
        assert!(close(head_offset(Circle, 1.25, 90.0, 40.0, 1, 0), (45.0, 0.0)));
        // A figure eight is at its top an eighth of the way round, and its middle at half.
        assert!((head_offset(Eight, 0.125, 90.0, 40.0, 1, 0).1 - 20.0).abs() < 1e-9);
        assert!(close(head_offset(Eight, 0.5, 90.0, 40.0, 1, 0), (0.0, 0.0)));
        // A wave retraces its line on the way back.
        let (out, back) = (
            head_offset(Wave, 0.1, 90.0, 40.0, 1, 0),
            head_offset(Wave, 0.4, 90.0, 40.0, 1, 0),
        );
        assert!(close(out, back));
        // Wander stays within its size, differs from head to head, and comes back the same.
        for k in 0..200 {
            let (p, t) = head_offset(Wander, f64::from(k) * 0.37, 90.0, 40.0, 7, 2);
            assert!(p.abs() <= 45.0 && t.abs() <= 20.0);
        }
        let a = head_offset(Wander, 3.3, 90.0, 40.0, 7, 0);
        assert!(!close(a, head_offset(Wander, 3.3, 90.0, 40.0, 7, 1)));
        assert!(!close(a, head_offset(Wander, 3.3, 90.0, 40.0, 8, 0)));
        assert!(close(a, head_offset(Wander, 3.3, 90.0, 40.0, 7, 0)));
    }

    #[test]
    fn servos_sweep_wave_and_step() {
        use ServoPattern::*;
        assert_eq!(servo_offset(Still, 0.3, 90.0, 4), 0.0);
        let sweep: Vec<f64> = [0.0, 0.125, 0.25, 0.5, 0.75, 1.0]
            .iter()
            .map(|&c| servo_offset(Sweep, c, 90.0, 4))
            .collect();
        assert_eq!(sweep, [0.0, 22.5, 45.0, 0.0, -45.0, 0.0]);
        assert!((servo_offset(Wave, 0.25, 90.0, 4) - 45.0).abs() < 1e-9);
        let steps: Vec<f64> = [0.0, 0.3, 0.6, 0.9]
            .iter()
            .map(|&c| servo_offset(Steps, c, 90.0, 4))
            .collect();
        assert_eq!(steps, [-45.0, -15.0, 15.0, 45.0]);
    }
}
