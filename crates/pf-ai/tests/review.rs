//! Reviewing a draft against its song: made-up songs and sequences with known faults (a missed
//! impact, a flat hit, a breakdown at full brightness, choruses that don't match, a strobe,
//! dark stretches, a hook word that never shows), each found, with a fix that fixes it; the
//! same review every time; and the review on the proposal card.

use pf_ai::provider::Message;
use pf_ai::review::{Review, Subject};
use pf_ai::testing::{ScriptedProvider, calls, fake_key, says};
use pf_ai::{Cancel, ChatEvent, ChatSession, Draft, OpenDoc, UiContext, Workspace};
use pf_analysis::{Analysis, BarEnergy, Confidence, Moment, MomentKind};
use pf_engine::{Edit, Engine};
use pf_model::{Generator, Group, GroupMember, Prop, Rgb, ShapeSource, Show, Vec3};
use pf_render::AudioSource;
use pf_sequence::{Effect, EffectKind, Mark, Row, Sequence, Target, TimingKind, TimingTrack};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 120 BPM: a beat, and a bar.
const BEAT: u64 = 500;
const BAR: u64 = 2_000;

const GRAY: Rgb = Rgb::new(128, 128, 128);
const WARM: Rgb = Rgb::new(255, 200, 128);

fn line(name: &str, x: f32, nodes: u32) -> Prop {
    let mut prop = Prop::new(
        name,
        ShapeSource::Generator(Generator::Line { nodes, length: 2.0 }),
    );
    prop.transform.position = Vec3::new(x, 0.0, 0.0);
    prop
}

/// Four lines of 20 pixels, left to right, and a group of them all.
fn show() -> Show {
    let mut show = Show::new("Test");
    for (i, name) in ["Left", "Middle Left", "Middle Right", "Right"]
        .iter()
        .enumerate()
    {
        show.props.push(line(name, i as f32 * 3.0, 20));
    }
    let mut all = Group::new("All");
    all.members = show.props.iter().map(|p| GroupMember::Prop(p.id)).collect();
    show.groups.push(all);
    show
}

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

/// A 120 BPM song with a bar for each of `bars` (its energy), loud throughout.
fn song(bars: &[f32], moments: Vec<Moment>) -> Analysis {
    let duration_ms = bars.len() as u64 * BAR;
    let beats: Vec<u64> = (0..duration_ms / BEAT).map(|i| i * BEAT).collect();
    Analysis {
        duration_ms,
        tempo_bpm: Some(120.0),
        bars: beats.iter().copied().step_by(4).collect(),
        onsets: beats.clone(),
        beats,
        energy: vec![0.8; duration_ms.div_ceil(1000) as usize],
        bar_energy: bars
            .iter()
            .map(|&e| BarEnergy {
                overall: e,
                low: e,
                mid: e,
                high: e,
            })
            .collect(),
        moments,
        confidence: Confidence {
            tempo: 0.9,
            downbeat: 0.8,
            sections: 0.0,
        },
        ..Analysis::default()
    }
}

/// A sequence with a row for each prop, and the user's own sections (label, start, end).
fn sequence(show: &Show, duration_ms: u64, sections: &[(&str, u64, u64)]) -> Sequence {
    let mut doc = Sequence::new("Lanterns", duration_ms);
    doc.rows = show.props.iter().map(|p| Row::new(Target::Prop(p.id))).collect();
    if !sections.is_empty() {
        doc.timing_tracks.push(TimingTrack::new(
            "Sections",
            TimingKind::Sections,
            sections.iter().map(|(l, a, b)| Mark::new(*a, *b, *l)).collect(),
        ));
    }
    doc
}

/// An effect on every prop's row (each its own), on `layer`.
fn everywhere(doc: &mut Sequence, layer: usize, make: impl Fn() -> Effect) {
    for row in &mut doc.rows {
        while row.layers.len() <= layer {
            row.layers.push(Default::default());
        }
        row.layers[layer].effects.push(make());
    }
}

fn on(color: Rgb, from: u64, to: u64) -> Effect {
    Effect::new(EffectKind::On, from, to).with_palette(vec![color])
}

