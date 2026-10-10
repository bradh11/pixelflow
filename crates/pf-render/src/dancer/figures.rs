//! The dancers: each character built on the rig and painted, shedding detail as it gets smaller.
//!
//! A character is designed in figure heights (1 = the top of its head or hat) and drawn in
//! cells. A small figure is drawn on the cells, like pixel art: its joints sit at cell middles
//! and its sizes are whole cells, so a bone is one clean cell wide and an eye is a whole cell.
//! A large one is drawn where the numbers fall, softened at the edges.
//!
//! Where the prop is too narrow for a pose, it folds in rather than being cut off: the body
//! sways only as far as its shoulders have room, an arm pushed in from the edge slides up or
//! down it at its full length, and a leg is drawn shorter, as if it reached toward us.
//!
//! The humanoids (skeleton, witch, Santa, elf) share one rig: hips, spine, shoulders, elbows,
//! hands, a head, and legs whose knees bend to keep the feet where the pose puts them. The ghost
//! and the snowman have no legs: the ghost floats on a wavy hem and the snowman is three stacked
//! balls, each taking the same pose as sway, bounce, and arms.

use super::moves::Pose;
use super::paint::{BLACK, Color, P, Paint};
use crate::color::Colors;
use pf_sequence::DancerCharacter;

/// A figure shorter than this many cells is drawn on the cells.
const CRISP_BELOW: f32 = 72.0;
/// How hard a small figure's edges are (see [`Paint::sharp`]).
const CRISP_EDGES: f32 = 1.7;
/// How much of a knee's bend shows sideways; the rest is toward us, shortening the leg.
const KNEE_OUT: f32 = 0.4;
/// How far around an arm the body behind it is dimmed, in cells, so the arm stands apart.
const APART: f32 = 0.7;

const WHITE: Color = [1.0, 1.0, 1.0];
const RED: Color = [1.0, 0.06, 0.04];
const GREEN: Color = [0.1, 0.9, 0.12];
const PURPLE: Color = [0.6, 0.14, 1.0];
const ORANGE: Color = [1.0, 0.42, 0.0];
const GOLD: Color = [1.0, 0.78, 0.08];
const SKIN: Color = [1.0, 0.7, 0.5];
const WITCH_SKIN: Color = [0.3, 1.0, 0.15];
const SHEET: Color = [0.88, 0.94, 1.0];
/// "Black" that still shows on lights: a hat or boots against the dark.
const COAL: Color = [0.24, 0.24, 0.34];
const TWIG: Color = [0.8, 0.45, 0.12];

/// The left and right sides, as signs.
const SIDES: [f32; 2] = [-1.0, 1.0];

/// How many times taller than wide a character may be drawn: on a narrower prop it's smaller.
pub(crate) fn tallest(character: DancerCharacter) -> f32 {
    match character {
        DancerCharacter::Skeleton | DancerCharacter::Witch | DancerCharacter::Elf => 4.2,
        DancerCharacter::Santa => 3.6,
        DancerCharacter::Snowman => 2.9,
        DancerCharacter::Ghost => 2.7,
    }
}

/// A figure's height in cells: whole cells for a small one, a cell at least.
pub(crate) fn fitted(size: f32) -> f32 {
    if size < CRISP_BELOW {
        size.floor().max(1.0)
    } else {
        size
    }
}

/// How hard the edges of a figure `size` cells tall are drawn.
pub(crate) fn edges(size: f32) -> f32 {
    if size < CRISP_BELOW { CRISP_EDGES } else { 1.0 }
}

/// The color of the glow behind a character.
pub(crate) fn glow(character: DancerCharacter) -> Color {
    match character {
        DancerCharacter::Skeleton => [0.5, 0.0, 1.0],
        DancerCharacter::Ghost => [0.0, 0.9, 0.3],
        DancerCharacter::Witch => [1.0, 0.35, 0.0],
        DancerCharacter::Santa => [0.0, 0.8, 0.1],
        DancerCharacter::Snowman => [0.0, 0.25, 1.0],
        DancerCharacter::Elf => [1.0, 0.0, 0.0],
    }
}

/// A character's own colors, or the effect's palette in their place.
#[derive(Clone, Copy)]
pub(crate) struct Tones(pub Option<Colors>);

impl Tones {
    /// The color for a part: its own, or the palette's color `part` (the palette again, paler
    /// then darker, for parts past its end, so a one-color character keeps its markings).
    fn of(&self, part: usize, own: Color) -> Color {
        let Some(palette) = &self.0 else {
            return own;
        };
        let c = palette.get(part as u64);
        match part / palette.len() {
            0 => c,
            n if n % 2 == 1 => c.map(|v| v + (1.0 - v) * 0.65),
            _ => c.map(|v| v * 0.4),
        }
    }
}

/// One dancer being drawn.
pub(crate) struct Figure<'r> {
    pub paint: Paint<'r>,
    /// Its height in cells.
    pub size: f32,
    /// How far from its center line it may reach, in cells.
    pub reach: f32,
    /// The cells above its head when it stands at rest.
    pub headroom: f32,
    pub pose: Pose,
    /// The beat it dances at, for what moves on its own (a ghost's hem).
    pub beat: f64,
    pub tones: Tones,
}

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

/// `x` pulled inside ±`limit`: unchanged near the middle, squeezed toward the limit past it.
fn fold(x: f32, limit: f32) -> f32 {
    if limit <= 0.0 {
        return 0.0;
    }
    let knee = 0.6 * limit;
    let a = x.abs();
    if a <= knee {
        return x;
    }
    let room = limit - knee;
    (knee + room * (1.0 - (-(a - knee) / room).exp())).copysign(x)
}

