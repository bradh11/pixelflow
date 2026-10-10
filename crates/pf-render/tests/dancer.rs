//! The Dancer effect: its characters on a pillar and on other grids, its moves, the beat it
//! follows, and that it draws the same picture every time.

use pf_model::{Corner, Generator, MatrixWiring, Orientation, Prop, ShapeSource, Show, Transform, Vec3};
use pf_render::audio::{AudioSource, AudioTrack};
use pf_render::{Audio, Canvas, Colors, EffectTime, Pixel, RenderContext, Renderer, Rgba, Shade, Shader};
use pf_sequence::*;
use std::f32::consts::TAU;
use std::sync::Arc;

const FRAME_MS: u32 = 25;

/// A `columns` × `rows` grid of cells, bottom row first.
fn grid(columns: u32, rows: u32) -> (Canvas, Vec<Pixel>) {
    let at = |i: u32, n: u32| if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
    let pixels = (0..rows)
        .flat_map(|y| {
            (0..columns).map(move |x| Pixel {
                u: at(x, columns),
                v: at(y, rows),
                index: y * columns + x,
                count: columns * rows,
            })
        })
        .collect();
    (Canvas { columns, rows }, pixels)
}

/// A dancer drawn on a grid: each cell's color, bottom row first.
#[derive(Debug, Clone, PartialEq)]
struct Picture {
    columns: usize,
    rows: usize,
    cells: Vec<Rgba>,
}

impl Picture {
    fn at(&self, x: usize, y: usize) -> Rgba {
        self.cells[y * self.columns + x]
    }

    /// How bright a cell shows over black, 0–1.
    fn level(&self, x: usize, y: usize) -> f32 {
        let c = self.at(x, y);
        c.r.max(c.g).max(c.b) * c.a.clamp(0.0, 1.0)
    }

    fn lit(&self, x: usize, y: usize) -> bool {
        self.level(x, y) > 0.2
    }

    /// Drawn black (an eye, a belt): covered, but dark.
    fn dark(&self, x: usize, y: usize) -> bool {
        self.at(x, y).a > 0.9 && self.level(x, y) < 0.05
    }

    fn all(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.rows).flat_map(|y| (0..self.columns).map(move |x| (x, y)))
    }

    fn lit_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.all().filter(|&(x, y)| self.lit(x, y))
    }

    fn lit_count(&self) -> usize {
        self.lit_cells().count()
    }

    fn dark_count(&self) -> usize {
        self.all().filter(|&(x, y)| self.dark(x, y)).count()
    }

    /// The lowest and highest rows with something lit.
    fn span(&self) -> Option<(usize, usize)> {
        let rows: Vec<usize> = self.lit_cells().map(|(_, y)| y).collect();
        Some((*rows.iter().min()?, *rows.iter().max()?))
    }

    /// The columns `from..to` as their own picture.
    fn columns(&self, from: usize, to: usize) -> Picture {
        Picture {
            columns: to - from,
            rows: self.rows,
            cells: self
                .all()
                .filter(|(x, _)| (from..to).contains(x))
                .map(|(x, y)| self.at(x, y))
                .collect(),
        }
    }

    fn flipped(&self) -> Picture {
        Picture {
            columns: self.columns,
            rows: self.rows,
            cells: self
                .all()
                .map(|(x, y)| self.at(self.columns - 1 - x, y))
                .collect(),
        }
    }

    /// The picture as text, top row first, for a failed check's message.
    fn text(&self) -> String {
        let mut out = String::from("\n");
        for y in (0..self.rows).rev() {
            out.push('|');
            for x in 0..self.columns {
                out.push(match self.level(x, y) {
                    l if l > 0.75 => '#',
                    l if l > 0.2 => '+',
                    _ if self.dark(x, y) => 'o',
                    _ => ' ',
                });
            }
            out.push_str("|\n");
        }
        out
    }
}

/// `p` drawn at `t_ms` into a minute-long effect, following `cx`.
fn dance(p: DancerParams, columns: u32, rows: u32, t_ms: u64, cx: &RenderContext) -> Picture {
    let (canvas, pixels) = grid(columns, rows);
    let time = EffectTime::within(0, 60_000, t_ms).with_frame_ms(FRAME_MS);
    let shader = Shader::in_context(
        &EffectParams::Dancer(p),
        &time,
        Colors::new(&[Rgb::RED, Rgb::GREEN, Rgb::BLUE]),
        7,
        canvas,
        cx,
    );
    Picture {
        columns: columns as usize,
        rows: rows as usize,
        cells: pixels.iter().map(|px| shader.shade(px)).collect(),
    }
}

