//! DMX fixtures held against xLights' render channel by channel.
//!
//! A flood, a moving head, or a set of servos has channels, not pixels, so the look score never
//! sees it. Here each fixture's channels in PixelFlow's render are held against the same channels
//! in xLights', frame by frame, as what a viewer would see of them:
//!
//! - **Where it points.** Each pan, tilt, and servo as a position in degrees from Home
//!   ([`DmxAxis::degrees`]: 16-bit where it has a fine channel). Within [`TURN`] degrees (or a
//!   step of its channel, where that is more) it points the same way; beyond [`ASTRAY`] it points
//!   somewhere else.
//! - **The light it shows.** The color its channels make on each cell ([`emitted_colors`]), dark
//!   where its shutter is closed. Red, green, and blue, a color wheel, cyan, magenta, and yellow,
//!   and a dimmer all compare as the light they make, whichever channels carry it.
//! - **What it holds.** Every other channel (strobe, mode, gobo, speed, zoom) as its value; a
//!   strobe channel as what its value does: closed, open, or a strobe at a pace.
//!
//! A head's pan and tilt and what it holds are only seen while it is lit, so they are compared
//! on the frames either render lights it. A servo makes no light, and is compared on every frame.
//!
//! Each of PixelFlow's frames is held against xLights' from the frame before to the frame after:
//! anywhere between where xLights has a value then, within the tolerance, is the same. A move or
//! a fade a frame (or part of one) early or late isn't counted; no one sees 50 milliseconds.
//!
//! Each part (a motor, the light, the held channels) earns a verdict from how many of its frames
//! are off: at most [`MATCHES_OFF`] of them and it **matches**; far off (pointing elsewhere,
//! another color, dark where the other is lit) on more than [`DIFFERENT_FAR`] of them and it is
//! **different**; otherwise **close**. A held value that differs is never more than close. A
//! fixture earns the worst verdict of its parts, and the fixtures of a sequence as a whole the
//! worst of theirs.

use pf_model::{DmxAxis, DmxChannel, DmxKind, DmxRole, ShutterState, axes, emitted_colors, shutter_at};
use pf_xlights::sequence::{ByVerdict, Verdict};
use serde::Serialize;

/// Degrees a pan, tilt, or servo may be off and still point the same way. A beam 15° wide moved
/// 2° still lights seven-eighths of the same spot, and a servo's arm 2° off can't be told at
/// show distance. An 8-bit pan of 540° steps 2.1° a value, so a motor is always allowed a step of
/// its own channel: a value rounded the other way is no difference.
pub const TURN: f64 = 2.0;
/// Degrees beyond which a motor points somewhere else: the width of a usual beam
/// ([`pf_model::DmxBeam`]'s 15°), past which the two beams no longer overlap.
pub const ASTRAY: f64 = 15.0;
/// Levels (of 255) each of a light's red, green, and blue may be off and show the same. The same
/// light carried by a dimmer on one side and by the color channels on the other comes back a
/// level or two apart, and a dim color more.
pub const LEVELS: f64 = 8.0;
/// ...or this share of the light's brightness, where that is more: the look score lets a fifth
/// of a brightness pass for the same, and an eighth is less than anyone sees.
pub const RELATIVE: f64 = 0.125;
/// Levels (of 255) two lights may differ by, each brought up to full, and be the same color at
/// another brightness: a quarter of the way to another color.
pub const SAME_COLOR: f64 = 64.0;
/// A strobe may flash this share faster or slower and flash the same.
pub const PACE: f64 = 0.2;
/// A part off on at most this share of the frames it is compared on matches.
pub const MATCHES_OFF: f64 = 0.02;
/// A motor or a light far off on more than this share of its frames is different.
pub const DIFFERENT_FAR: f64 = 0.25;

/// Where the lines are drawn when fixtures are compared, as a scorecard records them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lines {
    pub turn_deg: f64,
    pub astray_deg: f64,
    pub levels: f64,
    pub relative: f64,
    pub same_color: f64,
    pub pace: f64,
    pub matches_off: f64,
    pub different_far: f64,
}

impl Default for Lines {
    fn default() -> Self {
        Self {
            turn_deg: TURN,
            astray_deg: ASTRAY,
            levels: LEVELS,
            relative: RELATIVE,
            same_color: SAME_COLOR,
            pace: PACE,
            matches_off: MATCHES_OFF,
            different_far: DIFFERENT_FAR,
        }
    }
}

/// One fixture's channels in both renders, frame by frame: what is compared.
#[derive(Debug, Clone, Default)]
pub struct Track {
    width: usize,
    /// Per frame: whether both renders have it.
    compared: Vec<bool>,
    /// Per frame, PixelFlow's channels then xLights' (zeros for a frame not compared).
    values: Vec<u8>,
}

impl Track {
    /// A track of a fixture with `width` channels.
    pub fn new(width: usize) -> Self {
        Self {
            width,
            ..Self::default()
        }
    }

    /// Adds the next frame: PixelFlow's channels and xLights', or `None` where either render
    /// hasn't got the frame.
    pub fn push(&mut self, both: Option<(&[u8], &[u8])>) {
        self.compared.push(both.is_some());
        let (ours, theirs) = both.unwrap_or((&[], &[]));
        for side in [ours, theirs] {
            let start = self.values.len();
            self.values.extend(side.iter().take(self.width));
            self.values.resize(start + self.width, 0);
        }
    }

    fn len(&self) -> usize {
        self.compared.len()
    }

