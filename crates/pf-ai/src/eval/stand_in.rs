//! Scripted answers to each scenario: what a careful designer would do, written out by hand, so
//! the evaluation can run with no model at all. They prove the harness end to end (a good answer
//! passes its checks, an idle one fails them) and give its report something to show. They say
//! nothing about how well any model does.

use super::scenarios::Scenario;
use crate::error::AiError;
use crate::provider::{AssistantTurn, Cancel};
use crate::review::{self, Subject};
use crate::stage::{self, Stage};
use crate::testing::{calls, says};
use pf_sequence::Sequence;
use serde_json::{Value, json};

type Turn = Result<AssistantTurn, AiError>;
type Call = (&'static str, Value);

const VERSES: [(u64, u64); 2] = [(8_000, 24_000), (40_000, 56_000)];
const CHORUSES: [(u64, u64); 3] = [(24_000, 40_000), (56_000, 72_000), (80_000, 92_000)];

const WARM: &str = "#ffc880";
const GOLD: &str = "#ffd700";
const BLUE: &str = "#2a6bff";
const RED: &str = "#ff2a2a";
const WHITE: &str = "#ffffff";
const ORANGE: &str = "#ff7a00";
const PURPLE: &str = "#7a1fd1";
const SLIME: &str = "#39ff14";

/// The scripted answer to `scenario` on `stage` (the stage the scenario will run on: rows are
/// named by id, and every stage has its own).
pub fn script(scenario: &Scenario, stage: &Stage) -> Vec<Turn> {
    let start = (scenario.start)(stage);
    let on = Rows { stage, doc: &start };
    match scenario.key {
        "whole-song" => whole_song(&on),
        "dancers" => dancers(&on),
        "sing-along" => sing_along(&on),
        "build" => build(),
        "calm-and-big" => calm_and_big(&on),
        "halloween" => halloween(&on),
        "review-and-fix" => review_and_fix(&on),
        "scroll-words" => scroll_words(&on),
        "question" => question(),
        _ => idle(),
    }
}

/// An answer that does nothing: what every scenario's checks must fail.
pub fn idle() -> Vec<Turn> {
    vec![says("Done.")]
}

/// The scenario's sequence and its rows, by the names of what they light.
struct Rows<'a> {
    stage: &'a Stage,
    doc: &'a Sequence,
}

impl Rows<'_> {
    fn ids(&self, names: &[&str]) -> Vec<String> {
        names
            .iter()
            .filter_map(|name| self.stage.named(name))
            .filter_map(|target| self.stage.row(self.doc, target))
            .map(|id| id.to_string())
            .collect()
    }

    /// `effect` on the rows of `names` over `span`, on `layer`.
    fn place(&self, names: &[&str], span: (u64, u64), layer: usize, effect: Value) -> Call {
        (
            "place_effects",
            json!({ "rowIds": self.ids(names), "fromMs": span.0, "toMs": span.1, "layer": layer, "effect": effect }),
        )
    }

    /// A look for every part of the song: little and dim where it is quiet, the whole house
    /// where it is loud, dark through the stop before the last chorus.
    fn base(&self) -> Vec<Call> {
        let mut out = vec![
            self.place(
                &["Roofline"],
                (0, 8_000),
                0,
                json!({ "kind": "on", "colors": [WARM], "brightness": 0.3, "fadeInMs": 2000 }),
            ),
            self.place(
                &["Tree Star"],
                (0, 8_000),
                0,
                json!({ "kind": "on", "colors": [GOLD], "sparkles": 60 }),
            ),
        ];
        for span in VERSES {
            out.push(self.place(
                &["Roofline", "Windows"],
                span,
                0,
                json!({ "kind": "on", "colors": [BLUE], "brightness": 0.45 }),
            ));
            out.push(self.place(
                &["Mega Tree"],
                span,
                0,
                json!({ "kind": "twinkle", "colors": [BLUE, WHITE], "brightness": 0.6 }),
            ));
            out.push(self.place(
                &["Arches"],
                span,
                0,
                json!({ "kind": "chase", "colors": [WHITE], "brightness": 0.5 }),
            ));
        }
        for span in CHORUSES {
            out.push(self.place(
                &["Whole House"],
                span,
                0,
                json!({ "kind": "colorWash", "colors": [RED, GOLD] }),
            ));
            out.push(self.place(
                &["Arches"],
                span,
                0,
                json!({ "kind": "bars", "settings": { "axis": "horizontal", "count": 3 }, "colors": [RED, WHITE] }),
            ));
            out.push(self.place(
                &["Mega Tree"],
                span,
                0,
                json!({ "kind": "spiral", "colors": [RED, WHITE] }),
            ));
            out.push(self.place(
                &["Matrix"],
                span,
                0,
                json!({ "kind": "butterfly", "colors": [RED, GOLD] }),
            ));
        }
        out.push(self.place(
            &["Mega Tree"],
            (72_000, 79_000),
            0,
            json!({ "kind": "twinkle", "colors": ["#b02aff", WHITE], "brightness": 0.4 }),
        ));
        out.push(self.place(
            &["Whole House"],
            (92_000, 96_000),
            0,
            json!({ "kind": "on", "settings": { "startLevel": 1, "endLevel": 0 }, "colors": [WARM] }),
        ));
        out
    }
}