/// `p` on a 12 × 50 pillar, at the steady beat (two a second).
fn pillar(p: DancerParams, t_ms: u64) -> Picture {
    dance(p, 12, 50, t_ms, &RenderContext::default())
}

fn of(character: DancerCharacter) -> DancerParams {
    DancerParams {
        character,
        ..DancerParams::default()
    }
}

const CHARACTERS: [DancerCharacter; 6] = [
    DancerCharacter::Skeleton,
    DancerCharacter::Ghost,
    DancerCharacter::Witch,
    DancerCharacter::Santa,
    DancerCharacter::Snowman,
    DancerCharacter::Elf,
];

const MOVES: [DancerMove; 8] = [
    DancerMove::Mix,
    DancerMove::Bounce,
    DancerMove::ArmWave,
    DancerMove::Kick,
    DancerMove::Twist,
    DancerMove::Shuffle,
    DancerMove::Jump,
    DancerMove::HeadBob,
];

// ---------------------------------------------------------------------------------------------
// The characters

#[test]
fn every_character_stands_on_a_pillar_head_up_and_all_there() {
    // How many of the pillar's 600 pixels each lights, loosely: a thin skeleton, fuller others.
    let lit = [
        (DancerCharacter::Skeleton, 90..=230),
        (DancerCharacter::Ghost, 130..=330),
        (DancerCharacter::Witch, 170..=400),
        (DancerCharacter::Santa, 170..=420),
        (DancerCharacter::Snowman, 140..=330),
        (DancerCharacter::Elf, 130..=330),
    ];
    for (character, range) in lit {
        // Four bars of the mix, a frame every eighth of a beat.
        for t in (0..8_000).step_by(62) {
            let picture = pillar(of(character), t);
            let count = picture.lit_count();
            assert!(
                range.contains(&count),
                "{character:?} at {t}: {count}{}",
                picture.text()
            );
            let (low, high) = picture.span().unwrap();
            assert!(high - low >= 18, "{character:?} at {t}{}", picture.text());
            // Its eyes are in its head, and its head is at the top.
            let eyes = picture.all().filter(|&(x, y)| picture.dark(x, y));
            let top = eyes.map(|(_, y)| y).max().expect("eyes");
            assert!(
                top > (low + high) / 2 && top < high,
                "{character:?} at {t}: eyes at {top} of {low}–{high}{}",
                picture.text()
            );
        }
        // Standing on the ground (the ghost floats above it).
        let (low, _) = pillar(of(character), 250).span().unwrap();
        match character {
            DancerCharacter::Ghost => assert!(low >= 2, "{character:?}"),
            _ => assert_eq!(low, 0, "{character:?}"),
        }
    }
}

#[test]
fn a_dancer_keeps_inside_its_prop() {
    let still = RenderContext::default();
    for character in CHARACTERS {
        for (columns, rows) in [(12, 50), (16, 16), (16, 32), (32, 32), (36, 16), (64, 32)] {
            // Standing between beats, there's room over its head, and beside it on a prop wider
            // than it is.
            let p = DancerParams {
                moves: DancerMove::HeadBob,
                ..of(character)
            };
            let picture = dance(p, columns, rows, 250, &still);
            let (top, last) = (rows as usize - 1, columns as usize - 1);
            assert!(
                (0..=last).all(|x| !picture.lit(x, top)),
                "{character:?} on {columns}×{rows}{}",
                picture.text()
            );
            if columns >= 32 {
                assert!(
                    (0..=top).all(|y| !picture.lit(0, y) && !picture.lit(last, y)),
                    "{character:?} on {columns}×{rows}{}",
                    picture.text()
                );
            }
        }
        // On the pillar every pose folds in from the edges instead of running off them: there's
        // always some of the dancer on both sides of its middle.
        for t in (0..16_000).step_by(125) {
            let picture = pillar(of(character), t);
            let left = picture.lit_cells().filter(|&(x, _)| x < 6).count();
            let right = picture.lit_cells().filter(|&(x, _)| x > 6).count();
            assert!(left > 15 && right > 15, "{character:?} at {t}{}", picture.text());
        }
    }
}