    /// Both renders' channels at `frame`, where both have it.
    fn at(&self, frame: usize) -> Option<[&[u8]; 2]> {
        if !*self.compared.get(frame)? {
            return None;
        }
        let start = frame * 2 * self.width;
        let (ours, theirs) = self.values[start..start + 2 * self.width].split_at(self.width);
        Some([ours, theirs])
    }
}

/// What one part of a fixture is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PartKind {
    /// A pan, a tilt, or a servo: where it points.
    Motor,
    /// The light it shows.
    Light,
    /// Every other channel: what it holds.
    Held,
}

/// One part of a fixture compared with xLights'.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// A motor's channel name ("Pan", "Servo 2"), "Light", or "Held".
    pub name: String,
    pub kind: PartKind,
    /// A motor's resolution: 8 or 16 bits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits: Option<u8>,
    /// Frames it was compared on: every frame compared, or for a head's motors and held
    /// channels those either render lights it on.
    pub frames: u32,
    /// The share of those frames it was a little off on (a difference a viewer could notice),
    /// and far off on (pointing elsewhere, another color, dark where the other is lit). A held
    /// value is never far off.
    pub near: f64,
    pub far: f64,
    /// The most it was off: degrees for a motor, levels of 255 for the light.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most: Option<f64>,
    pub verdict: Verdict,
    /// What a viewer would notice, in a few words; `None` when it matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub differs: Option<String>,
}

impl Part {
    /// The verdict it would earn with `slack` more of its frames (a share of them) the same.
    pub fn verdict_with(&self, slack: f64) -> Verdict {
        earned(
            self.kind,
            (self.near + self.far - slack).max(0.0),
            (self.far - slack).max(0.0),
        )
    }
}

/// The verdict a part earns, off on `off` of its frames and far off on `far` of them.
fn earned(kind: PartKind, off: f64, far: f64) -> Verdict {
    if off <= MATCHES_OFF {
        Verdict::Matches
    } else if kind != PartKind::Held && far > DIFFERENT_FAR {
        Verdict::Different
    } else {
        Verdict::Close
    }
}

/// How one DMX fixture's channels compare with xLights'.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureScore {
    /// The prop's name.
    pub name: String,
    /// A moving head, servos, or a flood (anything else with channels).
    pub kind: DmxKind,
    /// First and last channel compared, counting from 1; `None` where the reference hasn't got
    /// them (or PixelFlow couldn't place the fixture).
    pub channels: Option<(u32, u32)>,
    /// Frames both renders have.
    pub frames: u32,
    /// Of every 100 frames compared, how many it showed the same on in every part; `None` when
    /// it wasn't compared.
    pub score: Option<f64>,
    /// The worst verdict of its parts; `None` when it wasn't compared.
    pub verdict: Option<Verdict>,
    /// What a viewer would notice, worst first; `None` when it matches.
    pub differs: Option<String>,
    /// Each motor, the light (unless neither render ever lights it), and the held channels (if
    /// it has any).
    pub parts: Vec<Part>,
}

impl FixtureScore {
    /// The verdict it would earn with `slack` more of each part's frames (a share of them) the
    /// same; `None` when it wasn't compared.
    pub fn verdict_with(&self, slack: f64) -> Option<Verdict> {
        self.verdict?;
        Some(
            self.parts
                .iter()
                .map(|p| p.verdict_with(slack))
                .max()
                .unwrap_or(Verdict::Matches),
        )
    }
}

/// What kind of fixture `channels` make.
fn kind_of(channels: &[DmxChannel]) -> DmxKind {
    let has = |role: DmxRole| channels.iter().any(|c| c.role == role);
    if has(DmxRole::Pan) || has(DmxRole::Tilt) {
        DmxKind::Head
    } else if has(DmxRole::Servo) {
        DmxKind::Servo
    } else {
        DmxKind::Flood
    }
}

/// Whether a fixture with `channels` makes light: servos with no dimmer don't (as
/// [`emitted_colors`] has it).
fn makes_light(channels: &[DmxChannel]) -> bool {
    let has = |role: DmxRole| channels.iter().any(|c| c.role == role);
    has(DmxRole::Dimmer) || !has(DmxRole::Servo)
}

/// The channels compared as what they hold: not a motor's, not one the light comes from.
fn is_held(channel: &DmxChannel) -> bool {
    !channel.role.moves() && !channel.from_color() && channel.role != DmxRole::Uv
}

/// What a held channel's value does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Held {
    Value(u8),
    Shutter(ShutterState),
}

impl Held {
    fn of(channel: &DmxChannel, value: u8) -> Self {
        match channel.shutter() {
            Some(ranges) => Held::Shutter(shutter_at(ranges, value)),
            None => Held::Value(value),
        }
    }

    /// The same to a viewer: the same value, or a shutter doing the same (a strobe within
    /// [`PACE`] of the same pace).
    fn same(self, other: Held) -> bool {
        match (self, other) {
            (Held::Shutter(ShutterState::Strobe(a)), Held::Shutter(ShutterState::Strobe(b))) => {
                (a - b).abs() <= PACE * a.max(b)
            }
            _ => self == other,
        }
    }
}

/// What one render shows of a fixture at a frame.
struct Seen {
    /// Each motor's position, in degrees from Home.
    degrees: Vec<f64>,
    /// Each cell's color: red, green, blue.
    light: Vec<u8>,
    /// The brightest of those.
    peak: u8,
    /// Each held channel.
    held: Vec<Held>,
}