/// The far end of a limb `length` long from `from` toward `to`, kept inside ±`reach`: pushed in
/// from the edge, it slides up or down the edge, the way it was heading (`rise`), so the limb
/// keeps its length instead of shrinking to a stub.
fn fold_limb(from: P, to: P, rise: f32, length: f32, reach: f32) -> P {
    let x = fold(to[0], reach);
    if (x - to[0]).abs() < 0.5 {
        return [x, to[1]];
    }
    let dx = x - from[0];
    let up = (length * length - dx * dx).max(0.0).sqrt();
    [x, from[1] + up.copysign(rise)]
}

impl Figure<'_> {
    fn crisp(&self) -> bool {
        self.size < CRISP_BELOW
    }

    /// `x` figure heights in cells: whole cells on a small figure, `least` at least.
    fn cells(&self, x: f32, least: f32) -> f32 {
        let v = x * self.size;
        if self.crisp() {
            v.round().max(least)
        } else {
            v.max(least)
        }
    }

    /// `x` figure heights as a half width, at least half a cell: on a small figure, one that
    /// makes a whole odd number of cells around a cell's middle.
    fn half(&self, x: f32) -> f32 {
        let v = x * self.size;
        if self.crisp() {
            ((v - 0.5).round() + 0.5).max(0.5)
        } else {
            v.max(0.5)
        }
    }

    /// A point, moved to the middle of its cell on a small figure.
    fn at(&self, p: P) -> P {
        if self.crisp() {
            [p[0].round(), p[1].floor() + 0.5]
        } else {
            p
        }
    }

    /// The bottom edge of the row at height `y`, on a small figure.
    fn row(&self, y: f32) -> f32 {
        if self.crisp() { y.floor() } else { y }
    }

    /// Half a limb's thickness, `x` figure heights: on a small figure, one cell across until
    /// it's tall enough for more.
    fn limb(&self, x: f32) -> f32 {
        let v = x * self.size;
        if self.crisp() {
            if v < 0.75 { 0.5 } else { 1.0 }
        } else {
            v
        }
    }

    fn stroke(&self) -> f32 {
        self.limb(0.0125)
    }

    /// A body's half width: as designed, but leaving a column each side for the arms.
    fn girth(&self, x: f32) -> f32 {
        self.half(x).min((self.reach - 0.5).max(0.5))
    }

    /// Two dark eyes on a face at `c`, `r` wide each side.
    fn eyes(&mut self, c: P, r: f32) {
        if r < 2.5 {
            return;
        }
        for side in SIDES {
            if self.crisp() {
                let x = c[0] + side * (0.45 * r).round().max(1.0);
                self.paint
                    .rect([x - 0.5, c[1] - 0.5], [x + 0.5, c[1] + 0.5], BLACK);
            } else {
                self.paint
                    .disc([c[0] + side * 0.42 * r, c[1] + 0.05 * r], 0.16 * r, BLACK);
            }
        }
    }

    /// A mouth under the eyes of a face at `c`, when the face has room for one.
    fn mouth(&mut self, c: P, r: f32, color: Color) {
        if r < 3.5 {
            return;
        }
        if self.crisp() {
            let w = if r >= 4.5 { 1.5 } else { 0.5 };
            self.paint
                .rect([c[0] - w, c[1] - 2.5], [c[0] + w, c[1] - 1.5], color);
        } else {
            self.paint
                .ellipse([c[0], c[1] - 0.5 * r], [0.3 * r, 0.1 * r], color);
        }
    }
}

/// A humanoid's proportions, in figure heights.
struct Build {
    /// How high the hips and the shoulder line are.
    hip: f32,
    chest: f32,
    head_r: f32,
    /// How tall its hat is, in head radii (none: 0). The head sits under it, at the top.
    hat: f32,
    /// The least rows between the bottom of the head and the shoulder line (a beard may overlap
    /// the shoulders: negative).
    neck: f32,
    /// Half the width of the shoulders and the hips.
    shoulder: f32,
    /// The least the shoulders are next to the head, in cells each side: a small figure's
    /// shoulders are as wide as its (big) head, give or take this.
    broad: f32,
    hip_w: f32,
    upper_arm: f32,
    forearm: f32,
    thigh: f32,
    shin: f32,
    /// How far from the center line each foot stands.
    stance: f32,
}

const SKELETON: Build = Build {
    hip: 0.46,
    chest: 0.745,
    head_r: 0.08,
    hat: 0.0,
    neck: 1.0,
    shoulder: 0.09,
    broad: 0.5,
    hip_w: 0.04,
    upper_arm: 0.15,
    forearm: 0.15,
    thigh: 0.235,
    shin: 0.235,
    stance: 0.06,
};

const WITCH: Build = Build {
    hip: 0.4,
    chest: 0.655,
    head_r: 0.075,
    hat: 2.0,
    neck: 1.0,
    shoulder: 0.075,
    broad: -0.5,
    hip_w: 0.04,
    upper_arm: 0.13,
    forearm: 0.13,
    thigh: 0.205,
    shin: 0.205,
    stance: 0.055,
};

const SANTA: Build = Build {
    hip: 0.33,
    chest: 0.6,
    head_r: 0.085,
    hat: 1.6,
    neck: -1.0,
    shoulder: 0.1,
    broad: 0.5,
    hip_w: 0.05,
    upper_arm: 0.125,
    forearm: 0.125,
    thigh: 0.17,
    shin: 0.17,
    stance: 0.07,
};