#[test]
fn characters_wear_their_own_colors_or_the_palettes() {
    let count = |picture: &Picture, keep: &dyn Fn(Rgba) -> bool| {
        picture
            .lit_cells()
            .filter(|&(x, y)| keep(picture.at(x, y)))
            .count()
    };
    let white = |c: Rgba| c.r > 0.8 && c.g > 0.8 && c.b > 0.8;
    let red = |c: Rgba| c.r > 0.8 && c.g < 0.3 && c.b < 0.3;
    let green = |c: Rgba| c.g > 0.8 && c.r < 0.45 && c.b < 0.3;
    let purple = |c: Rgba| c.r > 0.4 && c.b > 0.8 && c.g < 0.3;
    let orange = |c: Rgba| c.r > 0.8 && c.g > 0.25 && c.g < 0.6 && c.b < 0.2;
    let at = |character| pillar(of(character), 250);
    // A skeleton is all white, on black; so is a ghost.
    let skeleton = at(DancerCharacter::Skeleton);
    assert_eq!(count(&skeleton, &white), skeleton.lit_count());
    let ghost = at(DancerCharacter::Ghost);
    assert_eq!(count(&ghost, &white), ghost.lit_count());
    // A witch: purple hat and dress, a green face, orange hair.
    let witch = at(DancerCharacter::Witch);
    assert!(count(&witch, &purple) > 80, "{}", witch.text());
    assert!(count(&witch, &green) > 30 && count(&witch, &orange) > 10);
    // Santa: red suit, white beard and fur, a black belt.
    let santa = at(DancerCharacter::Santa);
    assert!(
        count(&santa, &red) > 80 && count(&santa, &white) > 40,
        "{}",
        santa.text()
    );
    assert!(santa.dark_count() >= 6);
    // A snowman: white, with an orange nose and a red scarf.
    let snowman = at(DancerCharacter::Snowman);
    assert!(count(&snowman, &white) > 120 && count(&snowman, &orange) >= 2);
    assert!(count(&snowman, &red) >= 5, "{}", snowman.text());
    // An elf: green, with red trim.
    let elf = at(DancerCharacter::Elf);
    assert!(
        count(&elf, &green) > 60 && count(&elf, &red) > 10,
        "{}",
        elf.text()
    );
    // With the palette instead (red, green, blue), every part takes one of its colors: as it
    // is, paler, or darker, but never a color of the character's own (white, orange, skin).
    // Only the soft edges where two parts meet mix two of them.
    for character in CHARACTERS {
        let p = DancerParams {
            use_palette: true,
            ..of(character)
        };
        let picture = pillar(p, 250);
        assert!(picture.lit_count() > 80, "{character:?}");
        let one_color = |c: Rgba| {
            let channels = [c.r, c.g, c.b];
            let top = channels.iter().copied().fold(0.0, f32::max);
            channels.iter().filter(|v| **v >= top - 0.01).count() == 1
        };
        let pure = count(&picture, &one_color);
        assert!(
            pure * 10 >= picture.lit_count() * 9,
            "{character:?}: {pure} of {}{}",
            picture.lit_count(),
            picture.text()
        );
    }
    let p = DancerParams {
        use_palette: true,
        ..of(DancerCharacter::Skeleton)
    };
    let recolored = pillar(p, 250);
    assert_eq!(
        count(&recolored, &red),
        recolored.lit_count(),
        "bones in the first color"
    );
}

