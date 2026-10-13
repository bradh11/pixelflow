//! The review judges a draft as a design, not only against the beat: made-up sequences on the
//! made-up stage, each with one fault a show designer would name (props left out, nothing ever
//! dark, a rainbow in one section, a chorus that looks like its verse, a big moment on three
//! props, fire on a string, words too wide to read, a shimmer over the whole house, one effect
//! all song long with nothing on the canvases), each found, said specifically enough to fix, and
//! fixed by the fix it gives.

use pf_ai::review::{Call, Fix, Review};
use pf_ai::stage::{self, BAR_MS, SONG_MS, Stage};
use pf_ai::{Cancel, Draft, OpenDoc, UiContext, Workspace};
use pf_analysis::Analysis;
use pf_model::Rgb;
use pf_render::{AudioSource, Pictures};
use pf_sequence::{Effect, EffectKind, EffectParams, Sequence};
use serde_json::{Value, json};

const RED: Rgb = Rgb::new(255, 0, 0);
const GREEN: Rgb = Rgb::new(0, 200, 60);
const BLUE: Rgb = Rgb::new(40, 90, 255);

struct Show {
    stage: Stage,
    doc: Sequence,
    song: Analysis,
}

impl Show {
    fn new() -> Self {
        let stage = Stage::new();
        let doc = stage.sequence();
        Self {
            stage,
            doc,
            song: stage::song(),
        }
    }

    /// `effect` on the row of the prop or group called `name`, on `layer`.
    fn put(&mut self, name: &str, layer: usize, effect: Effect) {
        let target = self.stage.named(name).unwrap_or_else(|| panic!("no {name}"));
        let row = self.doc.rows.iter_mut().find(|r| r.target == target).unwrap();
        while row.layers.len() <= layer {
            row.layers.push(Default::default());
        }
        row.layers[layer].effects.push(effect);
    }

    /// A steady color on `name` from `from` to `to`.
    fn on(&mut self, name: &str, color: Rgb, from: u64, to: u64) {
        self.put(
            name,
            0,
            Effect::new(EffectKind::On, from, to).with_palette(vec![color]),
        );
    }

    /// A look for each part of the song on the whole house: its own color each time the music
    /// changes, dark through the bridge's last second (where the band stops).
    fn a_look_per_section(&mut self) {
        for (section, color) in self.song.sections.clone().iter().zip([
            Rgb::new(255, 200, 128),
            BLUE,
            RED,
            BLUE,
            RED,
            Rgb::new(120, 0, 200),
            RED,
            Rgb::new(255, 200, 128),
        ]) {
            let end = if section.label == "Bridge" {
                section.end_ms - 1_000
            } else {
                section.end_ms
            };
            self.on("Whole House", color, section.start_ms, end);
        }
    }

    fn draft(&self) -> Draft {
        Draft::new(Workspace {
            show: self.stage.show.clone(),
            revision: 0,
            show_generation: 0,
            sequence: Some(OpenDoc {
                id: 1,
                doc: self.doc.clone(),
            }),
            music: None,
            audio: AudioSource::none(),
            pictures: Pictures::none(),
            images: Vec::new(),
            shaders: Vec::new(),
            context: UiContext::default(),
        })
    }

    fn review(&self) -> Review {
        review_of(&mut self.draft(), &self.song)
    }
}

fn review_of(draft: &mut Draft, song: &Analysis) -> Review {
    draft.review(Some(song), &Cancel::new()).unwrap().unwrap()
}

fn find<'a>(review: &'a Review, words: &str) -> &'a Fix {
    review
        .fixes
        .iter()
        .find(|f| f.what.contains(words))
        .unwrap_or_else(|| panic!("no fix says {words:?}: {:#?}", review.fixes))
}

/// Makes a fix's tool calls on the draft, as the assistant would.
fn make_fix(draft: &mut Draft, song: &Analysis, fix: &Fix) {
    assert!(!fix.calls.is_empty(), "the fix has calls to make: {fix:#?}");
    for call in &fix.calls {
        let result = match call.tool {
            "stage_cue" => pf_ai::cues::stage(draft, Some(song), &call.input),
            "change_effects" => pf_ai::rearrange::change(draft, &call.input),
            "place_effects" => pf_ai::arrange::place(draft, &call.input),
            other => panic!("unexpected tool {other}"),
        };
        result.unwrap_or_else(|e| panic!("{} {}: {e}", call.tool, call.input));
    }
}