/// The light a fixture with `channels` shows with them at `dmx`: the color each cell's channels
/// make, dark where its shutter is closed (a strobe shows its light).
fn shown(channels: &[DmxChannel], cells: usize, dmx: &[u8]) -> Vec<u8> {
    let mut light = vec![0u8; cells * 3];
    emitted_colors(channels, dmx, &mut light);
    let closed = channels
        .iter()
        .enumerate()
        .find_map(|(i, c)| {
            c.shutter()
                .map(|r| shutter_at(r, dmx.get(i).copied().unwrap_or(0)))
        })
        .is_some_and(|state| state == ShutterState::Closed);
    if closed {
        light.fill(0);
    }
    light
}

impl Seen {
    fn of(channels: &[DmxChannel], axes: &[DmxAxis], held: &[usize], cells: usize, dmx: &[u8]) -> Self {
        let light = shown(channels, cells, dmx);
        Self {
            degrees: axes.iter().map(|a| a.degrees(dmx)).collect(),
            peak: light.iter().copied().max().unwrap_or(0),
            light,
            held: held
                .iter()
                .map(|&i| Held::of(&channels[i], dmx.get(i).copied().unwrap_or(0)))
                .collect(),
        }
    }
}

/// How far off a frame of a part is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Off {
    Not,
    Near,
    Far,
}

/// How a light is off, for saying so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    Dimmer,
    Brighter,
    Tint,
    Color,
    Dark,
    Lit,
}

impl Why {
    const ALL: [Why; 6] = [
        Why::Dimmer,
        Why::Brighter,
        Why::Tint,
        Why::Color,
        Why::Dark,
        Why::Lit,
    ];

    fn far(self) -> bool {
        matches!(self, Why::Color | Why::Dark | Why::Lit)
    }

    fn words(self) -> &'static str {
        match self {
            Why::Dimmer => "light dimmer",
            Why::Brighter => "light brighter",
            Why::Tint => "color a little off",
            Why::Color => "color differs",
            Why::Dark => "dark where xLights lights it",
            Why::Lit => "lit where xLights is dark",
        }
    }
}

/// A part's frames, counted as they're compared.
#[derive(Debug, Clone, Default)]
struct Tally {
    frames: u32,
    near: u32,
    far: u32,
    most: f64,
    /// For the light, how often it was off each way ([`Why::ALL`]'s order); for the held
    /// channels, how often each differed.
    counts: Vec<u32>,
}

impl Tally {
    fn add(&mut self, off: Off, by: f64) {
        self.frames += 1;
        self.near += u32::from(off == Off::Near);
        self.far += u32::from(off == Off::Far);
        self.most = self.most.max(by);
    }

    fn share(&self, n: u32) -> f64 {
        if self.frames == 0 {
            0.0
        } else {
            round(f64::from(n) / f64::from(self.frames))
        }
    }

    /// The part, with what a viewer would notice once its verdict is known (`say`, from the
    /// verdict and the share of frames off).
    fn part(
        &self,
        name: &str,
        kind: PartKind,
        bits: Option<u8>,
        most: Option<f64>,
        say: impl FnOnce(Verdict, &str) -> String,
    ) -> Part {
        let (near, far) = (self.share(self.near), self.share(self.far));
        let verdict = earned(kind, near + far, far);
        Part {
            name: name.to_string(),
            kind,
            bits,
            frames: self.frames,
            near,
            far,
            most,
            verdict,
            differs: (verdict != Verdict::Matches).then(|| say(verdict, &percent(near + far))),
        }
    }
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn percent(share: f64) -> String {
    format!("{:.0}%", share * 100.0)
}

/// A channel's name as it reads in a sentence: "pan", "servo 2", "UV".
fn plain(name: &str) -> String {
    if name.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase) {
        name.to_string()
    } else {
        name.to_lowercase()
    }
}

/// `names` as a list in a sentence: "gobo", "gobo and strobe", "gobo, mode and strobe".
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// How a frame of PixelFlow's light (`ours`, three bytes a cell) compares with xLights' over
/// the frames around it (`theirs`): how far off, the most levels off, and why.
fn light_off(ours: &[u8], theirs: &[&[u8]], lit: u8) -> (Off, u8, Option<Why>) {
    let mut worst = (Off::Not, None);
    let mut most = 0u8;
    for (cell, o) in ours.as_chunks::<3>().0.iter().enumerate() {
        // The nearest light to ours between xLights' at the frame before and at the frame after.
        let mut n = [0u8; 3];
        for (k, n) in n.iter_mut().enumerate() {
            let values = theirs.iter().map(|t| t[cell * 3 + k]);
            let (lo, hi) = (values.clone().min().unwrap_or(0), values.max().unwrap_or(0));
            *n = o[k].clamp(lo, hi);
        }
        let by = (0..3).map(|k| o[k].abs_diff(n[k])).max().unwrap_or(0);
        most = most.max(by);
        let (po, pn) = (
            o.iter().copied().max().unwrap_or(0),
            n.iter().copied().max().unwrap_or(0),
        );
        let allowed = LEVELS.max(RELATIVE * f64::from(po.max(pn)));
        let full = |c: &[u8], peak: u8, k: usize| f64::from(c[k]) * 255.0 / f64::from(peak);
        let off = if (po <= lit && pn <= lit) || f64::from(by) <= allowed {
            (Off::Not, None)
        } else if po <= lit {
            (Off::Far, Some(Why::Dark))
        } else if pn <= lit {
            (Off::Far, Some(Why::Lit))
        } else if (0..3).any(|k| (full(o, po, k) - full(&n, pn, k)).abs() > SAME_COLOR) {
            (Off::Far, Some(Why::Color))
        } else if f64::from(po.abs_diff(pn)) > allowed {
            (Off::Near, Some(if po < pn { Why::Dimmer } else { Why::Brighter }))
        } else {
            (Off::Near, Some(Why::Tint))
        };
        if off.0 > worst.0 {
            worst = off;
        }
    }
    (worst.0, most, worst.1)
}