#[test]
fn small_props_shed_detail_and_no_size_breaks_it() {
    let still = RenderContext::default();
    for character in CHARACTERS {
        // Eyes need a head to sit in: there on a 16-pixel prop, gone on a tiny one.
        let eyes = |columns, rows| dance(of(character), columns, rows, 250, &still).dark_count();
        assert!(eyes(16, 16) >= 2, "{character:?}");
        assert_eq!(eyes(5, 8), 0, "{character:?}");
        // Any size draws something, whatever the settings.
        for (columns, rows) in [
            (1, 1),
            (2, 5),
            (5, 2),
            (1, 50),
            (50, 1),
            (3, 3),
            (7, 9),
            (300, 300),
        ] {
            let settings = [
                (1, 90.0, 50.0, 0.0),
                (12, 200.0, 0.0, 30.0),
                (3, 10.0, 100.0, 100.0),
            ];
            for (count, size, x, y) in settings {
                for moves in [DancerMove::Mix, DancerMove::Jump, DancerMove::Kick] {
                    let p = DancerParams {
                        moves,
                        count,
                        size,
                        x,
                        y,
                        mirror: count > 1,
                        use_palette: count > 2,
                        background: DancerBackground::Glow,
                        stagger: 0.5,
                        ..of(character)
                    };
                    for t in [0, 130, 59_999] {
                        let picture = dance(p, columns, rows, t, &still);
                        assert!(
                            picture.cells.iter().all(|c| c.a.is_finite() && c.r.is_finite()),
                            "{character:?} on {columns}×{rows}"
                        );
                    }
                }
            }
            let picture = dance(of(character), columns, rows, 250, &still);
            assert!(picture.lit_count() > 0, "{character:?} on {columns}×{rows}");
        }
    }
    // Settings out of range are pulled in before they're drawn.
    let wild = DancerParams {
        size: f32::NAN,
        x: f32::INFINITY,
        y: -1e9,
        count: u32::MAX,
        stagger: f32::NEG_INFINITY,
        bass_bounce: f32::NAN,
        routine: 0,
        ..DancerParams::default()
    };
    assert!(dance(wild, 12, 50, 250, &still).lit_count() > 0);
}

// ---------------------------------------------------------------------------------------------
// The dance

#[test]
fn poses_change_from_beat_to_beat() {
    for character in CHARACTERS {
        for moves in MOVES {
            let p = DancerParams {
                moves,
                ..of(character)
            };
            // A bar and a half's beats, on the beat.
            let beats: Vec<Picture> = (0..6).map(|beat| pillar(p, beat * 500)).collect();
            for (beat, pair) in beats.windows(2).enumerate() {
                assert_ne!(
                    pair[0],
                    pair[1],
                    "{character:?} {moves:?}: beats {beat} and {}",
                    beat + 1
                );
            }
            // And it moves between them.
            assert_ne!(beats[0], pillar(p, 250), "{character:?} {moves:?}");
        }
    }
}

#[test]
fn the_mix_dances_a_new_move_each_bar() {
    let p = of(DancerCharacter::Skeleton);
    // A bar's frames, away from its ends (where one move eases into the next).
    let bar = |p: DancerParams, bar: u64| -> Vec<Picture> {
        (0..6).map(|k| pillar(p, bar * 2000 + k * 250 + 125)).collect()
    };
    // Each bar of the mix is some single move's bar, and no two bars running are the same.
    let mut danced = Vec::new();
    for n in 0..7 {
        let mixed = bar(p, n);
        let single = MOVES[1..]
            .iter()
            .position(|&moves| bar(DancerParams { moves, ..p }, n) == mixed)
            .unwrap_or_else(|| panic!("bar {n} is no single move"));
        danced.push(single);
    }
    assert!(danced.windows(2).all(|w| w[0] != w[1]), "{danced:?}");
    let mut each = danced.clone();
    each.sort_unstable();
    assert_eq!(each, [0, 1, 2, 3, 4, 5, 6], "every move once in seven bars");
    // Another routine dances them in another order; the same routine, the same.
    let other = DancerParams { routine: 2, ..p };
    assert!((0..7).any(|n| bar(other, n) != bar(p, n)));
    assert_eq!(bar(DancerParams { routine: 1, ..p }, 3), bar(p, 3));
}

#[test]
fn mirrored_is_the_same_picture_flipped() {
    let still = RenderContext::default();
    for character in CHARACTERS {
        for (columns, rows, count, x) in [(12, 50, 1, 50.0), (13, 50, 1, 50.0), (64, 32, 3, 30.0)] {
            for t in [0, 375, 1000, 2600, 7125] {
                let p = DancerParams {
                    count,
                    x,
                    stagger: 0.5,
                    background: DancerBackground::Glow,
                    ..of(character)
                };
                let plain = dance(p, columns, rows, t, &still);
                let mirrored = dance(DancerParams { mirror: true, ..p }, columns, rows, t, &still);
                assert_eq!(
                    mirrored,
                    plain.flipped(),
                    "{character:?} on {columns}×{rows} at {t}"
                );
            }
        }
        // And it does change something: no dancer is the same both ways round all bar long.
        let differs = (0..8).any(|k| {
            let plain = pillar(of(character), k * 250);
            plain != plain.flipped()
        });
        assert!(differs, "{character:?}");
    }
}