fn review(show: &Show, doc: &Sequence, analysis: Option<&Analysis>) -> Review {
    let subject = Subject {
        show,
        doc,
        user: Some(doc),
        analysis,
        audio: &AudioSource::none(),
    };
    pf_ai::review::review(&subject, &Cancel::new()).unwrap()
}

/// A draft of `doc` on `show`.
fn draft(show: &Show, doc: &Sequence) -> Draft {
    Draft::new(Workspace {
        show: show.clone(),
        revision: 0,
        show_generation: 0,
        sequence: Some(OpenDoc {
            id: 1,
            doc: doc.clone(),
        }),
        music: None,
        audio: AudioSource::none(),
        context: UiContext::default(),
    })
}

/// Makes a fix's tool calls on the draft, as the assistant would.
fn make_fix(draft: &mut Draft, analysis: &Analysis, fix: &pf_ai::review::Fix) {
    for call in &fix.calls {
        let result = match call.tool {
            "stage_cue" => pf_ai::cues::stage(draft, Some(analysis), &call.input),
            "repeat_effects" => pf_ai::arrange::repeat(draft, &call.input),
            other => panic!("unexpected tool {other}"),
        };
        result.unwrap_or_else(|e| panic!("{} {}: {e}", call.tool, call.input));
    }
}

fn find<'a>(review: &'a Review, words: &str) -> &'a pf_ai::review::Fix {
    review
        .fixes
        .iter()
        .find(|f| f.what.contains(words))
        .unwrap_or_else(|| panic!("no fix says {words:?}: {:#?}", review.fixes))
}

fn cues(fix: &pf_ai::review::Fix) -> Vec<Value> {
    let call = fix
        .calls
        .iter()
        .find(|c| c.tool == "stage_cue")
        .expect("a stage_cue call");
    call.input["cues"].as_array().cloned().unwrap_or_default()
}

#[test]
fn a_missed_impact_is_listed_with_its_hit_and_the_hit_fixes_it() {
    let show = show();
    let analysis = song(
        &[0.6; 16],
        vec![moment(16_000, None, MomentKind::Impact, 0.9, None)],
    );
    let mut doc = sequence(&show, analysis.duration_ms, &[]);
    everywhere(&mut doc, 0, || on(GRAY, 0, 32_000));

    let before = review(&show, &doc, Some(&analysis));
    assert_eq!(before.criteria.moments, Some(0), "{before:#?}");
    assert_eq!(before.top_moments, (0, 1));
    assert!(
        before.summary.contains("the top moment not emphasised"),
        "{}",
        before.summary
    );
    let fix = find(&before, "impact (m0) doesn't show");
    assert_eq!(fix.at, "0:16.000");
    assert_eq!(cues(fix), [json!({ "cue": "hit", "at": "m0" })]);

    let mut d = draft(&show, &doc);
    make_fix(&mut d, &analysis, fix);
    let after = d.review(Some(&analysis), &Cancel::new()).unwrap().unwrap();
    assert_eq!(after.criteria.moments, Some(100), "{after:#?}");
    assert!(
        after.summary.contains("the top moment emphasised"),
        "{}",
        after.summary
    );
    assert!(after.score > before.score);
}

#[test]
fn a_flat_hit_is_flagged_and_a_dip_before_it_fixes_it() {
    let show = show();
    let analysis = song(
        &[0.6; 16],
        vec![moment(16_000, None, MomentKind::Impact, 0.9, None)],
    );
    let mut doc = sequence(&show, analysis.duration_ms, &[]);
    // The light comes up a little early and stays: the impact lands on full brightness.
    everywhere(&mut doc, 0, || on(Rgb::WHITE, 15_600, 32_000));

    let before = review(&show, &doc, Some(&analysis));
    assert_eq!(before.criteria.moments, Some(100), "it shows, near enough");
    assert_eq!(before.criteria.contrast, Some(0), "{before:#?}");
    let fix = find(&before, "Flat into the impact (m0)");
    assert_eq!(
        cues(fix),
        [json!({ "cue": "blackout", "at": 16_000 - BEAT / 2, "until": 16_000, "hit": false })]
    );

    let mut d = draft(&show, &doc);
    make_fix(&mut d, &analysis, fix);
    let after = d.review(Some(&analysis), &Cancel::new()).unwrap().unwrap();
    assert_eq!(after.criteria.contrast, Some(100), "{after:#?}");
    assert_eq!(after.criteria.moments, Some(100));
}

