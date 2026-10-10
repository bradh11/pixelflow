//! The dance: poses of a 2D rig keyed in beats, the moves made of them, and the mix that strings
//! moves together bar by bar.
//!
//! A pose says where the rig's parts are, in figure heights and degrees, as the dancer faces us
//! (left and right are ours). A move is a few poses a beat or half a beat apart that loop; the
//! dancer holds each for a moment, then eases to the next so it lands on the beat. Every move
//! loops in a whole number of bars' beats (1, 2, or 4), so a new move always starts on a bar.

use crate::effects::{Rng, hash};
use pf_sequence::DancerMove;

/// Beats in a bar (4/4, as the song's analysis assumes).
pub(crate) const BAR: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pose {
    /// The hips, from where they rest: right and up.
    pub root: [f32; 2],
    /// How far the spine leans to the right, in degrees.
    pub lean: f32,
    /// The head, from where the neck puts it: right and up.
    pub head: [f32; 2],
    /// How far the head tilts to the right, in degrees.
    pub tilt: f32,
    /// The left and right arms: the upper arm's angle out from hanging straight down, and how
    /// much further the forearm bends (out and up; negative folds it back in).
    pub arms: [[f32; 2]; 2],
    /// The left and right feet, from where they stand: out from the body and up.
    pub feet: [[f32; 2]; 2],
}

/// Standing, arms hanging a little out.
pub(crate) const REST: Pose = Pose {
    root: [0.0, 0.0],
    lean: 0.0,
    head: [0.0, 0.0],
    tilt: 0.0,
    arms: [[14.0, 8.0], [14.0, 8.0]],
    feet: [[0.0, 0.0], [0.0, 0.0]],
};

impl Pose {
    /// The pose `x` of the way to `to`.
    pub fn toward(self, to: Pose, x: f32) -> Pose {
        let mix = |a: f32, b: f32| a + (b - a) * x;
        let pair = |a: [f32; 2], b: [f32; 2]| [mix(a[0], b[0]), mix(a[1], b[1])];
        Pose {
            root: pair(self.root, to.root),
            lean: mix(self.lean, to.lean),
            head: pair(self.head, to.head),
            tilt: mix(self.tilt, to.tilt),
            arms: [pair(self.arms[0], to.arms[0]), pair(self.arms[1], to.arms[1])],
            feet: [pair(self.feet[0], to.feet[0]), pair(self.feet[1], to.feet[1])],
        }
    }

    /// The same pose to the other side.
    const fn other_way(self) -> Pose {
        Pose {
            root: [-self.root[0], self.root[1]],
            lean: -self.lean,
            head: [-self.head[0], self.head[1]],
            tilt: -self.tilt,
            arms: [self.arms[1], self.arms[0]],
            feet: [self.feet[1], self.feet[0]],
        }
    }
}

/// A move: poses at their beats, looping every `beats`.
pub(crate) struct Move {
    beats: f32,
    keys: &'static [(f32, Pose)],
}

// Arms, as [upper arm out from hanging, forearm bend]. None is quite level or quite straight
// up: on a narrow prop an arm slides along the edge the way it leans.
const LOOSE: [f32; 2] = [30.0, -35.0];
const HIP: [f32; 2] = [52.0, -112.0];
const UP: [f32; 2] = [150.0, 25.0];
const WIDE: [f32; 2] = [78.0, 8.0];
/// Elbow out, forearm straight up.
const GOAL: [f32; 2] = [100.0, 78.0];
/// Elbow up and out, the forearm hanging from it.
const PUPPET: [f32; 2] = [118.0, -112.0];
const SWING_IN: [f32; 2] = [40.0, -95.0];
const SWING_OUT: [f32; 2] = [50.0, 35.0];