const ELF: Build = Build {
    hip: 0.42,
    chest: 0.655,
    head_r: 0.075,
    hat: 2.0,
    neck: 1.0,
    shoulder: 0.075,
    broad: -0.5,
    hip_w: 0.04,
    upper_arm: 0.13,
    forearm: 0.13,
    thigh: 0.215,
    shin: 0.215,
    stance: 0.055,
};

/// A skull's jaw under its round top, in cells, for a head `r` across each side.
fn jaw(r: f32, crisp: bool) -> f32 {
    match crisp {
        true if r >= 3.5 => 2.0,
        true if r >= 2.5 => 1.0,
        true => 0.0,
        false => 0.55 * r,
    }
}

/// A humanoid's joints for a pose, in cells.
struct Body {
    hips: P,
    chest: P,
    head: P,
    head_r: f32,
    /// How far the head tilts to the right, in radians.
    tilt: f32,
    shoulders: [P; 2],
    elbows: [P; 2],
    hands: [P; 2],
    hip_joints: [P; 2],
    knees: [P; 2],
    feet: [P; 2],
    /// The end of each foot.
    toes: [P; 2],
    /// How tall the hat is, in cells.
    hat: f32,
}

/// Where a two-part limb from `from` to `to` bends, its parts `a` and `b` long, to `side`.
fn bend(from: P, to: P, a: f32, b: f32, side: f32) -> P {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let far = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if far > 1e-4 {
        (dx / far, dy / far)
    } else {
        (0.0, -1.0)
    };
    let least = (a - b).abs() + 1e-3;
    let d = far.clamp(least, (a + b - 1e-3).max(least));
    let along = (a * a - b * b + d * d) / (2.0 * d);
    let out = (a * a - along * along).max(0.0).sqrt() * KNEE_OUT;
    // The side of the limb's line that `side` is on.
    let way = if -uy * side >= 0.0 { 1.0 } else { -1.0 };
    [
        from[0] + ux * along - uy * out * way,
        from[1] + uy * along + ux * out * way,
    ]
}

impl Body {
    fn new(build: &Build, fig: &Figure) -> Self {
        let (s, pose, reach) = (fig.size, &fig.pose, fig.reach);
        // The head carries the character, so a small figure gets a big one: seven cells across
        // from 24 cells tall, five from 12, where the prop is wide enough to leave it a column
        // each side. The body below is shortened to make room.
        let designed = fig.half(build.head_r);
        let least: f32 = match s {
            _ if !fig.crisp() || s < 12.0 => 0.5,
            _ if s < 24.0 => 2.5,
            _ => 3.5,
        };
        let head_r = designed.max(least.min(reach - 0.5)).min(reach + 0.5).max(0.5);
        let hat = match fig.crisp() {
            true => (build.hat * head_r).round(),
            false => build.hat * head_r,
        };
        let head_y = s - hat - head_r;
        let chin = head_r
            + build.neck
            + if build.hat == 0.0 {
                jaw(head_r, fig.crisp())
            } else {
                0.0
            };
        let chest_y = (build.chest * s).min(head_y - chin - 0.5).max(0.0);
        let short = if build.chest * s > 0.0 {
            chest_y / (build.chest * s)
        } else {
            1.0
        };
        let across = match fig.crisp() {
            true => (build.shoulder * s).round().max(head_r + build.broad).min(reach),
            false => build.shoulder * s,
        };
        // The body sways until a shoulder meets the edge; the head, until it's a column short
        // of it (a raised arm passes there).
        let sway = (reach - across).max(0.0);
        let nod = (reach - 0.5 - head_r)
            .max(0.0)
            .max(sway.min(reach + 0.5 - head_r));
        let lean = pose.lean.to_radians();
        let tilt = pose.tilt.to_radians();
        let up = |angle: f32, length: f32| [length * angle.sin(), length * angle.cos()];
        let hips = [
            (pose.root[0] * s).clamp(-sway, sway),
            ((build.hip * short + pose.root[1]) * s).max(0.0),
        ];
        let mut chest = add(hips, up(lean, chest_y - build.hip * short * s));
        chest[0] = chest[0].clamp(-sway, sway);
        let mut head = add(
            add(chest, up(lean + 0.5 * tilt, head_y - chest_y)),
            [pose.head[0] * s, pose.head[1] * s],
        );
        head[0] = head[0].clamp(-nod, nod);
        let mut body = Body {
            hips: fig.at(hips),
            chest: fig.at(chest),
            head: fig.at(head),
            head_r,
            tilt,
            shoulders: [[0.0; 2]; 2],
            elbows: [[0.0; 2]; 2],
            hands: [[0.0; 2]; 2],
            hip_joints: [[0.0; 2]; 2],
            knees: [[0.0; 2]; 2],
            feet: [[0.0; 2]; 2],
            toes: [[0.0; 2]; 2],
            hat,
        };
        let limbs = short * s;
        for (i, side) in SIDES.into_iter().enumerate() {
            let shoulder = [
                (chest[0] + side * across * lean.cos()).clamp(-reach, reach),
                chest[1] - side * across * lean.sin(),
            ];
            let upper = pose.arms[i][0].to_radians();
            let fore = upper + pose.arms[i][1].to_radians();
            let (upper_arm, forearm) = (build.upper_arm * limbs, build.forearm * limbs);
            let limb = |from: P, angle: f32, length: f32| {
                [
                    from[0] + side * length * angle.sin(),
                    from[1] - length * angle.cos(),
                ]
            };
            let elbow = limb(shoulder, upper, upper_arm);
            let hand = limb(elbow, fore, forearm);
            let folded = fold_limb(shoulder, elbow, elbow[1] - shoulder[1], upper_arm, reach);
            body.shoulders[i] = fig.at(shoulder);
            body.elbows[i] = fig.at(folded);
            body.hands[i] = fig.at(fold_limb(folded, hand, hand[1] - elbow[1], forearm, reach));
            let hip = [hips[0] + side * build.hip_w * s, hips[1]];
            let foot = [
                side * (build.stance + pose.feet[i][0]) * s,
                (pose.feet[i][1] * s).max(0.0) + fig.stroke(),
            ];
            let knee = bend(hip, foot, build.thigh * limbs, build.shin * limbs, side);
            let place = |p: P| fig.at([fold(p[0], reach), p[1]]);
            body.hip_joints[i] = place(hip);
            body.knees[i] = place(knee);
            body.feet[i] = place(foot);
            let toe = body.feet[i][0] + side * fig.cells(0.03, 1.0);
            body.toes[i] = [toe.clamp(-reach, reach), body.feet[i][1]];
        }
        body
    }