#[test]
fn a_sound_design_passes_every_design_check() {
    let mut show = Show::new();
    show.a_look_per_section();
    let review = show.review();
    let c = &review.criteria;
    assert_eq!(c.props, Some(100), "{review:#?}");
    assert_eq!(c.palette, Some(100), "{review:#?}");
    assert_eq!(c.changes, Some(100), "{review:#?}");
    assert_eq!(c.fit, 100);
    assert_eq!(c.text, None, "no words, nothing to read");
    for words in [
        "never light",
        "colors at once",
        "Nothing changes at",
        "can't be read",
    ] {
        assert!(
            review.fixes.iter().all(|f| !f.what.contains(words)),
            "{words}: {:#?}",
            review.fixes
        );
    }
}

#[test]
fn props_with_no_part_are_named_with_their_rows() {
    let mut show = Show::new();
    // Only the arches and the tree ever play.
    for name in ["Arches", "Mega Tree"] {
        show.on(name, RED, 0, SONG_MS);
    }
    let review = show.review();
    assert_eq!(review.criteria.props, Some(42), "5 of 12 props play: {review:#?}");
    let fix = find(&review, "never light");
    assert!(
        fix.what.starts_with(
            "7 of the 12 props never light (half the display or more): Roofline, Window Left, and 5 more."
        ),
        "{}",
        fix.what
    );
    // Its hint gives the rows to put effects on.
    let roofline = show
        .stage
        .row(&show.doc, show.stage.named("Roofline").unwrap())
        .unwrap()
        .to_string();
    let hint = fix.hint.as_deref().unwrap();
    assert!(
        hint.contains(&roofline) && hint.contains("place_effects"),
        "{hint}"
    );

    show.on("Whole House", BLUE, 0, SONG_MS);
    assert_eq!(show.review().criteria.props, Some(100));
}

#[test]
fn a_show_with_nothing_ever_dark_is_told_what_to_rest() {
    let mut show = Show::new();
    show.on("Whole House", BLUE, 0, SONG_MS);
    let before = show.review();
    assert_eq!(before.criteria.dark_space, Some(0), "{before:#?}");
    let fix = find(&before, "Every prop is lit for 100% of the song");
    // The quiet parts of the song, to thin out: the intro, the bridge, and the outro.
    assert_eq!(
        fix.calls[0].input,
        json!({ "cues": [
            { "cue": "minimal", "at": 0, "until": 8_000 },
            { "cue": "minimal", "at": 72_000, "until": 80_000 },
            { "cue": "minimal", "at": 92_000, "until": 96_000 },
        ] })
    );
    let mut draft = show.draft();
    make_fix(&mut draft, &show.song, fix);
    let after = review_of(&mut draft, &show.song);
    assert!(
        after.criteria.dark_space.unwrap() >= 40,
        "a fifth of the song now rests: {:?} then {:?}",
        before.criteria.dark_space,
        after.criteria.dark_space
    );
    // A show whose whole house is lit only in its choruses has nothing to answer for.
    let mut shaped = Show::new();
    for section in shaped.song.sections.clone() {
        let name = if section.label == "Chorus" {
            "Whole House"
        } else {
            "Arches"
        };
        shaped.on(name, BLUE, section.start_ms, section.end_ms);
    }
    let review = shaped.review();
    assert_eq!(review.criteria.dark_space, Some(100), "{review:#?}");
    assert!(review.fixes.iter().all(|f| !f.what.contains("Every prop is lit")));
}