/// Knees bending on every beat, both arms thrown up with it.
const BOUNCE_DOWN: Pose = Pose {
    root: [-0.02, -0.055],
    lean: -3.0,
    arms: [GOAL, GOAL],
    ..REST
};
const BOUNCE_UP: Pose = Pose {
    root: [0.0, 0.012],
    arms: [LOOSE, LOOSE],
    ..REST
};
const BOUNCE: Move = Move {
    beats: 2.0,
    keys: &[
        (0.0, BOUNCE_DOWN),
        (0.5, BOUNCE_UP),
        (1.0, BOUNCE_DOWN.other_way()),
        (1.5, BOUNCE_UP),
    ],
};

/// One arm up, then the other, both up, both out.
const WAVE_LEFT: Pose = Pose {
    root: [0.025, -0.02],
    lean: 5.0,
    head: [0.02, 0.0],
    arms: [UP, WIDE],
    ..REST
};
const WAVE_BOTH: Pose = Pose {
    root: [0.0, -0.045],
    arms: [UP, UP],
    ..REST
};
const WAVE_OUT: Pose = Pose {
    root: [0.0, 0.01],
    arms: [WIDE, WIDE],
    ..REST
};
const ARM_WAVE: Move = Move {
    beats: 4.0,
    keys: &[
        (0.0, WAVE_LEFT),
        (1.0, WAVE_LEFT.other_way()),
        (2.0, WAVE_BOTH),
        (3.0, WAVE_OUT),
    ],
};

/// A kick out to one side with that arm up, feet together, then the other side.
const KICK_LEFT: Pose = Pose {
    root: [0.03, 0.0],
    lean: 6.0,
    head: [0.02, 0.0],
    arms: [GOAL, HIP],
    feet: [[0.17, 0.14], [0.0, 0.0]],
    ..REST
};
const KICK_BETWEEN: Pose = Pose {
    root: [0.0, -0.045],
    arms: [LOOSE, LOOSE],
    ..REST
};
const KICK: Move = Move {
    beats: 4.0,
    keys: &[
        (0.0, KICK_LEFT),
        (1.0, KICK_BETWEEN),
        (2.0, KICK_LEFT.other_way()),
        (3.0, KICK_BETWEEN),
    ],
};

/// Hips one way and shoulders the other, low, arms swinging across.
const TWIST_LEFT: Pose = Pose {
    root: [-0.04, -0.07],
    lean: 10.0,
    head: [-0.025, 0.0],
    tilt: -6.0,
    arms: [SWING_OUT, SWING_IN],
    feet: [[0.03, 0.0], [0.03, 0.0]],
};
const TWIST: Move = Move {
    beats: 2.0,
    keys: &[(0.0, TWIST_LEFT), (1.0, TWIST_LEFT.other_way())],
};

/// A step to the side, the other foot tapping in, then back.
const STEP_LEFT: Pose = Pose {
    root: [-0.04, -0.035],
    lean: -3.0,
    arms: [PUPPET, HIP],
    feet: [[0.09, 0.0], [0.0, 0.0]],
    ..REST
};
const TAP_LEFT: Pose = Pose {
    root: [-0.055, 0.0],
    arms: [HIP, PUPPET],
    feet: [[0.06, 0.0], [-0.06, 0.04]],
    ..REST
};
const SHUFFLE: Move = Move {
    beats: 4.0,
    keys: &[
        (0.0, STEP_LEFT),
        (1.0, TAP_LEFT),
        (2.0, STEP_LEFT.other_way()),
        (3.0, TAP_LEFT.other_way()),
    ],
};

/// A star jump on the beat, a crouch between; every other jump keeps the feet together.
const STAR: Pose = Pose {
    root: [0.0, 0.075],
    arms: [UP, UP],
    feet: [[0.09, 0.07], [0.09, 0.07]],
    ..REST
};
const PENCIL: Pose = Pose {
    root: [0.0, 0.075],
    arms: [PUPPET, PUPPET],
    feet: [[0.0, 0.07], [0.0, 0.07]],
    ..REST
};
const CROUCH: Pose = Pose {
    root: [0.0, -0.075],
    arms: [LOOSE, LOOSE],
    ..REST
};
const JUMP: Move = Move {
    beats: 2.0,
    keys: &[(0.0, STAR), (0.5, CROUCH), (1.0, PENCIL), (1.5, CROUCH)],
};