    /// The point `t` of the way up the spine from the hips to the chest.
    fn spine(&self, t: f32) -> P {
        [
            self.hips[0] + (self.chest[0] - self.hips[0]) * t,
            self.hips[1] + (self.chest[1] - self.hips[1]) * t,
        ]
    }
}

/// An arm: a sleeve to the wrist, then the hand. `apart` clears a gap around it first, so it
/// stands apart from a body of its own color.
fn arm(fig: &mut Figure, body: &Body, i: usize, r: f32, sleeve: Color, hand: Color, apart: bool) {
    let (shoulder, elbow, wrist) = (body.shoulders[i], body.elbows[i], body.hands[i]);
    if apart {
        fig.paint.cut(shoulder, elbow, r + APART);
        fig.paint.cut(elbow, wrist, r + APART);
    }
    fig.paint.line(shoulder, elbow, r, sleeve);
    fig.paint.line(elbow, wrist, r, sleeve);
    fig.paint.disc(wrist, r.max(0.5), hand);
}

/// A leg, in level bands of two colors (one color twice for a plain leg), and its foot.
fn leg(fig: &mut Figure, body: &Body, i: usize, r: f32, bands: [Color; 2], shoe: Color) {
    let band = fig.cells(0.045, 2.0);
    let top = body.hip_joints[i][1] + 0.5;
    for (from, to) in [(body.hip_joints[i], body.knees[i]), (body.knees[i], body.feet[i])] {
        fig.paint.line(from, to, r, bands[0]);
        if bands[0] != bands[1] {
            fig.paint.line_where(from, to, r, bands[1], |y| {
                ((top - y) / band).floor().rem_euclid(2.0) == 1.0
            });
        }
    }
    fig.paint.line(body.feet[i], body.toes[i], r, shoe);
}

/// A pointed hat on a head: a cone `width` across each side at the top of the head, up to a tip
/// `lean` cells to the side (and swinging against the head's tilt), in the first color, over a
/// band in the second. Gives the tip.
fn cone_hat(fig: &mut Figure, body: &Body, width: f32, lean: f32, colors: [Color; 2]) -> P {
    let (c, base, height) = (body.head, body.head[1] + body.head_r, body.hat);
    let tip = fig.at([c[0] + lean - body.tilt.sin() * height * 0.5, base + height - 0.5]);
    fig.paint.convex(
        &[
            [c[0] - width, base - 0.5],
            [c[0] + width, base - 0.5],
            [tip[0], tip[1] + 0.5],
        ],
        colors[0],
    );
    if height >= 4.0 {
        let rows = fig.cells(0.022, 1.0);
        fig.paint
            .rect([c[0] - width, base], [c[0] + width, base + rows], colors[1]);
    }
    tip
}

fn skeleton(fig: &mut Figure) {
    let body = Body::new(&SKELETON, fig);
    let bone = fig.tones.of(0, WHITE);
    let r = fig.stroke();
    let crisp = fig.crisp();
    for i in 0..2 {
        leg(fig, &body, i, r, [bone; 2], bone);
    }
    // The pelvis: a bar across the hips under a shorter one.
    let hips = body.hip_joints;
    fig.paint.line(hips[0], hips[1], r, bone);
    if hips[1][0] - hips[0][0] >= 4.0 {
        let lift = if crisp { 1.0 } else { 2.0 * r };
        fig.paint.line(
            [hips[0][0] + lift, hips[0][1] + lift],
            [hips[1][0] - lift, hips[1][1] + lift],
            r,
            bone,
        );
    }
    fig.paint.line(body.hips, body.chest, r, bone);
    // Ribs: one every other cell down from the shoulders, the last a little shorter, leaving the
    // spine bare above the hips. Dropped when there's no room for them to read as ribs.
    let torso = body.chest[1] - body.hips[1];
    let rib = match crisp {
        true => (body.shoulders[1][0] - body.shoulders[0][0]) / 2.0 - 1.5,
        false => 0.055 * fig.size,
    };
    let gap = if crisp { 2.0 } else { 0.045 * fig.size };
    let ribs = (((torso - 1.5 * gap) / gap).floor().max(0.0) as usize).min(4);
    if rib >= 1.5 && ribs >= 2 {
        for k in 1..=ribs {
            let y = body.chest[1] - k as f32 * gap;
            let x = fig.at(body.spine((y - body.hips[1]) / torso))[0];
            let w = if k == ribs && ribs > 2 { rib - 1.0 } else { rib } - r;
            fig.paint.line([x - w, y], [x + w, y], r, bone);
        }
    }
    fig.paint.line(body.shoulders[0], body.shoulders[1], r, bone);
    for i in 0..2 {
        arm(fig, &body, i, r, bone, bone, false);
    }
    fig.paint.line(body.chest, body.head, r, bone);
    skull(fig, &body, bone);
}