#[test]
fn a_breakdown_at_full_brightness_is_hotter_than_its_music() {
    let show = show();
    // Verse, chorus, breakdown, outro: the breakdown is the quietest music.
    let mut bars = vec![0.45; 6];
    bars.extend([0.9; 4]);
    bars.extend([0.1; 4]);
    bars.extend([0.45; 2]);
    let analysis = song(&bars, Vec::new());
    let sections = [
        ("Verse", 0, 12_000),
        ("Chorus", 12_000, 20_000),
        ("Breakdown", 20_000, 28_000),
        ("Outro", 28_000, 32_000),
    ];
    let mut doc = sequence(&show, analysis.duration_ms, &sections);
    everywhere(&mut doc, 0, || on(Rgb::new(176, 176, 176), 0, 12_000));
    everywhere(&mut doc, 0, || on(Rgb::WHITE, 12_000, 28_000));
    everywhere(&mut doc, 0, || on(Rgb::new(176, 176, 176), 28_000, 32_000));

    let r = review(&show, &doc, Some(&analysis));
    let fix = find(&r, "Breakdown (0:20.000–0:28.000) is lit like a loud part");
    assert_eq!(
        cues(fix)[0],
        json!({ "cue": "minimal", "at": 20_000, "until": 28_000 })
    );
    assert!(r.criteria.energy.unwrap() < 80, "{r:#?}");
    assert!(
        !r.fixes
            .iter()
            .any(|f| f.what.contains("Chorus") && f.what.contains("dim")),
        "the chorus follows its music: {:#?}",
        r.fixes
    );
}

#[test]
fn a_chorus_unlike_the_first_is_flagged_and_matching_ones_are_not() {
    let show = show();
    let analysis = song(&[0.6; 16], Vec::new());
    let sections = [
        ("Verse 1", 0, 8_000),
        ("Chorus 1", 8_000, 16_000),
        ("Verse 2", 16_000, 24_000),
        ("Chorus 2", 24_000, 32_000),
    ];
    let mut doc = sequence(&show, analysis.duration_ms, &sections);
    everywhere(&mut doc, 0, || on(WARM, 0, 8_000));
    everywhere(&mut doc, 0, || on(Rgb::RED, 8_000, 16_000));
    everywhere(&mut doc, 0, || on(WARM, 16_000, 24_000));
    let matching = {
        let mut doc = doc.clone();
        everywhere(&mut doc, 0, || on(Rgb::RED, 24_000, 32_000));
        doc
    };
    everywhere(&mut doc, 0, || {
        Effect::new(EffectKind::Twinkle, 24_000, 32_000).with_palette(vec![Rgb::BLUE])
    });

    let r = review(&show, &doc, Some(&analysis));
    let fix = find(&r, "Chorus 2 looks unlike Chorus 1");
    assert_eq!(fix.calls[0].tool, "repeat_effects");
    assert_eq!(
        fix.calls[0].input,
        json!({ "fromMs": 8_000, "toMs": 16_000, "startsMs": [24_000], "replace": true })
    );
    assert!(r.criteria.consistency.unwrap() < 100);

    let r = review(&show, &matching, Some(&analysis));
    assert_eq!(r.criteria.consistency, Some(100), "{:#?}", r.fixes);
}

#[test]
fn a_ten_hertz_strobe_over_the_whole_show_is_unsafe_and_a_slow_pulse_is_not() {
    let show = show();
    let strobe = |period: u64| {
        let mut doc = sequence(&show, 8_000, &[]);
        let mut t = 2_000;
        while t < 6_000 {
            everywhere(&mut doc, 0, || on(Rgb::WHITE, t, t + period / 2));
            t += period;
        }
        doc
    };

    let fast = review(&show, &strobe(100), None);
    assert!(!fast.flashes_safe, "{fast:#?}");
    assert_eq!(fast.criteria.safety, 0);
    assert!(fast.score <= 60, "unsafe flashing caps the score: {}", fast.score);
    assert!(fast.summary.ends_with("flashing too fast"), "{}", fast.summary);
    let fix = &fast.fixes[0];
    assert!(
        fix.what.contains("flashes about 10 times a second"),
        "{}",
        fix.what
    );
    assert!(fix.hint.as_deref().unwrap().contains("under 3 a second"));
    assert!(fix.calls.is_empty());

    let slow = review(&show, &strobe(500), None);
    assert!(slow.flashes_safe, "2 flashes a second: {:#?}", slow.fixes);
    assert_eq!(slow.criteria.safety, 100);
    assert!(slow.summary.ends_with("flashes safe"));
}