/// Holds the fixture `name`, whose channels are `channels` and which shows its light on `cells`
/// cells, to xLights' over the frames of `track`. A light at or under `lit` (of 255) is dark.
/// The channels it was found at are for the caller to fill in.
pub fn judge(name: &str, channels: &[DmxChannel], cells: usize, track: &Track, lit: u8) -> FixtureScore {
    let kind = kind_of(channels);
    let frames: Vec<usize> = (0..track.len()).filter(|&f| track.at(f).is_some()).collect();
    let mut score = FixtureScore {
        name: name.to_string(),
        kind,
        channels: None,
        frames: frames.len() as u32,
        score: None,
        verdict: None,
        differs: None,
        parts: Vec::new(),
    };
    if frames.is_empty() {
        return score;
    }
    let cells = cells.max(1);
    let axes = axes(channels);
    let held: Vec<usize> = (0..channels.len()).filter(|&i| is_held(&channels[i])).collect();
    let seen: Vec<Option<[Seen; 2]>> = (0..track.len())
        .map(|f| {
            track
                .at(f)
                .map(|sides| sides.map(|dmx| Seen::of(channels, &axes, &held, cells, dmx)))
        })
        .collect();
    let lit_at = |f: usize| seen[f].as_ref().is_some_and(|s| s.iter().any(|s| s.peak > lit));
    let light = makes_light(channels);
    let lit_ever = light && frames.iter().any(|&f| lit_at(f));

    let mut motors = vec![Tally::default(); axes.len()];
    let mut light_tally = Tally {
        counts: vec![0; Why::ALL.len()],
        ..Tally::default()
    };
    let mut held_tally = Tally {
        counts: vec![0; held.len()],
        ..Tally::default()
    };
    let mut same = 0u32;
    for &f in &frames {
        let Some([ours, _]) = &seen[f] else { continue };
        // xLights' from the frame before to the frame after, where it has them.
        let theirs: Vec<&Seen> = [f.wrapping_sub(1), f, f + 1]
            .into_iter()
            .filter_map(|g| seen.get(g)?.as_ref().map(|s| &s[1]))
            .collect();
        let mut all = true;
        // A head's aim and what it holds show only while it is lit.
        if !light || lit_at(f) {
            for (k, (axis, tally)) in axes.iter().zip(&mut motors).enumerate() {
                let p = ours.degrees[k];
                let (lo, hi) = theirs
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), s| {
                        (lo.min(s.degrees[k]), hi.max(s.degrees[k]))
                    });
                let by = (lo - p).max(p - hi).max(0.0);
                let full = if axis.fine.is_some() { 65535.0 } else { 255.0 };
                let allowed = TURN.max(f64::from(axis.motor.range_deg) / full) + 1e-9;
                let off = if by <= allowed {
                    Off::Not
                } else if by <= ASTRAY {
                    Off::Near
                } else {
                    Off::Far
                };
                tally.add(off, by);
                all &= off == Off::Not;
            }
            if !held.is_empty() {
                let mut off = Off::Not;
                for (j, count) in held_tally.counts.iter_mut().enumerate() {
                    if !theirs.iter().any(|s| ours.held[j].same(s.held[j])) {
                        *count += 1;
                        off = Off::Near;
                    }
                }
                held_tally.add(off, 0.0);
                all &= off == Off::Not;
            }
        }
        if lit_ever {
            let around: Vec<&[u8]> = theirs.iter().map(|s| s.light.as_slice()).collect();
            let (off, by, why) = light_off(&ours.light, &around, lit);
            light_tally.add(off, f64::from(by));
            if let Some(why) = why {
                light_tally.counts[why as usize] += 1;
            }
            all &= off == Off::Not;
        }
        same += u32::from(all);
    }

    for (axis, tally) in axes.iter().zip(&motors) {
        if tally.frames == 0 {
            continue;
        }
        let channel = &channels[axis.coarse].name;
        let most = round(tally.most);
        score.parts.push(tally.part(
            channel,
            PartKind::Motor,
            Some(if axis.fine.is_some() { 16 } else { 8 }),
            Some(most),
            |_, share| format!("{} off by up to {most:.0}° in {share} of frames", plain(channel)),
        ));
    }
    if lit_ever {
        let counts = &light_tally.counts;
        score.parts.push(light_tally.part(
            "Light",
            PartKind::Light,
            None,
            Some(light_tally.most),
            |verdict, _| {
                // The way it is off most often, of those that earn the verdict.
                let why = Why::ALL
                    .into_iter()
                    .filter(|w| verdict != Verdict::Different || w.far())
                    .max_by_key(|&w| (counts[w as usize], std::cmp::Reverse(w as usize)))
                    .unwrap_or(Why::Color);
                format!(
                    "{} in {} of frames",
                    why.words(),
                    percent(light_tally.share(counts[why as usize]))
                )
            },
        ));
    }
    if held_tally.frames > 0 {
        let counts = &held_tally.counts;
        let most = counts.iter().copied().max().unwrap_or(0);
        score
            .parts
            .push(held_tally.part("Held", PartKind::Held, None, None, |_, share| {
                // The channels that differ often enough to count, or the one that differs most.
                let mut names: Vec<String> = held
                    .iter()
                    .zip(counts)
                    .filter(|&(_, &n)| held_tally.share(n) > MATCHES_OFF || n == most)
                    .map(|(&i, _)| plain(&channels[i].name))
                    .collect();
                names.dedup();
                let verb = if names.len() == 1 { "differs" } else { "differ" };
                format!("{} {verb} in {share} of frames", listed(&names))
            }));
    }

    let verdict = score
        .parts
        .iter()
        .map(|p| p.verdict)
        .max()
        .unwrap_or(Verdict::Matches);
    let mut off: Vec<&Part> = score.parts.iter().filter(|p| p.differs.is_some()).collect();
    off.sort_by(|a, b| {
        b.verdict
            .cmp(&a.verdict)
            .then((b.near + b.far).total_cmp(&(a.near + a.far)))
    });
    score.differs = (!off.is_empty()).then(|| {
        off.iter()
            .filter_map(|p| p.differs.as_deref())
            .collect::<Vec<_>>()
            .join("; ")
    });
    score.verdict = Some(verdict);
    score.score = Some((f64::from(same) / frames.len() as f64 * 10_000.0).round() / 100.0);
    score
}