#[test]
fn several_dancers_each_keep_to_their_own_columns() {
    let still = RenderContext::default();
    for character in CHARACTERS {
        // Four across a 64-pixel prop: each is the one dancer a 16-pixel prop would show.
        let four = DancerParams {
            count: 4,
            ..of(character)
        };
        for t in [0, 250, 1125, 4000] {
            let wide = dance(four, 64, 32, t, &still);
            let alone = dance(of(character), 16, 32, t, &still);
            assert!(alone.lit_count() > 20, "{character:?}");
            for k in 0..4 {
                assert_eq!(
                    wide.columns(k * 16, k * 16 + 16),
                    alone,
                    "{character:?} dancer {k} at {t}"
                );
            }
        }
        // More dancers than fit as designed get smaller, still each in its own columns.
        let six = DancerParams {
            count: 6,
            ..of(character)
        };
        let many = dance(six, 36, 32, 250, &still);
        for k in 0..6 {
            assert!(
                many.columns(k * 6, k * 6 + 6).lit_count() > 4,
                "{character:?} dancer {k}"
            );
        }
        assert_eq!(many.columns(0, 6), many.columns(18, 24), "{character:?}");
    }
    // Staggered, each next dancer is a beat (here) behind the one before.
    let p = DancerParams {
        count: 3,
        stagger: 1.0,
        moves: DancerMove::ArmWave,
        ..of(DancerCharacter::Skeleton)
    };
    let (now, before) = (dance(p, 48, 32, 3000, &still), dance(p, 48, 32, 2500, &still));
    assert_eq!(now.columns(16, 32), before.columns(0, 16));
    assert_eq!(now.columns(32, 48), before.columns(16, 32));
    assert_ne!(now.columns(0, 16), now.columns(16, 32));
}

#[test]
fn position_and_size_place_the_dancer() {
    let still = RenderContext::default();
    let p = DancerParams {
        size: 50.0,
        moves: DancerMove::HeadBob,
        ..of(DancerCharacter::Skeleton)
    };
    let middle = |picture: &Picture| {
        let xs: Vec<usize> = picture.lit_cells().map(|(x, _)| x).collect();
        xs.iter().sum::<usize>() as f32 / xs.len() as f32
    };
    let centered = dance(p, 64, 32, 250, &still);
    let (low, high) = centered.span().unwrap();
    assert!(
        low == 0 && (13..=17).contains(&high),
        "half the prop's height: {high}"
    );
    assert!((middle(&centered) - 32.0).abs() < 2.0);
    let left = dance(DancerParams { x: 20.0, ..p }, 64, 32, 250, &still);
    assert!((middle(&left) - 13.0).abs() < 2.5, "{}", middle(&left));
    let raised = dance(DancerParams { y: 25.0, ..p }, 64, 32, 250, &still);
    assert_eq!(raised.span().unwrap().0, 8);
    // Taller than the prop is wide enough for, it stops growing.
    let narrow = |size| {
        let picture = dance(DancerParams { size, ..p }, 8, 64, 250, &still);
        picture.span().unwrap().1
    };
    assert!(narrow(30.0) < narrow(45.0));
    assert_eq!(narrow(100.0), narrow(200.0));
}

// ---------------------------------------------------------------------------------------------
// The beat

fn track(name: &str, kind: TimingKind, marks: &[(u64, &str)]) -> TimingTrack {
    TimingTrack::new(
        name,
        kind,
        marks
            .iter()
            .map(|&(at, label)| Mark::new(at, at + 40, label))
            .collect(),
    )
}