#[test]
fn a_section_in_too_many_colors_is_told_which_to_keep() {
    let mut show = Show::new();
    show.a_look_per_section();
    // The first chorus in seven colors, a prop each.
    let (from, to) = (12 * BAR_MS, 20 * BAR_MS);
    for (name, color) in [
        ("Roofline", RED),
        ("Matrix", GREEN),
        ("Mega Tree", BLUE),
        ("Arch 1", Rgb::new(255, 255, 0)),
        ("Arch 2", Rgb::new(255, 0, 255)),
        ("Arch 3", Rgb::new(0, 255, 255)),
        ("Arch 4", Rgb::new(255, 128, 0)),
    ] {
        show.on(name, color, from, to);
    }
    let before = show.review();
    assert_eq!(
        before.criteria.palette,
        Some(88),
        "one of eight sections: {before:#?}"
    );
    let fix = find(&before, "colors at once");
    assert!(
        fix.what
            .starts_with("Chorus 1 (0:24.000–0:40.000) uses 7 colors at once"),
        "{}",
        fix.what
    );
    let call = &fix.calls[0];
    assert_eq!(call.tool, "change_effects");
    assert_eq!(
        (call.input["fromMs"].as_u64(), call.input["toMs"].as_u64()),
        (Some(from), Some(to))
    );
    assert_eq!(call.input["change"]["colors"].as_array().unwrap().len(), 3);

    let mut draft = show.draft();
    make_fix(&mut draft, &show.song, fix);
    let after = review_of(&mut draft, &show.song);
    assert_eq!(after.criteria.palette, Some(100), "{after:#?}");
}

#[test]
fn a_new_section_that_looks_like_the_last_is_told_to_change() {
    let mut show = Show::new();
    // One look from the top of the first verse through the first chorus; the rest as it should be.
    show.a_look_per_section();
    let whole = show.stage.named("Whole House").unwrap();
    let row = show.doc.rows.iter_mut().find(|r| r.target == whole).unwrap();
    row.layers[0]
        .effects
        .retain(|e| !(8_000..40_000).contains(&e.start_ms));
    show.on("Whole House", BLUE, 8_000, 40_000);
    let before = show.review();
    assert!(before.criteria.changes.unwrap() < 100, "{before:#?}");
    let fix = find(&before, "Nothing changes at Chorus 1 (0:24.000)");
    assert!(
        fix.what.contains("looks the same as the Verse 1 before it"),
        "{}",
        fix.what
    );
    let cues = fix.calls[0].input["cues"].as_array().unwrap();
    assert_eq!(cues[0], json!({ "cue": "sweep", "at": 24_000 }));
    assert_eq!(cues[1]["cue"], "color_shift");
    assert_eq!(
        (cues[1]["at"].as_u64(), cues[1]["until"].as_u64()),
        (Some(24_000), Some(40_000))
    );

    let mut draft = show.draft();
    make_fix(&mut draft, &show.song, fix);
    let after = review_of(&mut draft, &show.song);
    assert_eq!(after.criteria.changes, Some(100), "{after:#?}");
}

#[test]
fn a_big_moment_on_a_few_props_is_given_the_whole_house() {
    let mut show = Show::new();
    // The song's biggest moment (the restart at 80 s) lands on the arches alone.
    show.on("Arches", RED, 80_000, 92_000);
    let before = show.review();
    assert!(before.criteria.scale.unwrap() < 50, "{before:#?}");
    let fix = find(&before, "lights only 4 of the 12 props");
    assert!(
        fix.what
            .starts_with("The restart (m0), one of the song's biggest moments"),
        "{}",
        fix.what
    );
    assert_eq!(
        fix.calls[0].input,
        json!({ "cues": [{ "cue": "hit", "at": "m0", "intensity": 1.0, "targets": ["all"] }] })
    );
    let mut draft = show.draft();
    make_fix(&mut draft, &show.song, fix);
    let after = review_of(&mut draft, &show.song);
    assert!(
        after
            .fixes
            .iter()
            .all(|f| !f.what.contains("The restart (m0), one of")),
        "{:#?}",
        after.fixes
    );
    assert!(after.criteria.scale > before.criteria.scale);
}

