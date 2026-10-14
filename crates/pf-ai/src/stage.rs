//! A made-up display and a made-up song, for tests, the guides' look sheets, and the evaluation
//! harness: a small house (a roofline, two windows, a matrix, a mega tree with a star, four
//! arches, a row of candy canes, and a singing face), groups of them, and a 96-second song at
//! 120 beats a minute with sections, a build, a drop, a breakdown, a stop, and sung lines in the
//! verses and every chorus.
//! Nothing here is anyone's real show, song, or lyric.

use pf_analysis::{
    Analysis, BarDrums, BarEnergy, Confidence, Drum, DrumHit, Event, EventKind, Level, Moment, MomentKind,
    Section,
};
use pf_engine::SequenceEdit;
use pf_model::{
    Corner, FaceDefinition, Generator, Group, GroupId, GroupMember, NodeRange, Phoneme, Prop, PropId, Region,
    ShapeSource, Show, TreeStyle, Vec3,
};
use pf_render::AudioSource;
use pf_sequence::{Effect, Mark, Row, RowId, Sequence, Target, TimingKind, TimingTrack};
use std::sync::{Arc, OnceLock};

/// A beat, and a bar, of the stage's song (120 beats a minute, four to the bar).
pub const BEAT_MS: u64 = 500;
pub const BAR_MS: u64 = 2_000;
/// How long the stage's song is.
pub const SONG_MS: u64 = 96_000;
/// The name the song's file goes by (no such file exists: an analyzer answers for it).
pub const SONG_FILE: &str = "paper-lanterns.mp3";

pub use crate::knowledge::Suit;

/// The stage: the show, and where its parts are.
#[derive(Debug, Clone)]
pub struct Stage {
    pub show: Show,
    pub roofline: PropId,
    pub matrix: PropId,
    pub tree: PropId,
    pub arches: Vec<PropId>,
    pub face: PropId,
    pub whole_house: GroupId,
    pub arches_group: GroupId,
    /// The stage's sequence, made once: every copy has the same rows and timing tracks, by id,
    /// so an effect that names a track finds it in whichever copy it's drawn on.
    doc: Sequence,
}

fn at(mut prop: Prop, x: f32, y: f32) -> Prop {
    prop.transform.position = Vec3::new(x, y, 0.0);
    prop
}

fn generated(name: &str, shape: Generator) -> Prop {
    Prop::new(name, ShapeSource::Generator(shape))
}

/// A face on a matrix of `columns` by `rows`: an outline round the edge, two eyes, and a mouth
/// for every mouth shape, each a block of pixels as wide and tall as that shape is open.
fn face(prop: &Prop, columns: u32, rows: u32) -> FaceDefinition {
    // The node at each column and row, whatever the wiring: found by where it sits.
    let positions = pf_geometry::world_positions(prop);
    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &positions {
        min = [min[0].min(p.x), min[1].min(p.y)];
        max = [max[0].max(p.x), max[1].max(p.y)];
    }
    let cell = |v: f32, lo: f32, hi: f32, cells: u32| {
        (((v - lo) / (hi - lo).max(f32::EPSILON)) * (cells - 1) as f32).round() as u32
    };
    let node_at = |column: u32, row: u32| {
        positions
            .iter()
            .position(|p| {
                cell(p.x, min[0], max[0], columns) == column && cell(p.y, min[1], max[1], rows) == row
            })
            .map(|i| NodeRange::new(i as u32, i as u32 + 1))
    };
    let block = |across: std::ops::RangeInclusive<u32>, up: std::ops::RangeInclusive<u32>| {
        let mut nodes = Vec::new();
        for row in up {
            for column in across.clone() {
                nodes.extend(node_at(column, row));
            }
        }
        nodes
    };
    // Wider matrices get wider features.
    let wide = (columns / 16).max(1);
    let mid = columns / 2;
    let mut definition = FaceDefinition::default();
    for column in 0..columns {
        definition.outline.extend(node_at(column, 0));
        definition.outline.extend(node_at(column, rows - 1));
    }
    for row in 1..rows - 1 {
        definition.outline.extend(node_at(0, row));
        definition.outline.extend(node_at(columns - 1, row));
    }
    let eyes = |up: std::ops::RangeInclusive<u32>| {
        [
            block(mid - 4 * wide..=mid - 2 * wide - 1, up.clone()),
            block(mid + 2 * wide..=mid + 4 * wide - 1, up),
        ]
        .concat()
    };
    definition.eyes_open = eyes(rows - 6..=rows - 4);
    definition.eyes_closed = eyes(rows - 5..=rows - 5);
    // Each mouth: half its width, and its height, in pixels.
    let mouths = [
        (Phoneme::Ai, 3, 4),
        (Phoneme::E, 4, 2),
        (Phoneme::Etc, 3, 2),
        (Phoneme::Fv, 3, 1),
        (Phoneme::L, 2, 3),
        (Phoneme::Mbp, 3, 1),
        (Phoneme::O, 2, 4),
        (Phoneme::Rest, 2, 1),
        (Phoneme::U, 1, 3),
        (Phoneme::Wq, 1, 2),
    ];
    for (phoneme, half, height) in mouths {
        definition.mouths.insert(
            phoneme,
            block(mid - half * wide..=mid + half * wide - 1, 3..=2 + height),
        );
    }
    definition
}