#[test]
fn it_dances_to_a_timing_tracks_marks() {
    // Beats of uneven lengths: 600, 400, 800, 600, 1000 ms.
    let starts = [1000, 1600, 2000, 2800, 3400, 4400];
    let marks: Vec<(u64, &str)> = starts.iter().map(|&at| (at, "")).collect();
    let tracks = [track("Taps", TimingKind::Custom, &marks)];
    let cx = RenderContext::new(None, &tracks, FRAME_MS);
    for character in [DancerCharacter::Skeleton, DancerCharacter::Ghost] {
        for moves in [DancerMove::ArmWave, DancerMove::Jump, DancerMove::Mix] {
            let p = DancerParams {
                moves,
                timing_track: Some(tracks[0].id),
                ..of(character)
            };
            let on_track = |t| dance(p, 12, 50, t, &cx);
            let free = DancerParams {
                timing_track: None,
                ..p
            };
            // Each mark is a beat: the pose there is the steady beat's pose on its beat, and
            // halfway to the next mark it's the pose halfway to the next beat.
            for (beat, pair) in starts.windows(2).enumerate() {
                let beat = beat as u64;
                assert_eq!(
                    on_track(pair[0]),
                    pillar(free, beat * 500),
                    "{character:?} {moves:?} mark {beat}"
                );
                assert_eq!(
                    on_track((pair[0] + pair[1]) / 2),
                    pillar(free, beat * 500 + 250),
                    "{character:?} {moves:?} after mark {beat}"
                );
            }
            // So the pose changes at every mark.
            for pair in starts.windows(2) {
                assert_ne!(on_track(pair[0]), on_track(pair[1]), "{character:?} {moves:?}");
            }
            // Past the last mark the beat carries on at the last marks' pace, and before the
            // first it's already dancing.
            assert_eq!(on_track(4900), pillar(free, 5 * 500 + 250));
            assert_ne!(on_track(400), on_track(1000));
        }
    }
}

#[test]
fn without_a_track_it_follows_the_songs_beats_or_keeps_its_own() {
    let p = DancerParams {
        moves: DancerMove::ArmWave,
        ..DancerParams::default()
    };
    // The song's beats, a little slower than two a second, numbered within their bars, the
    // first bar starting on the third mark.
    let beats: Vec<(u64, &str)> = ["3", "4", "1", "2", "3", "4", "1", "2", "3"]
        .into_iter()
        .enumerate()
        .map(|(i, label)| (i as u64 * 600, label))
        .collect();
    let bars = track("Bars", TimingKind::Bars, &[(0, "1"), (2400, "2"), (4800, "3")]);
    let tracks = [bars, track("Beats", TimingKind::Beats, &beats)];
    let cx = RenderContext::new(None, &tracks, FRAME_MS);
    // No track chosen: the Beats track, its bars counted from the mark labeled 1.
    for beat in 0..6u64 {
        assert_eq!(
            dance(p, 12, 50, (beat + 2) * 600, &cx),
            pillar(p, beat * 500),
            "beat {beat} of the first bar"
        );
    }
    // A chosen track is followed instead, mark by mark.
    let on_bars = DancerParams {
        timing_track: Some(tracks[0].id),
        ..p
    };
    assert_eq!(dance(on_bars, 12, 50, 2400, &cx), pillar(p, 500));
    // A track that's gone from the sequence: the song's beats again.
    let lost = DancerParams {
        timing_track: Some(TimingTrackId::new()),
        ..p
    };
    assert_eq!(dance(lost, 12, 50, 1800, &cx), dance(p, 12, 50, 1800, &cx));
    // No beats in the sequence at all: two beats a second from the effect's start, wherever
    // in the sequence that is.
    let unmarked = [track("Lyrics", TimingKind::Words, &[(0, "la"), (700, "la")])];
    let cx = RenderContext::new(None, &unmarked, FRAME_MS);
    assert_eq!(dance(p, 12, 50, 1500, &cx), pillar(p, 1500));
    assert_ne!(pillar(p, 0), pillar(p, 500));
    assert_eq!(pillar(p, 0), pillar(p, 2000), "arm wave comes round every bar");
    let (canvas, pixels) = grid(12, 50);
    let later = EffectTime::within(7_300, 60_000, 7_300 + 500).with_frame_ms(FRAME_MS);
    let shader = Shader::new(&EffectParams::Dancer(p), &later, Colors::new(&[]), 1, canvas);
    let picture: Vec<Rgba> = pixels.iter().map(|px| shader.shade(px)).collect();
    assert_eq!(picture, pillar(p, 500).cells);
}

#[test]
fn half_time_and_double_time_stretch_the_beat() {
    for moves in [DancerMove::ArmWave, DancerMove::Mix] {
        let p = DancerParams {
            moves,
            ..of(DancerCharacter::Skeleton)
        };
        let half = DancerParams {
            speed: DancerSpeed::Half,
            ..p
        };
        let double = DancerParams {
            speed: DancerSpeed::Double,
            ..p
        };
        for t in [0, 250, 500, 1375, 3000, 9250] {
            assert_eq!(pillar(half, 2 * t), pillar(p, t), "{moves:?} at {t}");
            assert_eq!(pillar(double, t), pillar(p, 2 * t), "{moves:?} at {t}");
        }
    }
}