/// A skull: a round top over a narrower jaw with teeth, eye sockets, and a nose, as many of
/// them as fit. The jaw stays over the neck, so a head shifted to one side looks tilted.
fn skull(fig: &mut Figure, body: &Body, bone: Color) {
    let (c, r) = (body.head, body.head_r);
    let crisp = fig.crisp();
    fig.paint.disc(c, r, bone);
    if r < 2.5 {
        return;
    }
    let jaw = jaw(r, crisp);
    let w = if crisp { r - 1.0 } else { 0.68 * r };
    let x = c[0] + (body.chest[0] - c[0]).clamp(-1.0, 1.0) * if crisp { 1.0 } else { 0.2 * r };
    let bottom = c[1] - r - jaw;
    fig.paint.rect([x - w, bottom], [x + w, c[1] - r + 1.0], bone);
    // Teeth: gaps along the bottom of the jaw.
    if r >= 3.5 {
        let pitch = if crisp { 2.0 } else { 0.5 * r };
        let gap = if crisp { 0.5 } else { 0.07 * r };
        let rows = if crisp { 1.0 } else { 0.5 * jaw };
        let mut at = pitch / 2.0;
        while at < w {
            for side in SIDES {
                let gx = x + side * at;
                fig.paint
                    .rect([gx - gap, bottom], [gx + gap, bottom + rows], BLACK);
            }
            at += pitch;
        }
    }
    if crisp {
        // Sockets either side of a one-cell bridge, and a nose under it.
        let size = (0.55 * r).round().max(1.0);
        let top = c[1] + 0.5;
        for side in SIDES {
            let (near, far) = (c[0] + side * 0.5, c[0] + side * (0.5 + size));
            fig.paint
                .rect([near.min(far), top - size], [near.max(far), top], BLACK);
        }
        if r >= 3.5 {
            fig.paint
                .rect([c[0] - 0.5, top - size - 1.0], [c[0] + 0.5, top - size], BLACK);
        }
    } else {
        for side in SIDES {
            fig.paint.disc([c[0] + side * 0.42 * r, c[1]], 0.27 * r, BLACK);
        }
        let y = c[1] - 0.45 * r;
        fig.paint.convex(
            &[
                [c[0], y + 0.14 * r],
                [c[0] - 0.1 * r, y - 0.1 * r],
                [c[0] + 0.1 * r, y - 0.1 * r],
            ],
            BLACK,
        );
    }
}

fn witch(fig: &mut Figure) {
    let body = Body::new(&WITCH, fig);
    let cloth = fig.tones.of(0, PURPLE);
    let skin = fig.tones.of(1, WITCH_SKIN);
    let trim = fig.tones.of(2, ORANGE);
    let r = fig.stroke();
    let (c, head_r) = (body.head, body.head_r);
    let top = fig.girth(WITCH.shoulder);
    let hem = fig.girth(0.14);
    // A broom standing at her right, when there's room beside her.
    if fig.reach >= hem + 4.0 {
        let x = body.hips[0] + hem + 2.0;
        let brush = fig.cells(0.12, 2.0);
        let stick = fig.tones.of(3, TWIG);
        fig.paint.line([x, body.chest[1]], [x, brush], r, stick);
        fig.paint.convex(
            &[[x, brush + 1.0], [x - 1.5, 0.0], [x + 1.5, 0.0]],
            fig.tones.of(3, GOLD),
        );
    }
    for i in 0..2 {
        leg(fig, &body, i, r, [trim, cloth], cloth);
    }
    // The dress: from under the chin out to a hem below the hips that trails behind a sway,
    // with a sash at the waist.
    let neck = body.chest[1] + 1.5;
    let hem_y = (fig.row(body.hips[1]) - fig.cells(0.19, 1.0)).max(0.0);
    let trail = fig.at([body.hips[0] * 0.5, 0.0])[0];
    fig.paint.convex(
        &[
            [body.chest[0] - top, neck],
            [body.chest[0] + top, neck],
            [trail + hem, hem_y],
            [trail - hem, hem_y],
        ],
        cloth,
    );
    if top >= 2.5 {
        let waist = fig.at(body.spine(0.45));
        let rows = fig.cells(0.022, 1.0);
        fig.paint.rect(
            [waist[0] - top, waist[1] - 0.5],
            [waist[0] + top, waist[1] - 0.5 + rows],
            trim,
        );
    }
    for i in 0..2 {
        arm(fig, &body, i, r, skin, skin, true);
    }
    // Hair down both sides of the face, where there's a column for it.
    if head_r >= 2.5 && fig.reach >= head_r + 1.0 {
        for side in SIDES {
            let x = c[0] + side * (head_r + 0.5);
            fig.paint
                .line([x, c[1] + head_r - 1.5], [x, body.chest[1] + 1.0], r, trim);
        }
    }
    fig.paint.disc(c, head_r, skin);
    fig.eyes(c, head_r);
    fig.mouth(c, head_r, BLACK);
    // The hat: a crooked point over a band, on a wide brim.
    let width = (head_r - 1.0).max(0.5);
    cone_hat(fig, &body, width, (0.15 * body.hat).round(), [cloth, trim]);
    if head_r >= 1.5 {
        let brim = (head_r + fig.cells(0.025, 1.0)).min(fig.reach + 0.5);
        let rows = fig.cells(0.022, 1.0);
        fig.paint.rect(
            [c[0] - brim, c[1] + head_r - rows],
            [c[0] + brim, c[1] + head_r],
            cloth,
        );
    }
}