/// Hands on hips, the head rocking side to side.
const BOB_LEFT: Pose = Pose {
    root: [0.01, -0.03],
    lean: -2.0,
    head: [-0.035, 0.0],
    tilt: -18.0,
    arms: [HIP, HIP],
    ..REST
};
const BOB_UP: Pose = Pose {
    root: [0.0, 0.01],
    arms: [HIP, HIP],
    ..REST
};
const HEAD_BOB: Move = Move {
    beats: 2.0,
    keys: &[
        (0.0, BOB_LEFT),
        (0.5, BOB_UP),
        (1.0, BOB_LEFT.other_way()),
        (1.5, BOB_UP),
    ],
};

/// The moves the mix draws from.
const MIX: [DancerMove; 7] = [
    DancerMove::Bounce,
    DancerMove::ArmWave,
    DancerMove::Kick,
    DancerMove::Twist,
    DancerMove::Shuffle,
    DancerMove::Jump,
    DancerMove::HeadBob,
];

/// The share of the time to the next pose that a pose is held before easing on.
const HOLD: f32 = 0.3;

impl Move {
    /// The key playing `t` beats into the move, and its place in the keys.
    fn key_at(&self, t: f32) -> usize {
        self.keys.partition_point(|(at, _)| *at <= t).saturating_sub(1)
    }
}

fn step(dance: DancerMove) -> &'static Move {
    match dance {
        DancerMove::Bounce | DancerMove::Mix => &BOUNCE,
        DancerMove::ArmWave => &ARM_WAVE,
        DancerMove::Kick => &KICK,
        DancerMove::Twist => &TWIST,
        DancerMove::Shuffle => &SHUFFLE,
        DancerMove::Jump => &JUMP,
        DancerMove::HeadBob => &HEAD_BOB,
    }
}

/// The mix's moves for the phrase of bars `phrase`, each once, in the order `routine` deals.
fn dealt(routine: u32, phrase: i64) -> [DancerMove; 7] {
    let mut order = MIX;
    let mut rng = Rng::new(hash(u64::from(routine), phrase as u64, 0xDA9C));
    for i in (1..order.len()).rev() {
        order.swap(i, rng.int(0, i as i32) as usize);
    }
    order
}

/// The move danced in bar `bar`: the one chosen, or the mix's for that bar (never the same
/// move two bars running).
pub(crate) fn move_in_bar(dance: DancerMove, routine: u32, bar: i64) -> DancerMove {
    if dance != DancerMove::Mix {
        return dance;
    }
    let n = MIX.len() as i64;
    let phrase = bar.div_euclid(n);
    let mut order = dealt(routine, phrase);
    if order[0] == dealt(routine, phrase - 1)[MIX.len() - 1] {
        order.swap(0, 3);
    }
    order[bar.rem_euclid(n) as usize]
}

/// Smooth start and stop.
fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * x * (x * (6.0 * x - 15.0) + 10.0)
}