fn guides(topics: &[&str], effects: &[&str]) -> Call {
    ("get_guide", json!({ "topics": topics, "effects": effects }))
}

fn propose(summary: &str) -> Turn {
    calls("", &[("propose_changes", json!({ "summary": summary }))])
}

fn whole_song(on: &Rows<'_>) -> Vec<Turn> {
    vec![
        calls("", &[guides(&["method"], &[])]),
        calls(
            "",
            &[
                ("analyze_song", json!({})),
                ("get_open_sequence", json!({})),
                ("list_props", json!({})),
            ],
        ),
        calls(
            "",
            &[guides(
                &["song", "roles", "layering", "palettes", "motion", "intensity"],
                &[],
            )],
        ),
        calls(
            "",
            &[guides(
                &[],
                &[
                    "on",
                    "colorWash",
                    "twinkle",
                    "chase",
                    "bars",
                    "spiral",
                    "butterfly",
                ],
            )],
        ),
        calls("", &on.base()),
        calls(
            "",
            &[guides(
                &["vocals", "openings-endings", "safety", "cues"],
                &["faces"],
            )],
        ),
        calls(
            "",
            &[
                ("stage_cue", json!({ "stageMoments": { "minImportance": 0.5 } })),
                (
                    "stage_cue",
                    json!({ "cues": [{ "cue": "sing", "at": 8_000, "until": 72_000 }] }),
                ),
            ],
        ),
        calls("", &[("review_draft", json!({}))]),
        propose(
            "A show for the whole song: blue and quiet in the verses, red and gold across the house in the choruses, dark for the stop, with the big moments staged.",
        ),
        says("Your show is ready to look over."),
    ]
}

fn dancers(on: &Rows<'_>) -> Vec<Turn> {
    vec![
        calls("", &[guides(&[], &["dancer"]), ("get_open_sequence", json!({}))]),
        calls(
            "",
            &[on.place(
                &["Matrix"],
                CHORUSES[0],
                0,
                json!({ "kind": "dancer", "settings": { "character": "skeleton", "background": "glow" } }),
            )],
        ),
        propose("Dancing skeletons on the matrix through the first chorus."),
        says("The skeletons dance through the first chorus."),
    ]
}

fn sing_along(on: &Rows<'_>) -> Vec<Turn> {
    let mut placed: Vec<Call> = vec![(
        "stage_cue",
        json!({ "cues": [{ "cue": "sing", "at": 8_000, "until": 92_000, "targets": ["Singing Face"] }] }),
    )];
    // Each chorus's words as they are sung: no text of their own, so each says the sung words,
    // held long enough to read.
    placed.extend(CHORUSES.iter().map(|&(from, to)| {
        let (name, mut input) = on.place(
            &["Matrix"],
            (from, to),
            0,
            json!({ "kind": "text", "settings": { "movement": "none" }, "colors": [WHITE] }),
        );
        input["track"] = json!("Lyrics (words)");
        (name, input)
    }));
    vec![
        calls(
            "",
            &[
                guides(&["vocals"], &["faces", "text"]),
                ("analyze_song", json!({})),
            ],
        ),
        calls("", &placed),
        calls("", &[("review_draft", json!({}))]),
        propose(
            "The singing face sings the words, and each chorus's words come up on the matrix as they are sung.",
        ),
        says("The face sings and the chorus words show on the matrix."),
    ]
}

fn build() -> Vec<Turn> {
    vec![
        calls(
            "",
            &[guides(&["intensity", "cues"], &[]), ("analyze_song", json!({}))],
        ),
        calls(
            "",
            &[(
                "stage_cue",
                json!({ "cues": [{ "cue": "ramp", "at": 20_000, "until": 24_000 }] }),
            )],
        ),
        propose("A two-bar build into the first chorus, landing on its first beat."),
        says("The last two bars of the verse now build into the chorus."),
    ]
}