fn santa(fig: &mut Figure) {
    let body = Body::new(&SANTA, fig);
    let suit = fig.tones.of(0, RED);
    let fur = fig.tones.of(1, WHITE);
    let skin = fig.tones.of(2, SKIN);
    let buckle = fig.tones.of(3, GOLD);
    let boot = fig.tones.of(4, COAL);
    let limb = fig.limb(0.02);
    let (c, head_r) = (body.head, body.head_r);
    for i in 0..2 {
        leg(fig, &body, i, limb, [suit; 2], boot);
        // Boots up the shins with fur at their tops, on a figure tall enough to have shins.
        if fig.size >= 24.0 {
            let foot = body.feet[i];
            let top = foot[1] + fig.cells(0.035, 1.0);
            fig.paint.line(foot, [foot[0], top], limb, boot);
            fig.paint
                .line([foot[0], top + 1.0], [foot[0], top + 1.0], limb, fur);
        }
    }
    // The coat: sloping shoulders, a belly, a fur hem, and a belt with its buckle.
    let belly = fig.girth(0.15);
    let top = fig.girth(0.08).min(belly);
    let hem = fig.row(body.hips[1]) - fig.cells(0.03, 1.0);
    let waist = fig.at(body.spine(0.4));
    let neck = body.chest[1] + 0.5;
    fig.paint.convex(
        &[
            [body.chest[0] - top, neck],
            [body.chest[0] + top, neck],
            [body.chest[0] + belly, neck - (belly - top)],
            [body.hips[0] + belly, hem],
            [body.hips[0] - belly, hem],
            [body.chest[0] - belly, neck - (belly - top)],
        ],
        suit,
    );
    if belly >= 2.5 {
        let trim = fig.cells(0.03, 1.0);
        fig.paint.rect(
            [body.hips[0] - belly, hem],
            [body.hips[0] + belly, hem + trim],
            fur,
        );
        let belt = fig.cells(0.035, 1.0);
        let (lo, hi) = (waist[1] - 0.5, waist[1] - 0.5 + belt);
        fig.paint
            .rect([waist[0] - belly, lo], [waist[0] + belly, hi], BLACK);
        let w = if belly >= 4.5 { 1.5 } else { 0.5 };
        fig.paint.rect([waist[0] - w, lo], [waist[0] + w, hi], buckle);
    }
    for i in 0..2 {
        arm(fig, &body, i, limb, suit, fur, true);
    }
    // The head: a face between the hat's fur and the beard.
    fig.paint.disc(c, head_r, skin);
    if head_r >= 1.5 {
        let chin = c[1] - head_r;
        let drop = fig.cells(0.07, 1.0);
        fig.paint.convex(
            &[
                [c[0] - head_r, c[1] - 0.5],
                [c[0] + head_r, c[1] - 0.5],
                [c[0] + head_r, chin + 1.0],
                [c[0], chin - drop],
                [c[0] - head_r, chin + 1.0],
            ],
            fur,
        );
    }
    let eyes = if fig.crisp() { 1.0 } else { 0.2 * head_r };
    fig.eyes([c[0], c[1] + eyes], head_r);
    // The hat: fur around the brow, a cone flopping to the right, and a pom-pom on its tip.
    let tip = cone_hat(fig, &body, head_r, (0.5 * body.hat).round(), [suit, suit]);
    if head_r >= 1.5 {
        let rows = fig.cells(0.04, 1.0);
        fig.paint.rect(
            [c[0] - head_r, c[1] + head_r - rows],
            [c[0] + head_r, c[1] + head_r],
            fur,
        );
        fig.paint.disc(tip, (0.022 * fig.size).max(0.75), fur);
    }
}

fn elf(fig: &mut Figure) {
    let body = Body::new(&ELF, fig);
    let suit = fig.tones.of(0, GREEN);
    let trim = fig.tones.of(1, RED);
    let skin = fig.tones.of(2, SKIN);
    let bell = fig.tones.of(3, GOLD);
    let stocking = fig.tones.of(4, WHITE);
    let r = fig.stroke();
    let (c, head_r) = (body.head, body.head_r);
    for i in 0..2 {
        leg(fig, &body, i, r, [trim, stocking], suit);
    }
    // The tunic: from under the chin, flared to a jagged hem below the hips, with a collar and
    // a belt.
    let top = fig.girth(ELF.shoulder);
    let flare = fig.girth(0.1);
    let neck = body.chest[1] + 1.5;
    let hem = fig.row(body.hips[1]) - fig.cells(0.03, 1.0);
    fig.paint.convex(
        &[
            [body.chest[0] - top, neck],
            [body.chest[0] + top, neck],
            [body.hips[0] + flare, hem],
            [body.hips[0] - flare, hem],
        ],
        suit,
    );
    if flare >= 2.5 {
        let tooth = fig.cells(0.025, 1.0);
        let mut x = -flare + tooth / 2.0;
        while x < flare {
            fig.paint.convex(
                &[
                    [body.hips[0] + x - tooth / 2.0, hem],
                    [body.hips[0] + x + tooth / 2.0, hem],
                    [body.hips[0] + x, hem - tooth],
                ],
                suit,
            );
            x += 2.0 * tooth;
        }
        let waist = fig.at(body.spine(0.45));
        let belt = fig.cells(0.025, 1.0);
        fig.paint.rect(
            [waist[0] - flare, waist[1] - 0.5],
            [waist[0] + flare, waist[1] - 0.5 + belt],
            BLACK,
        );
        fig.paint.rect(
            [waist[0] - 0.5, waist[1] - 0.5],
            [waist[0] + 0.5, waist[1] - 0.5 + belt],
            bell,
        );
        let collar = fig.cells(0.04, 1.0);
        fig.paint.rect(
            [body.chest[0] - top + 1.0, neck - collar],
            [body.chest[0] + top - 1.0, neck],
            trim,
        );
    }
    for i in 0..2 {
        arm(fig, &body, i, r, suit, skin, true);
    }
    // Pointed ears, where there's a column for them.
    if head_r >= 2.5 && fig.reach >= head_r + 1.0 {
        for side in SIDES {
            let ear = [c[0] + side * (head_r + 0.5), c[1]];
            let point = [ear[0] + side * 0.03 * fig.size, ear[1] + 0.03 * fig.size];
            fig.paint.line(ear, fig.at(point), r, skin);
        }
    }
    fig.paint.disc(c, head_r, skin);
    fig.eyes(c, head_r);
    fig.mouth(c, head_r, BLACK);
    // The hat: a band, a point bent to the left, and a bell.
    let tip = cone_hat(fig, &body, head_r, -(0.3 * body.hat).round(), [suit, trim]);
    if head_r >= 1.5 {
        fig.paint.disc(tip, (0.02 * fig.size).max(0.5), bell);
    }
}