#[test]
fn effects_on_props_they_dont_suit_are_named_and_words_that_cant_be_read_are_explained() {
    let mut show = Show::new();
    show.a_look_per_section();
    // Life wants a grid to live on; a roofline is one row. A Faces effect wants a face.
    for bar in [12, 14, 16] {
        show.put(
            "Roofline",
            1,
            Effect::new(EffectKind::Life, bar * BAR_MS, (bar + 2) * BAR_MS),
        );
    }
    show.put("Mega Tree", 1, Effect::new(EffectKind::Faces, 24_000, 28_000));
    // Sixteen letters can't stand still on forty columns; a roofline can't hold letters at all.
    let words = |text: &str| {
        Effect::new(EffectKind::Text, 40_000, 44_000).with_params(EffectParams::Text(
            pf_sequence::TextParams {
                text: text.to_string(),
                ..Default::default()
            },
        ))
    };
    show.put("Matrix", 1, words("MERRY AND BRIGHT"));
    show.put("Roofline", 1, words("JOY"));
    // Life suits the tree and the matrix; a short word fits the matrix.
    show.put("Mega Tree", 2, Effect::new(EffectKind::Life, 48_000, 52_000));
    let mut fits = words("JOY");
    (fits.start_ms, fits.end_ms) = (48_000, 52_000);
    show.put("Matrix", 1, fits);

    let review = show.review();
    assert!(review.criteria.fit < 100, "{review:#?}");
    let fire = find(&review, "Roofline is a single run of pixels");
    assert!(
        fire.what.contains(
            "so Life shows only a slice of itself there (its guide has looks for: matrix, tree, house)"
        ) && fire.what.contains("(3 effects, from 0:24.000)"),
        "{}",
        fire.what
    );
    let hint = fire.hint.as_deref().unwrap();
    assert!(
        hint.contains("\"kinds\": [\"life\"]") && hint.contains("change_effects"),
        "{hint}"
    );
    let faces = find(&review, "Mega Tree has no face");
    assert!(
        faces.what.contains("the sing effect sings on any prop"),
        "{}",
        faces.what
    );
    assert!(
        review
            .fixes
            .iter()
            .all(|f| !f.what.contains("Mega Tree is a single run")),
        "life on a tree is fine: {:#?}",
        review.fixes
    );

    assert_eq!(review.criteria.text, Some(33), "one of three can be read");
    let wide = find(&review, "The words on Matrix at 0:40.000 can't be read");
    assert!(
        wide.what.contains("6 letters fit") && wide.what.contains("Scroll it"),
        "{}",
        wide.what
    );
    // The settings that make them readable come with it: once across in the four seconds.
    assert_eq!(wide.calls.len(), 1, "{wide:?}");
    assert_eq!(wide.calls[0].tool, "change_effects");
    assert_eq!(
        wide.calls[0].input["change"],
        json!({ "settings": { "movement": "left", "speed": 14 } })
    );
    let run = find(&review, "The words on Roofline at 0:40.000 can't be read");
    assert!(run.what.contains("letters need rows to stand on"), "{}", run.what);
}

#[test]
fn a_shimmer_over_the_whole_house_is_unsafe_though_no_frame_shows_it() {
    let mut show = Show::new();
    show.a_look_per_section();
    let calm = show.review();
    assert!(calm.flashes_safe, "{calm:#?}");
    // Every other frame the other color, twenty times a second, on every prop for two bars. The
    // run-through draws every 50 ms and sees a steady color.
    let mut shimmer = Effect::new(EffectKind::On, 24_000, 28_000).with_palette(vec![RED, BLUE]);
    let Value::Object(mut params) = serde_json::to_value(&shimmer.params).unwrap() else {
        panic!()
    };
    params.insert("shimmer".into(), json!(true));
    shimmer.params = serde_json::from_value(Value::Object(params)).unwrap();
    show.put("Whole House", 1, shimmer.clone());
    let review = show.review();
    assert!(!review.flashes_safe);
    assert_eq!(review.criteria.safety, 0);
    assert!(
        review.score <= 60,
        "unsafe flashing caps the score: {}",
        review.score
    );
    let fix = &review.fixes[0];
    assert!(
        fix.what.starts_with("Whole House flickers about 20 times a second over a large part of the display (0:24.000–0:28.000)"),
        "{}",
        fix.what
    );
    // The call that stops it comes with it, naming the effect.
    assert_eq!(
        fix.calls,
        vec![Call {
            tool: "change_effects",
            input: json!({ "ids": [shimmer.id.to_string()], "change": { "settings": { "shimmer": false } } }),
        }]
    );
    // On one arch it is a small area: not this check's business.
    let mut small = Show::new();
    small.a_look_per_section();
    small.put("Arch 1", 1, shimmer);
    assert!(small.review().flashes_safe);
}