/// The fixtures of a sequence (or a corpus) as a whole.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureSummary {
    /// The worst verdict any fixture compared earned; `None` when none was compared.
    pub verdict: Option<Verdict>,
    /// Each fixture that earned it and why ("Spot Right: pan off by up to 14° in 30% of
    /// frames"); `None` when they all match.
    pub differs: Option<String>,
    /// The fixtures compared, by the verdict each earned.
    pub fixtures: ByVerdict<usize>,
    /// Fixtures compared, of how many the layout has.
    pub judged: usize,
    pub of: usize,
}

impl FixtureSummary {
    /// The summary of a sequence's fixtures.
    pub fn of(fixtures: &[FixtureScore]) -> Self {
        let mut summary = Self {
            of: fixtures.len(),
            ..Self::default()
        };
        for fixture in fixtures {
            if let Some(verdict) = fixture.verdict {
                summary.judged += 1;
                summary.fixtures.add(verdict, 1);
            }
        }
        summary.verdict = fixtures.iter().filter_map(|f| f.verdict).max();
        let worst: Vec<String> = fixtures
            .iter()
            .filter(|f| f.verdict == summary.verdict)
            .filter_map(|f| Some(format!("{}: {}", f.name, f.differs.as_deref()?)))
            .collect();
        summary.differs = (!worst.is_empty()).then(|| worst.join("; "));
        summary
    }

    /// Adds the fixtures of the sequence `label`.
    pub fn merge(&mut self, other: &FixtureSummary, label: &str) {
        self.judged += other.judged;
        self.of += other.of;
        self.fixtures.merge(&other.fixtures);
        let theirs = other.differs.as_ref().map(|d| format!("{d} ({label})"));
        match other.verdict.cmp(&self.verdict) {
            std::cmp::Ordering::Greater => {
                self.verdict = other.verdict;
                self.differs = theirs;
            }
            std::cmp::Ordering::Equal => {
                if let Some(theirs) = theirs {
                    self.differs = Some(match self.differs.take() {
                        Some(mine) => format!("{mine}; {theirs}"),
                        None => theirs,
                    });
                }
            }
            std::cmp::Ordering::Less => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{DmxLayout, DmxSlot, Rgb, dmx_layout};

    const LIT: u8 = 24;

    fn layout(id: &str, cells: u32) -> Vec<DmxChannel> {
        let layout: &DmxLayout = dmx_layout(id).unwrap();
        layout.channels(cells)
    }

    /// A track of `frames` frames, each side's channels made by `ours` and `theirs` from the
    /// frame's number.
    fn track(
        width: usize,
        frames: usize,
        ours: impl Fn(usize) -> Vec<u8>,
        theirs: impl Fn(usize) -> Vec<u8>,
    ) -> Track {
        let mut track = Track::new(width);
        for f in 0..frames {
            track.push(Some((&ours(f), &theirs(f))));
        }
        track
    }

    /// A 16-bit RGBW head (pan, pan fine, tilt, tilt fine, speed, dimmer, strobe, red, green,
    /// blue, white) lit red, pointing `pan` and `tilt` degrees from Home.
    fn head(pan: f64, tilt: f64) -> Vec<u8> {
        let channels = layout("head-rgbw-16", 1);
        let axes = axes(&channels);
        let mut dmx = vec![0u8; channels.len()];
        for (axis, degrees) in axes.iter().zip([pan, tilt]) {
            let (coarse, fine) = axis.values(degrees);
            dmx[axis.coarse] = coarse;
            if let (Some(at), Some(v)) = (axis.fine, fine) {
                dmx[at] = v;
            }
        }
        dmx[5] = 255;
        dmx[7] = 255;
        dmx
    }

    fn part<'a>(score: &'a FixtureScore, name: &str) -> &'a Part {
        score.parts.iter().find(|p| p.name == name).unwrap()
    }

    #[test]
    fn a_fixture_the_same_in_both_renders_matches() {
        let channels = layout("head-rgbw-16", 1);
        let sweep = |f: usize| head(f as f64 * 3.0 - 90.0, 45.0);
        let score = judge(
            "Spot",
            &channels,
            1,
            &track(channels.len(), 60, sweep, sweep),
            LIT,
        );
        assert_eq!(score.verdict, Some(Verdict::Matches));
        assert_eq!(
            (score.frames, score.score, score.differs.as_deref()),
            (60, Some(100.0), None)
        );
        assert_eq!(score.kind, DmxKind::Head);
        let names: Vec<&str> = score.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Pan", "Tilt", "Light", "Held"]);
        assert_eq!(part(&score, "Pan").bits, Some(16));
        assert!(
            score
                .parts
                .iter()
                .all(|p| p.verdict == Verdict::Matches && p.frames == 60)
        );
    }