/// The ghost: a sheet with a round top, floating on a hem that ripples, with stubby arms. The
/// pose's hips carry it (sway and bounce), its lean tips it, and a jump lifts it. With room
/// above, it floats there, drifting up and down over a few bars.
fn ghost(fig: &mut Figure) {
    let s = fig.size;
    let pose = fig.pose;
    let sheet = fig.tones.of(0, SHEET);
    let crisp = fig.crisp();
    let w = fig.girth(0.2);
    let shift = (fig.reach + 0.5 - w).max(0.0);
    let jump = pose.feet[0][1].min(pose.feet[1][1]).max(0.0);
    let drift = 0.5 + 0.25 * (fig.beat * std::f64::consts::TAU / 16.0).sin() as f32;
    let up = (pose.root[1] * 1.5 + jump) * s + fig.headroom.max(0.0) * drift;
    let x = fig.at([(pose.root[0] * s * 1.5).clamp(-shift, shift), 0.0])[0];
    let top = fig.row(0.92 * s + up + 0.5);
    let middle = top - w;
    let hem = fig.row(0.12 * s + up * 0.8 + 0.5).max(0.0);
    // The hem: points a few cells apart, rippling along twice a beat.
    let pitch = fig.cells(0.13, 4.0);
    let depth = if crisp { (pitch / 2.0).floor() } else { 0.06 * s };
    let ripple = (fig.beat * 2.0) as f32;
    let travel = if crisp {
        ripple.floor()
    } else {
        ripple * pitch / 4.0
    };
    // It tips with the lean: each row a little further over than the one below.
    let slope = pose.lean.to_radians().tan() * 0.6;
    let over = move |y: f32| {
        let d = slope * (y - middle);
        if crisp { d.round().clamp(-shift, shift) } else { d }
    };
    fig.paint
        .fill([x - w - 2.0, hem], [x + w + 2.0, top], sheet, |px, py| {
            let dx = px - x - over(py);
            if py >= middle {
                return (dx * dx + (py - middle).powi(2)).sqrt() - w;
            }
            let phase = ((px - x + travel) / pitch).rem_euclid(1.0);
            let edge = hem + depth * (1.0 - (2.0 * phase - 1.0).abs());
            (dx.abs() - w).max(edge - py)
        });
    // Arms: short, out of the sheet under its round top, then up or down with the pose's arms.
    let length = fig.cells(0.14, 2.0);
    let r = if crisp { 0.5 } else { 0.03 * s };
    for (i, side) in SIDES.into_iter().enumerate() {
        let angle = (pose.arms[i][0] + 0.5 * pose.arms[i][1]).to_radians();
        let y = middle - 0.5 * w;
        let out = (w + 1.5).min(fig.reach.max(w - 0.5));
        let elbow = [x + side * out, y];
        let hand = [
            elbow[0] + side * fold((length * angle.sin()).max(0.0), (fig.reach - out).max(0.0)),
            y - length * angle.cos(),
        ];
        fig.paint
            .line(fig.at([x + side * (w - 0.5), y]), fig.at(elbow), r, sheet);
        fig.paint.line(fig.at(elbow), fig.at(hand), r, sheet);
    }
    // Dark eyes and a mouth that opens on the beat.
    if w < 2.5 {
        return;
    }
    let open = 1.0 - (fig.beat.rem_euclid(1.0) as f32 * 2.0).min(1.0);
    let fx = x + over(middle);
    if crisp {
        let eye = (0.4 * w).round().max(1.0);
        let ex = (0.55 * w).floor().max(1.0) + if eye % 2.0 == 0.0 { 0.5 } else { 0.0 };
        let tall = eye + 1.0;
        let low = middle - (tall / 2.0).ceil() + 0.5;
        for side in SIDES {
            let c = fx + side * ex;
            fig.paint
                .rect([c - eye / 2.0, low], [c + eye / 2.0, low + tall], BLACK);
        }
        let mouth = if w >= 4.5 { 1.5 } else { 0.5 };
        let rows = if open > 0.5 { 2.0 } else { 1.0 };
        fig.paint
            .rect([fx - mouth, low - 1.0 - rows], [fx + mouth, low - 1.0], BLACK);
    } else {
        for side in SIDES {
            fig.paint.ellipse(
                [fx + side * 0.45 * w, middle + 0.1 * w],
                [0.17 * w, 0.26 * w],
                BLACK,
            );
        }
        let y = middle - 0.6 * w;
        fig.paint
            .ellipse([fx, y], [0.2 * w, (0.12 + 0.14 * open) * w], BLACK);
    }
}