const RATE: u32 = 44_100;

/// An 80 Hz tone at `amplitude` for each of `parts` (seconds, amplitude): all bass.
fn bass_line(parts: &[(f32, f32)]) -> Arc<AudioTrack> {
    let mut samples = Vec::new();
    for &(seconds, amplitude) in parts {
        let n = (seconds * RATE as f32) as usize;
        let start = samples.len();
        samples.extend((0..n).map(|i| amplitude * (TAU * 80.0 * (start + i) as f32 / RATE as f32).sin()));
    }
    Arc::new(pf_analysis::audio_track(samples, RATE, FRAME_MS))
}

#[test]
fn the_bass_pushes_the_bounce_down_when_asked() {
    // A second of loud bass, then silence.
    let music = bass_line(&[(1.0, 1.0), (2.0, 0.0)]);
    let cx = RenderContext::new(Some(Audio::new(&music, FRAME_MS)), &[], FRAME_MS);
    let top = |p: DancerParams, t: u64, cx: &RenderContext| dance(p, 12, 50, t, cx).span().unwrap().1;
    let plain = DancerParams {
        moves: DancerMove::HeadBob,
        ..of(DancerCharacter::Skeleton)
    };
    let bouncy = DancerParams {
        bass_bounce: 1.0,
        ..plain
    };
    // In the bass it sits lower than it would; in the silence, and without the setting or the
    // music, just where it would.
    assert!(top(bouncy, 750, &cx) + 2 <= top(plain, 750, &cx));
    assert_eq!(top(bouncy, 2750, &cx), top(plain, 2750, &cx));
    assert_eq!(dance(plain, 12, 50, 750, &cx), pillar(plain, 750));
    assert_eq!(pillar(bouncy, 750), pillar(plain, 750));
    // A little of it, a little lower.
    let some = DancerParams {
        bass_bounce: 0.4,
        ..plain
    };
    assert!(top(some, 750, &cx) > top(bouncy, 750, &cx));
    // It needs the music only when it bounces with it.
    let effect = |p| Effect::new(EffectKind::Dancer, 0, 1000).with_params(EffectParams::Dancer(p));
    assert!(pf_render::audio::effect_follows_music(&effect(bouncy)));
    assert!(!pf_render::audio::effect_follows_music(&effect(plain)));
    let mut curved = effect(plain);
    curved.curves.insert("bassBounce".into(), Curve::ramp(0.0, 1.0));
    assert!(pf_render::audio::effect_follows_music(&curved));
}

#[test]
fn a_glow_goes_behind_the_dancer() {
    for character in CHARACTERS {
        let plain = pillar(of(character), 250);
        let p = DancerParams {
            background: DancerBackground::Glow,
            ..of(character)
        };
        let glowing = pillar(p, 250);
        let mut dim = 0;
        for (x, y) in plain.all() {
            if plain.at(x, y).a >= 1.0 {
                assert_eq!(
                    glowing.at(x, y),
                    plain.at(x, y),
                    "{character:?}: the dancer is as it was"
                );
            } else if plain.at(x, y).a == 0.0 && glowing.at(x, y).a > 0.0 {
                assert!(glowing.level(x, y) < 0.3, "{character:?}: the glow is dim");
                dim += 1;
            }
        }
        assert!(dim > 100, "{character:?}: {dim}");
        // Around the dancer, not the whole prop.
        assert_eq!(glowing.at(0, 49).a, 0.0);
    }
}

// ---------------------------------------------------------------------------------------------
// The same every time

/// A show with one 12 × 50 pillar, as an imported one can be: twelve strands of fifty wired
/// up and down, squeezed narrow in the layout, and leaning a degree.
fn pillar_show() -> Show {
    let mut pillar = Prop::new(
        "Pillar Left",
        ShapeSource::Generator(Generator::Matrix {
            columns: 12,
            rows: 50,
            width: 0.11,
            height: 0.49,
            wiring: MatrixWiring {
                start: Corner::BottomLeft,
                orientation: Orientation::Vertical,
                serpentine: true,
            },
        }),
    );
    pillar.transform = Transform {
        position: Vec3::new(-6.5, 3.5, -2.0),
        rotation_deg: Vec3::new(0.0, 0.0, -1.0),
        scale: Vec3::new(2.6, 5.7, 1.0),
    };
    let mut show = Show::new("t");
    show.props.push(pillar);
    show
}