    #[test]
    fn a_pan_a_few_degrees_off_is_close_and_one_pointing_elsewhere_different() {
        let channels = layout("head-rgbw-16", 1);
        let judged = |off: f64| {
            let track = track(
                channels.len(),
                40,
                |_| head(30.0 + off, 10.0),
                |_| head(30.0, 10.0),
            );
            judge("Spot", &channels, 1, &track, LIT)
        };
        let near = judged(1.5);
        assert_eq!(near.verdict, Some(Verdict::Matches), "within 2°");
        let off = judged(6.0);
        assert_eq!(off.verdict, Some(Verdict::Close));
        assert_eq!(off.score, Some(0.0));
        assert_eq!(
            off.differs.as_deref(),
            Some("pan off by up to 6° in 100% of frames")
        );
        assert_eq!((part(&off, "Pan").near, part(&off, "Pan").far), (1.0, 0.0));
        assert_eq!(part(&off, "Tilt").verdict, Verdict::Matches);
        let elsewhere = judged(40.0);
        assert_eq!(elsewhere.verdict, Some(Verdict::Different));
        assert_eq!(
            elsewhere.differs.as_deref(),
            Some("pan off by up to 40° in 100% of frames")
        );
        // Off for a few frames of many is still a match; for a tenth of them, close.
        let few = |frames: usize| {
            let track = track(
                channels.len(),
                100,
                |f| head(if f < frames { 70.0 } else { 30.0 }, 10.0),
                |_| head(30.0, 10.0),
            );
            judge("Spot", &channels, 1, &track, LIT).verdict
        };
        assert_eq!(few(2), Some(Verdict::Matches));
        assert_eq!(few(10), Some(Verdict::Close));
        assert_eq!(few(30), Some(Verdict::Different));
    }

    #[test]
    fn a_move_a_frame_late_is_the_same_move() {
        let channels = layout("head-rgbw-16", 1);
        // A quick sweep, 9° a frame, and a jump; xLights' a frame behind PixelFlow's.
        let at = |f: usize| head(if f < 30 { f as f64 * 9.0 - 135.0 } else { 120.0 }, 0.0);
        let track = track(channels.len(), 40, at, |f| at(f.saturating_sub(1)));
        let score = judge("Spot", &channels, 1, &track, LIT);
        assert_eq!(score.verdict, Some(Verdict::Matches), "{:?}", score.differs);
        // Half a frame either way is as good.
        let half = |f: usize| head(if f < 29 { f as f64 * 9.0 - 130.5 } else { 126.0 }, 0.0);
        let track = self::track(channels.len(), 30, half, |f| head(f as f64 * 9.0 - 135.0, 0.0));
        assert_eq!(
            judge("Spot", &channels, 1, &track, LIT).verdict,
            Some(Verdict::Matches)
        );
    }

    #[test]
    fn an_8_bit_motor_is_allowed_a_step_of_its_channel_and_a_16_bit_one_two_degrees() {
        // An 8-bit pan of 540° steps 2.1°: a value rounded the other way is the same place.
        let channels = layout("head-rgbw-8", 1);
        let at = |pan: u8| {
            let mut dmx = vec![0u8; channels.len()];
            (dmx[0], dmx[1], dmx[3], dmx[4]) = (pan, 128, 255, 255);
            dmx
        };
        let score = judge(
            "Eight",
            &channels,
            1,
            &track(channels.len(), 20, |_| at(171), |_| at(170)),
            LIT,
        );
        assert_eq!(part(&score, "Pan").bits, Some(8));
        assert_eq!(score.verdict, Some(Verdict::Matches));
        let score = judge(
            "Eight",
            &channels,
            1,
            &track(channels.len(), 20, |_| at(172), |_| at(170)),
            LIT,
        );
        assert_eq!(score.verdict, Some(Verdict::Close), "two steps: 4.2°");

        // A 16-bit pan reads its fine channel: 1.4° (the same) and 2.1° (a little off) apart
        // with the same coarse value or the next.
        let channels = layout("head-rgbw-16", 1);
        let at = |coarse: u8, fine: u8| {
            let mut dmx = head(0.0, 0.0);
            (dmx[0], dmx[1]) = (coarse, fine);
            dmx
        };
        let judged = |a: Vec<u8>, b: Vec<u8>, verdict: Verdict, most: f64| {
            let score = judge(
                "Sixteen",
                &channels,
                1,
                &track(channels.len(), 20, |_| a.clone(), |_| b.clone()),
                LIT,
            );
            assert_eq!(score.verdict, Some(verdict));
            let by = part(&score, "Pan").most.unwrap();
            assert!((by - most).abs() < 0.001, "{by}");
        };
        judged(
            at(170, 170),
            at(170, 0),
            Verdict::Matches,
            170.0 * 540.0 / 65535.0,
        );
        judged(
            at(171, 170),
            at(170, 170),
            Verdict::Close,
            256.0 * 540.0 / 65535.0,
        );
    }