/// The pose at `beat` (beats, whole numbers on the beat, bars every four).
pub(crate) fn pose_at(dance: DancerMove, routine: u32, beat: f64) -> Pose {
    if !beat.is_finite() {
        return REST;
    }
    let bar = (beat / BAR).floor();
    let in_bar = (beat - bar * BAR) as f32;
    let bar = bar as i64;
    let now = step(move_in_bar(dance, routine, bar));
    let loops = (in_bar / now.beats).floor();
    let t = in_bar - loops * now.beats;
    let k = now.key_at(t);
    let (from_at, from) = now.keys[k];
    let (to_at, to) = match now.keys.get(k + 1) {
        Some(next) => *next,
        // Back to the move's first pose, or on to the next bar's move.
        None if (loops + 1.0) * now.beats >= BAR as f32 => {
            let next = step(move_in_bar(dance, routine, bar + 1));
            (now.beats, next.keys[0].1)
        }
        None => (now.beats, now.keys[0].1),
    };
    let x = (t - from_at) / (to_at - from_at).max(1e-6);
    from.toward(to, ease((x - HOLD) / (1.0 - HOLD)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [DancerMove; 8] = [
        DancerMove::Mix,
        DancerMove::Bounce,
        DancerMove::ArmWave,
        DancerMove::Kick,
        DancerMove::Twist,
        DancerMove::Shuffle,
        DancerMove::Jump,
        DancerMove::HeadBob,
    ];

    #[test]
    fn moves_loop_in_whole_bars_with_a_pose_on_the_first_beat() {
        for dance in MIX {
            let m = step(dance);
            assert!([1.0, 2.0, 4.0].contains(&m.beats), "{dance:?}");
            assert_eq!(m.keys[0].0, 0.0, "{dance:?}");
            assert!(
                m.keys.windows(2).all(|w| w[0].0 < w[1].0 && w[1].0 < m.beats),
                "{dance:?}"
            );
        }
    }

    #[test]
    fn a_pose_is_held_on_its_beat_then_eases_to_the_next() {
        for dance in EVERY {
            for beat in 0..16 {
                let on = pose_at(dance, 3, f64::from(beat));
                let held = pose_at(dance, 3, f64::from(beat) + 0.1);
                assert_eq!(on, held, "{dance:?} holds beat {beat}");
                // No jump into the next beat, a bar's first included.
                let before = pose_at(dance, 3, f64::from(beat) + 0.999);
                let next = pose_at(dance, 3, f64::from(beat) + 1.0);
                assert!(
                    (before.root[1] - next.root[1]).abs() < 0.002,
                    "{dance:?} beat {beat}"
                );
                assert!(
                    (before.arms[0][0] - next.arms[0][0]).abs() < 1.0,
                    "{dance:?} beat {beat}"
                );
            }
            // Every move moves: some beat's pose differs from the one before.
            assert!(
                (0..8).any(|b| pose_at(dance, 3, f64::from(b)) != pose_at(dance, 3, f64::from(b) + 1.0)),
                "{dance:?}"
            );
        }
    }

    #[test]
    fn the_mix_changes_move_every_bar_the_same_way_every_time() {
        for routine in [1, 2, 7, 99] {
            let bars: Vec<DancerMove> = (-20..200)
                .map(|bar| move_in_bar(DancerMove::Mix, routine, bar))
                .collect();
            assert!(bars.windows(2).all(|w| w[0] != w[1]), "routine {routine}");
            assert!(bars.iter().all(|m| *m != DancerMove::Mix));
            // Every move turns up within a phrase.
            for m in MIX {
                assert!(bars[20..27].contains(&m), "routine {routine}: {m:?}");
            }
            let again: Vec<DancerMove> = (-20..200)
                .map(|bar| move_in_bar(DancerMove::Mix, routine, bar))
                .collect();
            assert_eq!(bars, again);
        }
        let of = |routine| -> Vec<DancerMove> {
            (0..28)
                .map(|bar| move_in_bar(DancerMove::Mix, routine, bar))
                .collect()
        };
        assert_ne!(of(1), of(2), "another routine, another order");
        assert_eq!(move_in_bar(DancerMove::Kick, 1, 5), DancerMove::Kick);
    }

    #[test]
    fn odd_times_still_give_a_pose() {
        for beat in [f64::NAN, f64::INFINITY, -1e12, 1e15, -0.25] {
            let pose = pose_at(DancerMove::Mix, 1, beat);
            assert!(pose.root[1].is_finite() && pose.arms[1][1].is_finite(), "{beat}");
        }
    }
}