#[test]
fn a_narrow_show_is_told_where_a_showpiece_goes_and_the_call_places_one() {
    // One effect, `on`, in every section: every part of the song plays the same kit, and the
    // matrix and the tree never show anything only a canvas can.
    let show = {
        let mut show = Show::new();
        show.a_look_per_section();
        show
    };
    let before = show.review();
    // One kind of the ten it has room for, no showpiece, and every section the same kit as the
    // one before it: next to nothing.
    let range = before
        .criteria
        .range
        .expect("a show with effects is judged for its range");
    assert!(range <= 5, "{before:#?}");
    let kinds = find(&before, "The show uses 1 kind of effect");
    assert!(
        kinds.what.contains("has room for about 10") && kinds.hint.as_deref().unwrap().contains("`plasma`"),
        "{kinds:#?}"
    );
    // The range's fixes take the room the others leave: here, the sections repeating their
    // kit are left off a full list.
    assert!(before.more_fixes >= 2, "{before:#?}");
    // A show that is otherwise sound loses a point or two for its range, not its standing.
    let mut full = before.criteria.clone();
    full.range = Some(100);
    assert!(
        before.score + 3 >= full.overall(),
        "{} against {}",
        before.score,
        full.overall()
    );

    // Each canvas is told what to show, with the call that places it, from its guide.
    let matrix = find(&before, "Matrix never shows a showpiece");
    assert!(
        matrix
            .what
            .contains("plasma \"Icy clouds\" through Bridge (1:12.000–1:20.000)"),
        "{}",
        matrix.what
    );
    let tree = find(&before, "Mega Tree never shows a showpiece");
    assert!(
        tree.what
            .contains("fireworks \"A firework finale on a matrix\" through Chorus 3"),
        "{}",
        tree.what
    );
    // In the bridge's own colors, ending before the stop at 1:19.
    let call = &matrix.calls[0];
    assert_eq!(call.tool, "place_effects");
    assert_eq!(call.input["toMs"], json!(79_000));
    assert_eq!(call.input["effect"]["kind"], json!("plasma"));
    let mut draft = show.draft();
    for fix in [matrix, tree] {
        make_fix(&mut draft, &show.song, fix);
    }
    let after = review_of(&mut draft, &show.song);
    assert!(after.criteria.range > before.criteria.range, "{after:#?}");
    assert!(
        after
            .fixes
            .iter()
            .all(|f| !f.what.contains("never shows a showpiece")),
        "{:#?}",
        after.fixes
    );
    // What it placed suits the props, keeps the colors few and the stop dark, and is safe.
    let c = &after.criteria;
    assert_eq!((c.fit, c.palette, c.safety), (100, Some(100), 100), "{after:#?}");
    assert_eq!(c.moments, before.criteria.moments, "{after:#?}");
}

#[test]
fn range_never_means_an_effect_on_a_prop_it_doesnt_suit() {
    // A fan's blades show as dashes on one row of pixels: the review still says so, however
    // much range it adds.
    let mut show = Show::new();
    show.a_look_per_section();
    show.put("Arch 1", 1, Effect::new(EffectKind::Fan, 24_000, 32_000));
    let review = show.review();
    assert!(review.criteria.fit < 100, "{review:#?}");
    let fan = find(&review, "Arch 1 is a single run of pixels");
    assert!(
        fan.what
            .contains("so Fan shows only a slice of itself there (its guide has looks for: matrix, house)"),
        "{}",
        fan.what
    );
    // Nor does it count for the range: a kind put where it doesn't suit adds nothing.
    let mut plain = Show::new();
    plain.a_look_per_section();
    assert_eq!(review.criteria.range, plain.review().criteria.range);
    // On the matrix it suits, and is the matrix's showpiece.
    let mut show = Show::new();
    show.a_look_per_section();
    show.put("Matrix", 1, Effect::new(EffectKind::Fan, 24_000, 32_000));
    let review = show.review();
    assert_eq!(review.criteria.fit, 100);
    assert!(
        review
            .fixes
            .iter()
            .all(|f| !f.what.contains("Matrix never shows a showpiece")),
        "{:#?}",
        review.fixes
    );
}