    #[test]
    fn a_color_wheel_and_red_green_and_blue_showing_the_same_color_are_the_same_light() {
        let slot = |value: u8, hex: &str| DmxSlot {
            value,
            color: Rgb::from_hex(hex),
            name: None,
        };
        let mut wheel = DmxChannel::new(DmxRole::ColorWheel);
        wheel.slots = vec![slot(0, "#ffffff"), slot(16, "#ff0000"), slot(32, "#00ff00")];
        let spot = vec![DmxChannel::new(DmxRole::Dimmer), wheel];
        let rgb = layout("dimmer-rgb", 1);
        // Red at about half: through the wheel's red slot, and through red with its dimmer full.
        let from_wheel = shown(&spot, 1, &[128, 20]);
        let from_rgb = shown(&rgb, 1, &[255, 128, 0, 0]);
        assert_eq!(from_wheel, [128, 0, 0]);
        assert_eq!(from_rgb, [128, 0, 0]);
        assert_eq!(light_off(&from_wheel, &[from_rgb.as_slice()], LIT).0, Off::Not);
        // Green on the wheel isn't.
        let green = shown(&spot, 1, &[128, 32]);
        assert_eq!(
            light_off(&green, &[from_rgb.as_slice()], LIT),
            (Off::Far, 128, Some(Why::Color))
        );

        // The same light from other channel values compares as the same: the wheel a few
        // values off within its slot, and the brightness in the dimmer or in the color.
        let track = track(2, 30, |_| vec![128, 18], |_| vec![128, 14]);
        assert_eq!(
            judge("Wheel", &spot, 1, &track, LIT).verdict,
            Some(Verdict::Matches)
        );
        let track = self::track(4, 30, |_| vec![128, 255, 0, 0], |_| vec![255, 128, 0, 0]);
        let score = judge("Flood", &rgb, 1, &track, LIT);
        assert_eq!(score.verdict, Some(Verdict::Matches));
        assert_eq!(score.kind, DmxKind::Flood);
        // Cyan, magenta, and yellow take color out of a lamp: all three out is dark.
        let cmy = vec![
            DmxChannel::new(DmxRole::Cyan),
            DmxChannel::new(DmxRole::Magenta),
            DmxChannel::new(DmxRole::Yellow),
        ];
        assert_eq!(shown(&cmy, 1, &[0, 255, 255]), [255, 0, 0]);
        let track = self::track(3, 30, |_| vec![0, 255, 255], |_| vec![0, 251, 252]);
        assert_eq!(
            judge("Lamp", &cmy, 1, &track, LIT).verdict,
            Some(Verdict::Matches)
        );
    }

    #[test]
    fn a_light_dimmer_is_close_and_another_color_or_dark_is_different() {
        let rgb = layout("dimmer-rgb", 1);
        let judged = |ours: [u8; 4], theirs: [u8; 4]| {
            let track = track(4, 20, |_| ours.to_vec(), |_| theirs.to_vec());
            let score = judge("Flood", &rgb, 1, &track, LIT);
            (score.verdict, score.differs)
        };
        // An eighth dimmer is the same light; half as bright is close; another color, or dark,
        // different.
        assert_eq!(
            judged([230, 255, 0, 0], [255, 255, 0, 0]).0,
            Some(Verdict::Matches)
        );
        assert_eq!(
            judged([128, 255, 0, 0], [255, 255, 0, 0]),
            (
                Some(Verdict::Close),
                Some("light dimmer in 100% of frames".to_string())
            )
        );
        assert_eq!(
            judged([255, 255, 60, 0], [255, 255, 0, 0]),
            (
                Some(Verdict::Close),
                Some("color a little off in 100% of frames".to_string())
            )
        );
        assert_eq!(
            judged([255, 0, 0, 255], [255, 255, 0, 0]),
            (
                Some(Verdict::Different),
                Some("color differs in 100% of frames".to_string())
            )
        );
        assert_eq!(
            judged([0, 255, 0, 0], [255, 255, 0, 0]),
            (
                Some(Verdict::Different),
                Some("dark where xLights lights it in 100% of frames".to_string())
            )
        );
        // Dark on both, however the channels hold it, is the same.
        assert_eq!(judged([0, 255, 0, 0], [255, 0, 0, 0]).0, Some(Verdict::Matches));
        // A color that is off for a third of the frames: different, and said so.
        let track = track(
            4,
            30,
            |f| {
                if f < 10 {
                    vec![255, 0, 255, 0]
                } else {
                    vec![255, 255, 0, 0]
                }
            },
            |_| vec![255, 255, 0, 0],
        );
        let score = judge("Flood", &rgb, 1, &track, LIT);
        assert_eq!(score.verdict, Some(Verdict::Different));
        assert_eq!(score.differs.as_deref(), Some("color differs in 33% of frames"));
        assert_eq!(score.score, Some(66.67));
    }

    #[test]
    fn a_dark_head_isnt_held_to_where_it_points_and_a_servo_always_is() {
        let channels = layout("head-rgbw-16", 1);
        let dark = |pan: f64| {
            let mut dmx = head(pan, 0.0);
            dmx[5] = 0;
            dmx
        };
        let score = judge(
            "Spot",
            &channels,
            1,
            &track(channels.len(), 20, |_| dark(90.0), |_| dark(-90.0)),
            LIT,
        );
        assert_eq!(score.verdict, Some(Verdict::Matches));
        assert!(score.parts.is_empty(), "nothing seen: {:?}", score.parts);
        // Lit on either side, it is.
        let score = judge(
            "Spot",
            &channels,
            1,
            &track(channels.len(), 20, |_| dark(90.0), |_| head(-90.0, 0.0)),
            LIT,
        );
        assert_eq!(score.verdict, Some(Verdict::Different));
        assert_eq!(
            score.differs.as_deref(),
            Some("pan off by up to 180° in 100% of frames; dark where xLights lights it in 100% of frames")
        );

        // Two 16-bit servos make no light, and are held to where they are on every frame.
        let servos = layout("servo-16", 2);
        let at = |a: f64, b: f64| {
            let mut dmx = vec![0u8; 4];
            for (axis, degrees) in axes(&servos).iter().zip([a, b]) {
                let (coarse, fine) = axis.values(degrees);
                dmx[axis.coarse] = coarse;
                dmx[axis.fine.unwrap()] = fine.unwrap();
            }
            dmx
        };
        let score = judge(
            "Arm",
            &servos,
            2,
            &track(4, 20, |_| at(10.0, -45.0), |_| at(10.0, -40.0)),
            LIT,
        );
        assert_eq!(score.kind, DmxKind::Servo);
        let names: Vec<&str> = score.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Servo 1", "Servo 2"]);
        assert_eq!(score.verdict, Some(Verdict::Close));
        assert_eq!(
            score.differs.as_deref(),
            Some("servo 2 off by up to 5° in 100% of frames")
        );
    }