#[test]
fn dark_stretches_while_the_music_plays_are_dead_air_but_a_stop_is_not() {
    let show = show();
    let analysis = song(
        &[0.6; 16],
        vec![moment(22_000, Some(24_000), MomentKind::Stop, 0.8, Some("full"))],
    );
    let mut doc = sequence(&show, analysis.duration_ms, &[]);
    everywhere(&mut doc, 0, || on(GRAY, 0, 10_000));
    everywhere(&mut doc, 0, || on(GRAY, 16_000, 22_000));
    everywhere(&mut doc, 0, || on(GRAY, 24_000, 32_000));

    let r = review(&show, &doc, Some(&analysis));
    assert!(r.criteria.dead_air < 100, "{r:#?}");
    let fix = find(
        &r,
        "Nothing is lit from 0:10.000 to 0:16.000 while the music plays",
    );
    let cue = &cues(fix)[0];
    assert_eq!(
        (&cue["cue"], &cue["at"], &cue["until"]),
        (&json!("breathe"), &json!(10_000), &json!(16_000))
    );
    assert!(
        !r.fixes.iter().any(|f| f.what.contains("0:22")),
        "the stop may be dark: {:#?}",
        r.fixes
    );
    assert_eq!(r.criteria.moments, Some(100), "the stop goes dark");

    let mut d = draft(&show, &doc);
    make_fix(&mut d, &analysis, fix);
    let after = d.review(Some(&analysis), &Cancel::new()).unwrap().unwrap();
    assert_eq!(after.criteria.dead_air, 100, "{after:#?}");
}

#[test]
fn a_hook_word_that_never_shows_gets_word_pops() {
    let show = show();
    let analysis = song(
        &[0.6; 16],
        vec![moment(18_000, None, MomentKind::Shout, 0.4, Some("Lantern"))],
    );
    let mut doc = sequence(&show, analysis.duration_ms, &[]);
    let words = [
        (6_000, "glow"),
        (8_000, "Lantern"),
        (18_000, "Lantern"),
        (26_000, "lantern!"),
    ];
    doc.timing_tracks.push(TimingTrack::new(
        "Lyrics (words)",
        TimingKind::Words,
        words.iter().map(|(t, w)| Mark::new(*t, t + 400, *w)).collect(),
    ));
    everywhere(&mut doc, 0, || on(GRAY, 0, 32_000));

    let r = review(&show, &doc, Some(&analysis));
    assert_eq!(r.criteria.lyrics, Some(0), "{r:#?}");
    let fix = find(&r, "hook word \"lantern\" doesn't show 3 of the 3 times");
    assert_eq!(
        cues(fix),
        [json!({ "cue": "word_pop", "at": 8_000, "until": 26_400 + BEAT, "match": "lantern" })]
    );
    let mut d = draft(&show, &doc);
    make_fix(&mut d, &analysis, fix);
    let after = d.review(Some(&analysis), &Cancel::new()).unwrap().unwrap();
    assert_eq!(after.criteria.lyrics, Some(100), "{after:#?}");
}

/// Busy effects over a long song: twinkles (random), chases, color washes, and meters.
fn busy(show: &Show, duration_ms: u64) -> Sequence {
    let mut doc = sequence(show, duration_ms, &[]);
    let kinds = [
        EffectKind::Twinkle,
        EffectKind::Chase,
        EffectKind::ColorWash,
        EffectKind::Bars,
    ];
    let mut t = 0;
    let mut k = 0;
    while t < duration_ms {
        let end = (t + 4 * BAR).min(duration_ms);
        everywhere(&mut doc, 0, || {
            Effect::new(kinds[k % kinds.len()], t, end).with_palette(vec![Rgb::RED, Rgb::BLUE])
        });
        t = end;
        k += 1;
    }
    doc
}