impl Stage {
    pub fn new() -> Self {
        let roofline = at(
            generated(
                "Roofline",
                Generator::Line {
                    nodes: 60,
                    length: 9.0,
                },
            ),
            0.0,
            4.2,
        );
        let window = |name: &str, x: f32| {
            at(
                generated(
                    name,
                    Generator::WindowFrame {
                        top: 12,
                        sides: 16,
                        bottom: 12,
                        width: 1.0,
                        height: 1.4,
                        start: Corner::BottomLeft,
                        counter_clockwise: false,
                    },
                ),
                x,
                2.4,
            )
        };
        let mut matrix = at(
            generated(
                "Matrix",
                Generator::Matrix {
                    columns: 40,
                    rows: 20,
                    width: 2.4,
                    height: 1.2,
                    wiring: Default::default(),
                },
            ),
            0.0,
            2.3,
        );
        let definition = face(&matrix, 40, 20);
        matrix.regions.push(Region::face("Face", definition));
        let tree = at(
            generated(
                "Mega Tree",
                Generator::tree(16, 30, 3.2, 1.1, 0.05, TreeStyle::Round),
            ),
            -6.5,
            0.0,
        );
        let star = at(
            generated("Tree Star", Generator::star(5, 50, 0.45, 0.2)),
            -6.5,
            3.75,
        );
        let arches: Vec<Prop> = (0..4)
            .map(|i| {
                at(
                    generated(&format!("Arch {}", i + 1), Generator::arch(30, 1.5, 0.75)),
                    -3.0 + 2.0 * i as f32,
                    0.0,
                )
            })
            .collect();
        let canes = at(
            generated(
                "Candy Canes",
                Generator::CandyCanes {
                    canes: 4,
                    nodes_per_cane: 18,
                    width: 2.4,
                    height: 1.0,
                    cane_height: 1.2,
                    reverse: false,
                    sticks: false,
                    alternate_nodes: false,
                    skew_deg: 0.0,
                    start_right: false,
                },
            ),
            6.4,
            0.0,
        );
        let mut singer = at(
            generated(
                "Singing Face",
                Generator::Matrix {
                    columns: 16,
                    rows: 16,
                    width: 1.4,
                    height: 1.4,
                    wiring: Default::default(),
                },
            ),
            6.4,
            2.8,
        );
        let definition = face(&singer, 16, 16);
        singer.regions.push(Region::face("Face", definition));

        let mut show = Show::new("Stage");
        let (roof_id, matrix_id, tree_id, face_id) = (roofline.id, matrix.id, tree.id, singer.id);
        let arch_ids: Vec<PropId> = arches.iter().map(|a| a.id).collect();
        let windows = [window("Window Left", -2.7), window("Window Right", 2.7)];
        show.props.push(roofline);
        show.props.extend(windows);
        show.props.push(matrix);
        show.props.push(tree);
        show.props.push(star);
        show.props.extend(arches);
        show.props.push(canes);
        show.props.push(singer);

        let mut whole_house = Group::new("Whole House");
        whole_house.members = show.props.iter().map(|p| GroupMember::Prop(p.id)).collect();
        let mut arches_group = Group::new("Arches");
        arches_group.members = arch_ids.iter().map(|id| GroupMember::Prop(*id)).collect();
        let mut windows_group = Group::new("Windows");
        windows_group.members = show
            .props
            .iter()
            .filter(|p| p.name.starts_with("Window"))
            .map(|p| GroupMember::Prop(p.id))
            .collect();
        let (house_id, arches_id) = (whole_house.id, arches_group.id);
        show.groups = vec![whole_house, arches_group, windows_group];
        let doc = empty_sequence(&show);
        Self {
            show,
            roofline: roof_id,
            matrix: matrix_id,
            tree: tree_id,
            arches: arch_ids,
            face: face_id,
            whole_house: house_id,
            arches_group: arches_id,
            doc,
        }
    }