    #[test]
    fn held_values_that_differ_are_close_and_a_shutter_is_held_to_what_it_does() {
        let mut channels = layout("head-spot-16", 1);
        let width = channels.len();
        let at = |gobo: u8, strobe: u8| {
            let mut dmx = vec![0u8; width];
            (dmx[0], dmx[2], dmx[5], dmx[6], dmx[8]) = (128, 128, 255, strobe, gobo);
            dmx
        };
        let judged = |channels: &[DmxChannel], ours: Vec<u8>, theirs: Vec<u8>| {
            let track = track(channels.len(), 20, |_| ours.clone(), |_| theirs.clone());
            let score = judge("Spot", channels, 1, &track, LIT);
            (score.verdict, score.differs)
        };
        assert_eq!(judged(&channels, at(64, 0), at(64, 0)).0, Some(Verdict::Matches));
        assert_eq!(
            judged(&channels, at(64, 0), at(32, 0)),
            (
                Some(Verdict::Close),
                Some("gobo differs in 100% of frames".to_string())
            )
        );
        // The usual shutter is open from 0 to 9: two values that open it are the same.
        assert_eq!(judged(&channels, at(64, 0), at(64, 5)).0, Some(Verdict::Matches));
        // A strobe at nearly the same pace is the same; open against strobing isn't.
        assert_eq!(
            judged(&channels, at(64, 250), at(64, 255)).0,
            Some(Verdict::Matches)
        );
        assert_eq!(
            judged(&channels, at(64, 0), at(64, 255)),
            (
                Some(Verdict::Close),
                Some("strobe differs in 100% of frames".to_string())
            )
        );
        assert_eq!(
            judged(&channels, at(32, 0), at(64, 255)),
            (
                Some(Verdict::Close),
                Some("strobe and gobo differ in 100% of frames".to_string())
            )
        );
        // A shutter closed on one side puts its light out there.
        channels[6].shutter = vec![
            pf_model::DmxShutterRange {
                from: 0,
                does: pf_model::DmxShutter::Closed,
            },
            pf_model::DmxShutterRange {
                from: 255,
                does: pf_model::DmxShutter::Open,
            },
        ];
        let (verdict, differs) = judged(&channels, at(64, 0), at(64, 255));
        assert_eq!(verdict, Some(Verdict::Different));
        assert_eq!(
            differs.as_deref(),
            Some("dark where xLights lights it in 100% of frames; strobe differs in 100% of frames")
        );
    }

    #[test]
    fn a_fixture_neither_render_has_isnt_compared() {
        let channels = layout("rgb", 1);
        let mut track = Track::new(3);
        track.push(None);
        let score = judge("Flood", &channels, 1, &track, LIT);
        assert_eq!((score.frames, score.verdict, score.score), (0, None, None));
        assert_eq!(score.verdict_with(0.03), None);
    }

    #[test]
    fn the_fixtures_as_a_whole_earn_the_worst_verdict_and_say_why() {
        let channels = layout("head-rgbw-16", 1);
        let judged = |name: &str, off: f64| {
            let track = track(channels.len(), 20, |_| head(off, 0.0), |_| head(0.0, 0.0));
            judge(name, &channels, 1, &track, LIT)
        };
        let unplaced = judge("Lost", &channels, 1, &Track::new(channels.len()), LIT);
        let fixtures = [
            judged("Left", 0.0),
            judged("Right", 5.0),
            judged("Up", 6.0),
            unplaced,
        ];
        let summary = FixtureSummary::of(&fixtures);
        assert_eq!((summary.judged, summary.of), (3, 4));
        assert_eq!((summary.fixtures.matches, summary.fixtures.close), (1, 2));
        assert_eq!(summary.verdict, Some(Verdict::Close));
        assert_eq!(
            summary.differs.as_deref(),
            Some("Right: pan off by up to 5° in 100% of frames; Up: pan off by up to 6° in 100% of frames")
        );
        let mut corpus = FixtureSummary::default();
        corpus.merge(&FixtureSummary::of(&fixtures[..1]), "First");
        assert_eq!(
            (corpus.verdict, corpus.differs.as_deref()),
            (Some(Verdict::Matches), None)
        );
        corpus.merge(&summary, "Second");
        assert_eq!((corpus.judged, corpus.of, corpus.fixtures.close), (4, 5, 2));
        assert_eq!(corpus.verdict, Some(Verdict::Close));
        assert!(corpus.differs.as_deref().unwrap().ends_with("(Second)"));

        // With a share of slack, a part off on few more frames than a match allows matches.
        let few = |frames: usize| {
            let track = track(
                channels.len(),
                100,
                |f| head(if f < frames { 40.0 } else { 0.0 }, 0.0),
                |_| head(0.0, 0.0),
            );
            judge("Spot", &channels, 1, &track, LIT)
        };
        assert_eq!(few(4).verdict, Some(Verdict::Close));
        assert_eq!(few(4).verdict_with(0.03), Some(Verdict::Matches));
        assert_eq!(few(27).verdict, Some(Verdict::Different));
        assert_eq!(few(27).verdict_with(0.03), Some(Verdict::Close));
    }
}