#[test]
fn a_frame_is_the_same_whenever_and_however_often_its_drawn() {
    let still = RenderContext::default();
    for character in CHARACTERS {
        let p = DancerParams {
            count: 2,
            stagger: 0.25,
            background: DancerBackground::Glow,
            ..of(character)
        };
        let first: Vec<Picture> = (0..40).map(|k| dance(p, 24, 50, k * 137, &still)).collect();
        // Again, backwards: no frame leans on the one before.
        for k in (0..40).rev() {
            assert_eq!(
                dance(p, 24, 50, k * 137, &still),
                first[k as usize],
                "{character:?}"
            );
        }
    }
    // The effect's own seed changes nothing: two dancers with the same settings, on two props,
    // move together.
    let p = EffectParams::Dancer(DancerParams::default());
    let (canvas, pixels) = grid(12, 50);
    let time = EffectTime::within(0, 60_000, 4_250).with_frame_ms(FRAME_MS);
    let draw = |seed| -> Vec<Rgba> {
        let shader = Shader::new(&p, &time, Colors::new(&[]), seed, canvas);
        pixels.iter().map(|px| shader.shade(px)).collect()
    };
    assert_eq!(draw(1), draw(987_654_321));
}

#[test]
fn it_renders_on_a_matrix_prop_and_seeks() {
    let show = pillar_show();
    let mut seq = Sequence::new("s", 20_000);
    seq.frame_ms = FRAME_MS;
    let beats: Vec<(u64, &str)> = (0..40)
        .map(|i| (i * 480, ["1", "2", "3", "4"][i as usize % 4]))
        .collect();
    seq.timing_tracks = vec![track("Beats", TimingKind::Beats, &beats)];
    let mut row = Row::new(Target::Prop(show.props[0].id));
    row.layers[0].effects = vec![Effect::new(EffectKind::Dancer, 0, 20_000)];
    seq.rows.push(row);
    assert_eq!(validate_sequence(&seq, &show), vec![]);
    let mut renderer = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    renderer.set_audio(AudioSource::none());
    let mut frames = Vec::new();
    for t in (0..8_000).step_by(120) {
        let mut frame = vec![0u8; renderer.frame_len()];
        renderer.render(&seq, t, &mut frame);
        // Pixel for pixel the picture drawn on a 12 × 50 grid, up one strand and down the next.
        let cx = RenderContext::new(None, &seq.timing_tracks, FRAME_MS);
        let picture = dance(DancerParams::default(), 12, 50, t, &cx);
        for (n, px) in frame.chunks(3).enumerate() {
            let (column, along) = (n / 50, n % 50);
            let row = if column % 2 == 0 { along } else { 49 - along };
            assert_eq!(px, picture.at(column, row).to_rgb8(), "at {t}: pixel {n}");
        }
        frames.push(frame);
    }
    assert!(frames.windows(4).all(|w| w[0] != w[3]), "it keeps moving");
    // Any frame again, on its own, out of order.
    let mut again = Renderer::new(&show, &pf_mapping::map_show(&show).0);
    for k in [40, 3, 66, 17] {
        let mut frame = vec![0u8; again.frame_len()];
        again.render(&seq, k as u64 * 120, &mut frame);
        assert_eq!(frame, frames[k], "frame {k}");
    }
    // A dancer whose timing track is gone says so, and dances on to the song's beats.
    let EffectParams::Dancer(p) = &mut seq.rows[0].layers[0].effects[0].params else {
        unreachable!()
    };
    p.timing_track = Some(TimingTrackId::new());
    let issues = validate_sequence(&seq, &show);
    assert_eq!(issues.len(), 1);
    assert!(
        issues[0]
            .message
            .contains("it dances to the song's beats instead"),
        "{:?}",
        issues[0]
    );
    let mut frame = vec![0u8; again.frame_len()];
    again.render(&seq, 4800, &mut frame);
    assert_eq!(frame, frames[40]);
}