    /// What a guide's look is tried on, for each kind of target.
    pub fn target(&self, suit: Suit) -> Target {
        match suit {
            Suit::String => Target::Prop(self.roofline),
            Suit::Arch => Target::Prop(self.arches[1]),
            Suit::Matrix => Target::Prop(self.matrix),
            Suit::Tree => Target::Prop(self.tree),
            Suit::House => Target::Group(self.whole_house),
        }
    }

    /// The props a target lights.
    pub fn props_of(&self, target: Target) -> Vec<PropId> {
        match target {
            Target::Prop(id) | Target::Region { prop: id, .. } => vec![id],
            Target::Group(id) => self
                .show
                .groups
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.members.iter().map(|m| m.prop()).collect())
                .unwrap_or_default(),
        }
    }

    /// The stage's sequence with nothing on it yet: a row for each group, then one for each prop
    /// (so a prop's own row draws over its groups'), with the song's Beats and Bars tracks and its
    /// sung lines, words, syllables, and mouth shapes. Every copy is the same, ids and all.
    pub fn sequence(&self) -> Sequence {
        self.doc.clone()
    }

    /// The row of `doc` that lights `target`.
    pub fn row(&self, doc: &Sequence, target: Target) -> Option<RowId> {
        doc.rows.iter().find(|r| r.target == target).map(|r| r.id)
    }

    /// The stage's sequence with `layers` (bottom first) playing on the row that lights `suit`'s
    /// target and nothing else: what a guide's look is drawn from.
    pub fn with_effects(&self, suit: Suit, layers: &[Effect]) -> Sequence {
        self.with_effects_on(self.target(suit), layers)
    }

    /// [`Stage::with_effects`] on the row that lights `target`.
    pub fn with_effects_on(&self, target: Target, layers: &[Effect]) -> Sequence {
        let mut doc = self.sequence();
        if let Some(row) = doc.rows.iter_mut().find(|r| r.target == target) {
            row.layers = layers
                .iter()
                .map(|effect| pf_sequence::Layer {
                    effects: vec![effect.clone()],
                })
                .collect();
        }
        doc
    }

    /// The prop or group called `name`, as a row's target.
    pub fn named(&self, name: &str) -> Option<Target> {
        let group = self.show.groups.iter().find(|g| g.name == name);
        let prop = self.show.props.iter().find(|p| p.name == name);
        group
            .map(|g| Target::Group(g.id))
            .or(prop.map(|p| Target::Prop(p.id)))
    }

    /// Where each pixel of `suit`'s target sits seen from the front (x right, y up), and where
    /// its color is in a frame.
    pub fn pixels(&self, suit: Suit) -> Vec<Pixel> {
        let props = self.props_of(self.target(suit));
        pf_engine::preview_props_of(&self.show)
            .into_iter()
            .filter(|p| props.contains(&p.prop))
            .flat_map(|p| {
                let step = usize::from(p.channels_per_pixel.max(1));
                p.points
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .enumerate()
                    .map(|(i, &[x, y])| Pixel {
                        x,
                        y,
                        offset: p.frame_offset + i * step,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The frames of `doc` at `times_ms`, with the stage's song for the effects that follow it
    /// and `pictures` for the ones that draw them.
    pub fn frames(&self, doc: &Sequence, times_ms: &[u64], pictures: &pf_render::Pictures) -> Vec<Vec<u8>> {
        let mut renderer = pf_engine::DraftRenderer::new(&self.show);
        renderer.set_audio(audio());
        renderer.set_pictures(pictures.waiting());
        times_ms.iter().map(|&t| renderer.frame(doc, t)).collect()
    }
}

/// An effect id made from `name`, the same every time: effects that scatter at random are
/// seeded by their id, so one given this id draws the same way on every run.
pub fn steady_id(name: &str) -> pf_sequence::EffectId {
    // FNV-1a, twice over for 128 bits.
    let hash = |seed: u64| {
        name.bytes().fold(seed, |h, byte| {
            (h ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
        })
    };
    let bits = (u128::from(hash(0xCBF2_9CE4_8422_2325)) << 64) | u128::from(hash(0x8422_2325_CBF2_9CE4));
    pf_sequence::EffectId(uuid::Uuid::from_u128(bits))
}

/// The stage's one picture, as a Picture effect's `file` names it.
pub const PICTURE: &str = "images/snowman.png";

/// A PNG of `width` by `height` from its pixels (`rgb`, rows from the top), stored uncompressed:
/// all the look sheets and the stage's picture need, with nothing to depend on.
pub fn png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let sum = crc(&out[start..]);
        out.extend(sum.to_be_bytes());
    }
    let row = width as usize * 3;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgb.chunks(row.max(1)).take(height as usize) {
        raw.push(0);
        raw.extend(line);
    }
    // zlib, in stored blocks.
    let mut data = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        data.push(u8::from(i + 1 == blocks.len()));
        data.extend((block.len() as u16).to_le_bytes());
        data.extend((!(block.len() as u16)).to_le_bytes());
        data.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    data.extend(((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::new();
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &data);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The stage's picture, as a PNG file's bytes: a snowman drawn here, pixel by pixel (white body,
/// orange nose, a red scarf, black around it), 24 pixels square.
pub fn snowman() -> Vec<u8> {
    const SIDE: i32 = 24;
    let mut rgb = vec![0u8; (SIDE * SIDE * 3) as usize];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let inside = |cx: i32, cy: i32, r: i32| (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
            let color = if (9..=14).contains(&x) && (10..=11).contains(&y) {
                Some([220, 30, 30])
            } else if (12..=15).contains(&x) && y == 6 {
                Some([255, 140, 0])
            } else if (inside(10, 5, 0) || inside(13, 5, 0)) && y == 5 {
                Some([20, 20, 20])
            } else if inside(12, 6, 4) || inside(12, 16, 7) {
                Some([255, 255, 255])
            } else {
                None
            };
            if let Some(color) = color {
                let at = ((y * SIDE + x) * 3) as usize;
                rgb[at..at + 3].copy_from_slice(&color);
            }
        }
    }
    png(SIDE as u32, SIDE as u32, &rgb)
}

/// The stage's pictures, as the renderer reads them: [`PICTURE`], and nothing else.
pub fn pictures() -> pf_render::Pictures {
    pf_render::Pictures::new(|file| {
        if file == PICTURE {
            Ok(snowman())
        } else {
            Err(format!("the stage has no picture \"{file}\""))
        }
    })
}

/// One pixel of the stage: where it sits in the front view, and where its red byte is in a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pixel {
    pub x: f32,
    pub y: f32,
    pub offset: usize,
}

/// A new sequence for the stage's song on `show` (see [`Stage::sequence`]).
fn empty_sequence(show: &Show) -> Sequence {
    let mut doc = Sequence::new("Paper Lanterns", SONG_MS);
    doc.audio = Some(SONG_FILE.to_string());
    doc.rows = show
        .groups
        .iter()
        .map(|g| Row::new(Target::Group(g.id)))
        .chain(show.props.iter().map(|p| Row::new(Target::Prop(p.id))))
        .collect();
    let analysis = song();
    doc.timing_tracks = analysis.timing_tracks().into_iter().take(2).collect();
    doc.timing_tracks.extend(lyric_tracks());
    let made = crate::lyrics::tracks::from_words(&doc.timing_tracks, &analysis.onsets, SONG_MS);
    for edit in made.into_iter().flatten() {
        if let SequenceEdit::AddTimingTrack { track } = edit {
            doc.timing_tracks.push(track);
        }
    }
    doc
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

/// The song's parts: label, group, first bar, bars, and energy.
const PARTS: [(&str, &str, u64, u64, f32); 8] = [
    ("Intro", "A", 0, 4, 0.25),
    ("Verse", "B", 4, 8, 0.45),
    ("Chorus", "C", 12, 8, 0.9),
    ("Verse", "B", 20, 8, 0.5),
    ("Chorus", "C", 28, 8, 0.9),
    ("Bridge", "D", 36, 4, 0.2),
    ("Chorus", "C", 40, 6, 1.0),
    ("Outro", "E", 46, 2, 0.3),
];

/// The sung lines: the bar each starts on, and its words (made up for the stage). Every chorus
/// is sung, the last one too.
const LINES: [(u64, &str); 10] = [
    (4, "Paper lanterns in the snow"),
    (8, "Every window starts to glow"),
    (12, "Light it up, light it up"),
    (16, "Lanterns high above the town"),
    (20, "Footprints fading down the lane"),
    (24, "Frost is singing on the pane"),
    (28, "Light it up, light it up"),
    (32, "Lanterns high above the town"),
    (40, "Light it up, light it up"),
    (44, "Lanterns high above the town"),
];

fn moment(
    time_ms: u64,
    end_ms: Option<u64>,
    kind: MomentKind,
    importance: f32,
    label: Option<&str>,
) -> Moment {
    Moment {
        time_ms,
        end_ms,
        kind,
        strength: importance,
        importance,
        label: label.map(str::to_string),
        suggest: kind.suggest(),
    }
}

/// The stage's song, as PixelFlow's analysis would describe it: tempo, beats and bars, sections
/// that repeat, each bar's energy and drums, and its moments (a build into the first chorus, a
/// drop on it, hits, a breakdown, a stop before the last chorus, the restart, the climax, and a
/// held last chord).
pub fn song() -> Analysis {
    let beats: Vec<u64> = (0..SONG_MS / BEAT_MS).map(|i| i * BEAT_MS).collect();
    let bars: Vec<u64> = beats.iter().copied().step_by(4).collect();
    let part_of = |bar: u64| {
        PARTS
            .iter()
            .find(|p| (p.2..p.2 + p.3).contains(&bar))
            .copied()
            .unwrap_or(PARTS[0])
    };
    let bar_energy: Vec<BarEnergy> = (0..bars.len() as u64)
        .map(|bar| {
            let e = part_of(bar).4;
            BarEnergy {
                overall: e,
                low: (e - 0.1).max(0.0),
                mid: e,
                high: (e + 0.05).min(1.0),
            }
        })
        .collect();
    let bar_drums = (0..bars.len() as u64)
        .map(|bar| {
            let part = part_of(bar);
            let n = if part.4 >= 0.8 {
                4
            } else if part.4 >= 0.4 {
                2
            } else {
                0
            };
            BarDrums {
                kick: n,
                snare: n / 2,
                hat: 2 * n,
                crash: u16::from(part.2 == bar && part.4 >= 0.8),
            }
        })
        .collect();
    let energy: Vec<f32> = (0..SONG_MS / 1000)
        .map(|s| {
            // The band cuts out for the last second of the bridge.
            if s == 79 {
                0.02
            } else {
                part_of(s * 1000 / BAR_MS).4
            }
        })
        .collect();
    let sections = PARTS
        .iter()
        .map(|&(label, group, bar, length, energy)| Section {
            start_ms: bar * BAR_MS,
            end_ms: (bar + length) * BAR_MS,
            energy,
            level: if energy < 0.4 {
                Level::Low
            } else if energy < 0.7 {
                Level::Medium
            } else {
                Level::High
            },
            label: label.to_string(),
            group: group.to_string(),
            confidence: 0.8,
        })
        .collect();
    let event = |time_ms, kind, strength, duration_ms| Event {
        time_ms,
        kind,
        strength,
        duration_ms,
    };
    let drum = |time_ms, drum, strength| DrumHit {
        time_ms,
        drum,
        strength,
    };
    Analysis {
        duration_ms: SONG_MS,
        tempo_bpm: Some(120.0),
        bars,
        onsets: beats.clone(),
        beats,
        energy,
        sections,
        events: vec![
            event(20_000, EventKind::Build, 0.7, Some(4_000)),
            event(24_000, EventKind::Drop, 1.0, None),
            event(56_000, EventKind::Drop, 0.9, None),
            event(72_000, EventKind::Break, 0.6, Some(8_000)),
            event(80_000, EventKind::Hit, 1.0, None),
        ],
        bar_energy,
        moments: vec![
            moment(20_000, Some(24_000), MomentKind::Build, 0.7, None),
            moment(24_000, None, MomentKind::Drop, 0.95, None),
            moment(32_000, None, MomentKind::Impact, 0.6, None),
            moment(54_000, Some(56_000), MomentKind::Fill, 0.45, None),
            moment(56_000, None, MomentKind::Impact, 0.85, None),
            moment(72_000, Some(79_000), MomentKind::Breakdown, 0.6, None),
            moment(79_000, Some(80_000), MomentKind::Stop, 0.8, Some("full")),
            moment(80_000, None, MomentKind::Restart, 1.0, None),
            moment(80_000, Some(92_000), MomentKind::Peak, 0.75, Some("climax")),
            moment(92_000, Some(96_000), MomentKind::Hold, 0.5, None),
        ],
        drums: vec![
            drum(24_000, Drum::Crash, 1.0),
            drum(32_000, Drum::Snare, 0.8),
            drum(56_000, Drum::Crash, 0.9),
            drum(80_000, Drum::Crash, 1.0),
        ],
        bar_drums,
        confidence: Confidence {
            tempo: 0.9,
            downbeat: 0.8,
            sections: 0.8,
        },
        ..Analysis::default()
    }
}

/// The song's sung lines and words as timing tracks ("Lyrics" and "Lyrics (words)"): each line
/// over a bar and three quarters, its words sharing that time evenly.
pub fn lyric_tracks() -> Vec<TimingTrack> {
    let mut lines = Vec::new();
    let mut words = Vec::new();
    for (bar, text) in LINES {
        let (start, length) = (bar * BAR_MS, 3_500);
        lines.push(Mark::new(start, start + length, text));
        let sung: Vec<&str> = text.split_whitespace().collect();
        let each = length / sung.len() as u64;
        for (i, word) in sung.iter().enumerate() {
            let from = start + i as u64 * each;
            words.push(Mark::new(from, from + each - 60, word.trim_matches(',')));
        }
    }
    vec![
        TimingTrack::new(crate::lyrics::tracks::LYRICS_TRACK, TimingKind::Lyrics, lines),
        TimingTrack::new(crate::lyrics::tracks::WORDS_TRACK, TimingKind::Words, words),
    ]
}

/// How many samples a second the song's sound has.
const SOUND_RATE: u32 = 11_025;

/// The song as sound: a low thump on every beat and a steady tone, as loud as each part's energy,
/// silent through the stop. Mono, at [`SOUND_RATE`].
fn sound() -> impl Iterator<Item = f32> {
    let total = SONG_MS * u64::from(SOUND_RATE) / 1000;
    (0..total).map(|n| {
        let ms = n * 1000 / u64::from(SOUND_RATE);
        if (79_000..80_000).contains(&ms) {
            return 0.0;
        }
        let energy = PARTS
            .iter()
            .find(|p| (p.2 * BAR_MS..(p.2 + p.3) * BAR_MS).contains(&ms))
            .map_or(0.2, |p| p.4);
        let t = n as f32 / SOUND_RATE as f32;
        let since_beat = (ms % BEAT_MS) as f32 / 1000.0;
        let thump = (std::f32::consts::TAU * 60.0 * t).sin() * (-since_beat * 18.0).exp();
        let tone = (std::f32::consts::TAU * 440.0 * t).sin() * 0.35
            + (std::f32::consts::TAU * 1_760.0 * t).sin() * 0.15;
        energy * (0.7 * thump + tone)
    })
}

/// The song as sound, for the effects that follow the music (see [`sound`]). Made once and kept.
pub fn audio() -> AudioSource {
    static TRACK: OnceLock<Arc<pf_analysis::AudioTrack>> = OnceLock::new();
    let track = TRACK.get_or_init(|| Arc::new(pf_analysis::audio_track(sound(), SOUND_RATE, 25)));
    AudioSource::ready(Arc::clone(track))
}

/// The song's sound as a WAV file's bytes (16-bit, mono): what [`audio`] is worked out from, for
/// a copy of the stage opened in the app or drawn as a video, so the effects that follow the
/// music there hear what they hear here.
pub fn song_wav() -> Vec<u8> {
    let samples: Vec<i16> = sound()
        .map(|s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)
        .collect();
    let data = samples.len() as u32 * 2;
    let mut wav = Vec::with_capacity(44 + data as usize);
    wav.extend(b"RIFF");
    wav.extend((36 + data).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    // PCM, one channel, the rate, bytes a second, bytes a sample, bits a sample.
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(SOUND_RATE.to_le_bytes());
    wav.extend((SOUND_RATE * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data.to_le_bytes());
    for sample in samples {
        wav.extend(sample.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_is_a_valid_show_with_a_row_for_everything() {
        let stage = Stage::new();
        let report = pf_model::validate_show(&stage.show);
        assert!(
            report
                .issues
                .iter()
                .all(|i| i.severity != pf_model::Severity::Error),
            "{:?}",
            report.issues
        );
        let doc = stage.sequence();
        assert!(pf_sequence::limit_problems(&doc).is_empty());
        for suit in Suit::ALL {
            assert!(stage.row(&doc, stage.target(suit)).is_some(), "{suit:?}");
        }
        let names: Vec<&str> = doc.timing_tracks.iter().map(|t| t.name.as_str()).collect();
        for track in [
            "Beats",
            "Bars",
            "Lyrics",
            "Lyrics (words)",
            "Lyrics (syllables)",
            "Lyrics (phonemes)",
        ] {
            assert!(names.contains(&track), "{track} in {names:?}");
        }
    }

    #[test]
    fn an_effect_that_names_a_track_is_drawn_following_it() {
        // Every copy of the stage's sequence has the same tracks, so a track found in one copy
        // is there in the copy the effect is drawn on.
        let stage = Stage::new();
        assert_eq!(stage.sequence(), stage.sequence());
        let doc = stage.sequence();
        let beats = doc.timing_tracks.iter().find(|t| t.name == "Beats").unwrap().id;
        let mut on = Effect::new(pf_sequence::EffectKind::On, 24_000, 28_000);
        on.curves.insert(
            "brightness".into(),
            pf_sequence::Curve {
                fade: 4.0,
                timing_track: Some(beats),
                ..pf_sequence::Curve::shaped(pf_sequence::CurveShape::TimingFade, 0.0, 1.0, 1.0)
            },
        );
        let drawn = stage.with_effects(Suit::String, &[on]);
        let pixel = stage.pixels(Suit::String)[0];
        // On the beat it is bright; 300 ms later the four-frame fade has run out.
        let frames = stage.frames(&drawn, &[24_000, 24_300], &pf_render::Pictures::none());
        assert!(frames[0][pixel.offset] > 200, "{}", frames[0][pixel.offset]);
        assert_eq!(frames[1][pixel.offset], 0);
    }

    #[test]
    fn the_stages_faces_have_pixels_and_sing_the_words() {
        let stage = Stage::new();
        for name in ["Matrix", "Singing Face"] {
            let prop = stage.show.props.iter().find(|p| p.name == name).unwrap();
            let face = prop
                .regions
                .iter()
                .find_map(|r| match &r.kind {
                    pf_model::RegionKind::Face(face) => Some(face),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name} has a face"));
            assert!(
                face.ranges()
                    .all(|r| r.len() == 1 && r.fits_within(prop.node_count()))
            );
            assert!(face.outline.len() > 40 && face.eyes_open.len() >= 12, "{name}");
            assert_eq!(face.mouths.len(), Phoneme::ALL.len());
        }
        // A Faces effect on the mouth shapes of the sung words lights the matrix's face while
        // a word is sung (the first chorus's first word starts at 24 s).
        let doc = stage.sequence();
        let phonemes = doc
            .timing_tracks
            .iter()
            .find(|t| t.kind == TimingKind::Phonemes)
            .expect("the stage has mouth shapes");
        let mut faces = Effect::new(pf_sequence::EffectKind::Faces, 24_000, 28_000);
        let pf_sequence::EffectParams::Faces(params) = &mut faces.params else {
            panic!()
        };
        params.timing_track = Some(phonemes.id);
        let drawn = stage.with_effects(Suit::Matrix, &[faces]);
        let frame = &stage.frames(&drawn, &[24_200], &pf_render::Pictures::none())[0];
        let lit = stage
            .pixels(Suit::Matrix)
            .iter()
            .filter(|p| frame[p.offset..p.offset + 3].iter().any(|v| *v > 12))
            .count();
        assert!(lit > 20, "{lit} pixels of the face are lit");
    }

    #[test]
    fn every_chorus_is_sung_and_every_line_has_its_words() {
        // A request about "each chorus" has words for each one to work with, the last too.
        let song = song();
        let tracks = lyric_tracks();
        let (lines, words) = (&tracks[0], &tracks[1]);
        let choruses: Vec<_> = song.sections.iter().filter(|s| s.label == "Chorus").collect();
        assert_eq!(choruses.len(), 3);
        for chorus in choruses {
            let sung = lines
                .marks
                .iter()
                .filter(|m| m.start_ms >= chorus.start_ms && m.end_ms <= chorus.end_ms)
                .count();
            assert_eq!(sung, 2, "the chorus from {} ms", chorus.start_ms);
        }
        for line in &lines.marks {
            let said: Vec<&str> = words
                .marks
                .iter()
                .filter(|w| w.start_ms >= line.start_ms && w.end_ms <= line.end_ms)
                .map(|w| w.label.as_str())
                .collect();
            assert_eq!(said.join(" "), line.label.replace(',', ""));
        }
    }

    #[test]
    fn the_song_has_a_bar_of_energy_and_drums_for_every_bar() {
        let song = song();
        assert_eq!(song.bars.len(), 48);
        assert_eq!(song.bar_energy.len(), song.bars.len());
        assert_eq!(song.bar_drums.len(), song.bars.len());
        assert_eq!(song.sections.last().map(|s| s.end_ms), Some(SONG_MS));
        assert!(
            song.moments.windows(2).all(|w| w[0].time_ms <= w[1].time_ms),
            "moments are in time order"
        );
    }
}