#[test]
fn the_same_sequence_reviews_the_same_way_every_time() {
    let show = show();
    let analysis = song(
        &[0.3, 0.6, 0.9, 0.6].repeat(8),
        vec![moment(20_000, None, MomentKind::Drop, 0.9, None)],
    );
    let doc = busy(&show, analysis.duration_ms);
    let a = review(&show, &doc, Some(&analysis));
    let b = review(&show, &doc, Some(&analysis));
    assert_eq!(a, b);
    assert_eq!(a.to_model(), b.to_model());
}

#[test]
fn a_long_song_on_a_bigger_show_reviews_in_reasonable_time() {
    let mut show = Show::new("Big");
    for i in 0..20 {
        show.props.push(line(&format!("Line {i}"), i as f32, 100));
    }
    let analysis = song(&[0.5; 90], Vec::new());
    let doc = busy(&show, analysis.duration_ms);
    let started = Instant::now();
    let r = review(&show, &doc, Some(&analysis));
    // A loose bound (an unoptimized build on a slow machine), not a benchmark.
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "{:?}",
        started.elapsed()
    );
    println!("{:?}: {r:?}", started.elapsed());
    assert_eq!(r.criteria.dead_air, 100, "it was all drawn, and lit");
}

#[test]
fn review_draft_reviews_the_open_sequence_and_the_proposal_carries_the_review() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path());
    let show = show();
    let edits: Vec<Edit> = show
        .props
        .iter()
        .map(|p| Edit::AddProp { prop: p.clone() })
        .collect();
    engine.apply(edits).unwrap();
    let rows = engine
        .show()
        .props
        .iter()
        .map(|p| {
            let mut row = Row::new(Target::Prop(p.id));
            row.layers[0].effects.push(on(GRAY, 0, 32_000));
            row
        })
        .collect();
    engine
        .new_sequence_doc_with_rows("Lanterns", 32_000, Some("/music/lanterns.mp3"), rows)
        .unwrap();
    let analysis = song(
        &[0.6; 16],
        vec![moment(16_000, None, MomentKind::Impact, 0.9, None)],
    );
    let provider = ScriptedProvider::new(vec![
        calls("", &[("review_draft", json!({}))]),
        calls(
            "",
            &[("stage_cue", json!({ "cues": [{ "cue": "hit", "at": "m0" }] }))],
        ),
        calls(
            "",
            &[("propose_changes", json!({ "summary": "A hit on the impact." }))],
        ),
        says("Ready."),
    ]);
    let found = analysis.clone();
    let mut session =
        ChatSession::new().with_analyzer(Arc::new(move |_: &Path, _: &Cancel| Ok(found.clone())));
    let mut events = Vec::new();
    let reply = session
        .run_turn(
            &provider,
            &fake_key(),
            "scripted",
            "review this sequence",
            Workspace::from_engine(&engine, UiContext::default()),
            &Cancel::new(),
            &mut |e| events.push(e),
        )
        .unwrap();

    // The open sequence, unchanged, is reviewed.
    let Some(Message::ToolResults(results)) = provider.requests()[1].last().cloned() else {
        panic!()
    };
    let said = &results[0].content;
    assert!(said.starts_with("Review: {\"score\":"), "{said}");
    assert!(
        said.contains("\"tool\":\"stage_cue\"") && said.contains("\"at\":\"m0\""),
        "{said}"
    );
    assert!(said.ends_with("Changes: The draft has no changes yet."), "{said}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ChatEvent::Activity { label } if label == "Checking the draft"))
    );

    // The proposal shows the final review.
    let review = reply.proposal.unwrap().review.expect("a review on the card");
    assert!(review.line.starts_with("Review: "), "{}", review.line);
    assert!(
        review.line.contains("the top moment emphasised · flashes safe"),
        "{}",
        review.line
    );
    let card = serde_json::to_value(session.proposal().unwrap().view()).unwrap();
    assert_eq!(card["review"]["line"], json!(review.line));
}