/// The snowman: three balls, a hat, a carrot, a scarf, and stick arms. The balls shift over one
/// another with the pose's sway and lean, sit deeper with its bounce, and hop with its jumps.
fn snowman(fig: &mut Figure) {
    let s = fig.size;
    let pose = fig.pose;
    let snow = fig.tones.of(0, WHITE);
    let scarf = fig.tones.of(1, RED);
    let carrot = fig.tones.of(2, ORANGE);
    let hat = fig.tones.of(3, COAL);
    let twig = fig.tones.of(4, TWIG);
    let crisp = fig.crisp();
    let stroke = fig.stroke();
    // The balls, bottom up: the bottom one may fill the width, the middle leaves room for arms.
    let balls = [
        fig.half(0.19).min(fig.reach + 0.5),
        fig.girth(0.155),
        fig.girth(0.155).min(fig.half(0.11)),
    ];
    let whole = |v: f32| if crisp { v.round() } else { v };
    let jump = pose.feet[0][1].min(pose.feet[1][1]).max(0.0) * s;
    let squash = (-pose.root[1] * s * 0.6).clamp(0.0, 0.08 * s);
    let lean = pose.lean.to_radians().sin();
    let shift = |ball: usize, x: f32| {
        let room = (fig.reach + 0.5 - balls[ball]).max(0.0);
        fig.at([x.clamp(-room, room), 0.0])[0]
    };
    let xs = [
        0.0,
        shift(1, pose.root[0] * s + lean * 0.1 * s),
        shift(
            2,
            pose.root[0] * s * 1.5 + lean * 0.25 * s + pose.head[0] * s * 1.5,
        ),
    ];
    // Each ball sits a little into the one below; a bounce sits them deeper.
    let mut ys = [whole(jump) + balls[0], 0.0, 0.0];
    ys[1] = ys[0] + balls[0] + balls[1] - whole(0.03 * s + squash);
    ys[2] = ys[1] + balls[1] + balls[2] - whole(0.03 * s + 0.5 * squash) + whole(pose.head[1] * s);
    // Stick arms from the middle ball, a column out from it, up or down with the pose's arms.
    let length = 0.2 * s;
    for (i, side) in SIDES.into_iter().enumerate() {
        let angle = (pose.arms[i][0] + 0.4 * pose.arms[i][1]).to_radians();
        let out = (balls[1] + 0.5).min(fig.reach.max(balls[1] - 0.5));
        let from = [xs[1] + side * (balls[1] - 1.0), ys[1]];
        let elbow = [xs[1] + side * out, ys[1] - 0.25 * length * angle.cos()];
        let to = [
            elbow[0] + side * fold((length * angle.sin()).max(0.0), (fig.reach - out).max(0.0)),
            ys[1] - length * angle.cos(),
        ];
        fig.paint.line(fig.at(from), fig.at(elbow), stroke, twig);
        fig.paint.line(fig.at(elbow), fig.at(to), stroke, twig);
    }
    for ball in 0..3 {
        fig.paint.disc([xs[ball], ys[ball]], balls[ball], snow);
    }
    let (c, r) = ([xs[2], ys[2]], balls[2]);
    if r >= 1.5 {
        // Coal buttons down the middle ball.
        let button = (0.018 * s).max(0.5);
        for k in [-0.4, 0.3] {
            let at = fig.at([xs[1], ys[1] + k * balls[1]]);
            fig.paint.disc(at, button, BLACK);
        }
        // The scarf: around the neck, an end hanging on the left.
        let neck = c[1] - r;
        let rows = fig.cells(0.04, 1.0);
        let around = (r - 1.0).max(1.5).min(balls[1]);
        fig.paint
            .rect([c[0] - around, neck], [c[0] + around, neck + rows], scarf);
        let end = c[0] - around + 0.5;
        fig.paint.line(
            [end, neck + 0.5],
            [end, neck + 0.5 - fig.cells(0.07, 1.0)],
            (0.02 * s).max(0.5),
            scarf,
        );
    }
    // Coal eyes, and a carrot pointing right.
    let eyes = if crisp { 1.0 } else { 0.15 * r };
    fig.eyes([c[0], c[1] + eyes], r);
    if r >= 2.5 {
        let y = if crisp { c[1] } else { c[1] - 0.15 * r };
        let length = if crisp { (0.6 * r).round() } else { 0.75 * r };
        fig.paint
            .line([c[0], y], [c[0] + length, y], (0.12 * r).max(0.5), carrot);
    }
    // The hat: a brim across the top of the head, a crown, and a band.
    let top = c[1] + r;
    let brim = (r + fig.cells(0.02, 1.0)).min(fig.reach + 0.5);
    let crown = (r - 1.0).max(1.5).min(r);
    let rows = fig.cells(0.022, 1.0);
    let height = fig.cells(0.14, 2.0);
    fig.paint
        .rect([c[0] - crown, top - 1.0], [c[0] + crown, top - 1.0 + height], hat);
    if r >= 1.5 {
        fig.paint
            .rect([c[0] - brim, top - 1.0], [c[0] + brim, top - 1.0 + rows], hat);
        fig.paint.rect(
            [c[0] - crown, top - 1.0 + rows],
            [c[0] + crown, top - 1.0 + 2.0 * rows],
            scarf,
        );
    }
}

/// Draws `character` in the pose and place `fig` gives.
pub(crate) fn draw(character: DancerCharacter, fig: &mut Figure) {
    match character {
        DancerCharacter::Skeleton => skeleton(fig),
        DancerCharacter::Ghost => ghost(fig),
        DancerCharacter::Witch => witch(fig),
        DancerCharacter::Santa => santa(fig),
        DancerCharacter::Snowman => snowman(fig),
        DancerCharacter::Elf => elf(fig),
    }
}