fn calm_and_big(on: &Rows<'_>) -> Vec<Turn> {
    let mut placed: Vec<Call> = vec![("change_effects", json!({ "fromMs": 0, "remove": true }))];
    placed.extend(on.base());
    vec![
        calls(
            "",
            &[
                guides(&["intensity", "layering", "roles"], &[]),
                ("analyze_song", json!({})),
                ("list_sequence_effects", json!({})),
            ],
        ),
        calls(
            "",
            &[guides(
                &[],
                &[
                    "on",
                    "colorWash",
                    "twinkle",
                    "chase",
                    "bars",
                    "spiral",
                    "butterfly",
                ],
            )],
        ),
        calls("", &placed),
        calls("", &[("review_draft", json!({}))]),
        propose("Quiet blue verses on a few props, and red and gold choruses across the whole house."),
        says("The verses are calmer now and the choruses use the whole house."),
    ]
}

fn halloween(on: &Rows<'_>) -> Vec<Turn> {
    let house = on.ids(&["Whole House"]);
    let recolor = |select: Value, colors: &[&str]| -> Call {
        let mut input = select;
        input["change"] = json!({ "colors": colors });
        ("change_effects", input)
    };
    let mut changes = vec![recolor(json!({ "rowIds": house, "fromMs": 0 }), &[ORANGE])];
    changes.extend(
        VERSES
            .iter()
            .map(|(from, to)| recolor(json!({ "rowIds": house, "fromMs": from, "toMs": to }), &[PURPLE])),
    );
    changes.extend([
        recolor(json!({ "kinds": ["bars"] }), &[ORANGE, PURPLE]),
        recolor(json!({ "kinds": ["spiral"] }), &[ORANGE, PURPLE]),
        recolor(json!({ "kinds": ["chase"] }), &[ORANGE]),
        recolor(json!({ "kinds": ["twinkle"] }), &[PURPLE, SLIME]),
    ]);
    vec![
        calls(
            "",
            &[guides(&["palettes"], &[]), ("list_sequence_effects", json!({}))],
        ),
        calls("", &changes),
        propose("Everything recolored in orange, purple, and a little slime green; nothing else moved."),
        says("It's in Halloween colors now."),
    ]
}

/// The fixes the review gives for the scenario's sequence, as the tool calls it spells out.
fn fixes(on: &Rows<'_>) -> Vec<Call> {
    let song = stage::song();
    review::review(
        &Subject {
            show: &on.stage.show,
            doc: on.doc,
            user: Some(on.doc),
            analysis: Some(&song),
            audio: &stage::audio(),
            pictures: &stage::pictures(),
        },
        &Cancel::new(),
    )
    .map(|reviewed| {
        reviewed
            .fixes
            .into_iter()
            .flat_map(|fix| fix.calls.into_iter().map(|call| (call.tool, call.input)))
            .collect()
    })
    .unwrap_or_default()
}

fn review_and_fix(on: &Rows<'_>) -> Vec<Turn> {
    vec![
        calls("", &[("analyze_song", json!({})), ("review_draft", json!({}))]),
        calls("", &fixes(on)),
        calls("", &[("review_draft", json!({}))]),
        propose(
            "The review's fixes made: the flashing calmed, the colors held to a few, the words made readable, and the dark stretches filled.",
        ),
        says("I made the fixes the review asked for."),
    ]
}

fn scroll_words(on: &Rows<'_>) -> Vec<Turn> {
    vec![
        calls(
            "",
            &[guides(&[], &["text"]), ("get_open_sequence", json!({}))],
        ),
        calls(
            "",
            &[on.place(
                &["Matrix"],
                CHORUSES[2],
                0,
                json!({ "kind": "text", "settings": { "text": "LANTERNS HIGH", "movement": "left", "speed": 4 }, "colors": [WHITE] }),
            )],
        ),
        calls("", &[("review_draft", json!({}))]),
        propose("LANTERNS HIGH scrolling across the matrix through the last chorus."),
        says("The words cross the matrix through the last chorus."),
    ]
}

fn question() -> Vec<Turn> {
    vec![
        calls("", &[guides(&[], &["chase", "bars"])]),
        says(
            "On your arches a Chase sends one band of light running along each arch and round again, so it reads as something travelling. Bars lays several stripes along the arch and slides them all together, so the whole arch is striped and the stripes drift. Use Chase for a pulse that moves, Bars for a candy-stripe that fills the arch.",
        ),
    ]
}
