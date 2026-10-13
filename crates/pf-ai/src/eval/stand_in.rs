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
const BRIDGE: &str = "#b02aff";

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
    /// where it is loud, dark through the stop before the last chorus. All of it in one call.
    fn base(&self) -> Call {
        batch(self.looks())
    }

    /// The base with the song's signature moments on the canvases, each a look from its
    /// effect's guide: the tree filling with garlands through the intro, snow falling on the
    /// matrix through the quiet verses, icy clouds on it through the bridge (ending before the
    /// stop), and a burst of fireworks over the restart. All in one call.
    fn show(&self) -> Call {
        let mut looks = self.looks();
        looks.push(self.place(
            &["Mega Tree"],
            (0, 8_000),
            0,
            json!({ "kind": "garlands", "settings": { "spacing": 15 }, "colors": [WARM, GOLD], "brightness": 0.6 }),
        ));
        for span in VERSES {
            looks.push(self.place(
                &["Matrix"],
                span,
                0,
                json!({ "kind": "snowflakes", "settings": { "count": 14, "speed": 4, "motion": "falling" }, "colors": [WHITE], "brightness": 0.6 }),
            ));
        }
        looks.push(self.place(
            &["Matrix"],
            (72_000, 79_000),
            0,
            json!({ "kind": "plasma", "settings": { "speed": 85 }, "colors": [BRIDGE, BLUE], "brightness": 0.4 }),
        ));
        looks.push(self.place(
            &["Matrix"],
            (80_000, 84_000),
            1,
            json!({ "kind": "fireworks", "settings": { "rate": 4, "sparks": 100, "size": 80, "life": 1.8, "trail": 0.5, "fall": 0.4, "colors": "spark" }, "colors": [RED, GOLD, WHITE], "blend": "max" }),
        ));
        batch(looks)
    }

    fn looks(&self) -> Vec<Call> {
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
            json!({ "kind": "twinkle", "colors": [BRIDGE, WHITE], "brightness": 0.4 }),
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

/// Calls to one tool as one call doing them all, with `items`.
fn batch(calls: Vec<Call>) -> Call {
    let name = calls.first().map_or("place_effects", |(name, _)| *name);
    let items: Vec<Value> = calls.into_iter().map(|(_, input)| input).collect();
    (name, json!({ "items": items }))
}

fn guides(topics: &[&str], effects: &[&str]) -> Call {
    ("get_guide", json!({ "topics": topics, "effects": effects }))
}

/// Topics whole, and only some `parts` of each effect's guide.
fn guide_parts(topics: &[&str], effects: &[&str], parts: &[&str]) -> Call {
    (
        "get_guide",
        json!({ "topics": topics, "effects": effects, "parts": parts }),
    )
}

fn propose(summary: &str) -> Turn {
    calls("", &[("propose_changes", json!({ "summary": summary }))])
}

/// The effects the base is made of.
const BASE_EFFECTS: [&str; 7] = [
    "on",
    "colorWash",
    "twinkle",
    "chase",
    "bars",
    "spiral",
    "butterfly",
];

/// The showpieces the whole song places on its canvases.
const SHOWPIECES_PLACED: [&str; 4] = ["garlands", "snowflakes", "plasma", "fireworks"];

fn whole_song(on: &Rows<'_>) -> Vec<Turn> {
    let mut effects = BASE_EFFECTS.to_vec();
    effects.push("faces");
    vec![
        // The method and the design topics in one read; the effects chosen from the overview
        // and the rest of the topics in a second.
        calls(
            "",
            &[guides(
                &[
                    "method",
                    "song",
                    "roles",
                    "layering",
                    "palettes",
                    "motion",
                    "intensity",
                    "choosing",
                ],
                &[],
            )],
        ),
        calls(
            "",
            &[guide_parts(
                &["vocals", "openings-endings", "safety", "cues"],
                &effects,
                &["draws", "looks"],
            )],
        ),
        // Where the showpieces go, and the ones it will place.
        calls(
            "",
            &[guide_parts(
                &["showpieces"],
                &SHOWPIECES_PLACED,
                &["draws", "looks"],
            )],
        ),
        // The whole base with its showpieces in one call, and the moments and the singing
        // beside it.
        calls(
            "",
            &[
                on.show(),
                ("stage_cue", json!({ "stageMoments": { "minImportance": 0.5 } })),
                (
                    "stage_cue",
                    json!({ "cues": [{ "cue": "sing", "at": 8_000, "until": 72_000 }] }),
                ),
            ],
        ),
        calls("", &[("review_draft", json!({}))]),
        propose(
            "Paper lanterns in the snow: garlands filling the tree in the intro, snow on the matrix in quiet blue verses, red and gold across the house in the choruses, icy clouds in the bridge, dark for the stop, and fireworks as the last chorus lands.",
        ),
        says("Your show is ready to look over."),
    ]
}

fn dancers(on: &Rows<'_>) -> Vec<Turn> {
    vec![
        calls("", &[guides(&[], &["dancer"])]),
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
    // Each chorus's words as they are sung: no text of their own, so each says the sung words,
    // held long enough to read. One call for the three, the track shared.
    let (name, mut words) = batch(
        CHORUSES
            .iter()
            .map(|&(from, to)| {
                on.place(
                    &["Matrix"],
                    (from, to),
                    0,
                    json!({ "kind": "text", "settings": { "movement": "none" }, "colors": [WHITE] }),
                )
            })
            .collect(),
    );
    words["track"] = json!("Lyrics (words)");
    let placed: Vec<Call> = vec![
        (
            "stage_cue",
            json!({ "cues": [{ "cue": "sing", "at": 8_000, "until": 92_000, "targets": ["Singing Face"] }] }),
        ),
        (name, words),
    ];
    vec![
        calls("", &[guides(&["vocals"], &["faces", "text"])]),
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
        calls("", &[guides(&["intensity", "cues"], &[])]),
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
    let placed: Vec<Call> = vec![
        ("change_effects", json!({ "fromMs": 0, "remove": true })),
        on.base(),
    ];
    vec![
        // What is there, and everything to read, in one step.
        calls(
            "",
            &[
                guide_parts(
                    &["intensity", "layering", "roles"],
                    &BASE_EFFECTS,
                    &["draws", "looks"],
                ),
                ("list_sequence_effects", json!({})),
            ],
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
        calls("", &[batch(changes)]),
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
        calls("", &[("review_draft", json!({}))]),
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
            &[guides(&[], &["text"])],
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
        calls("", &[guide_parts(&[], &["chase", "bars"], &["draws", "use"])]),
        says(
            "On your arches a Chase sends one band of light running along each arch and round again, so it reads as something travelling. Bars lays several stripes along the arch and slides them all together, so the whole arch is striped and the stripes drift. Use Chase for a pulse that moves, Bars for a candy-stripe that fills the arch.",
        ),
    ]
}
