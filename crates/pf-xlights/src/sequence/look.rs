//! How an imported effect is expected to look next to the xLights original.
//!
//! The importer knows which settings it carried over ([`Fidelity`]); what a viewer cares about is
//! whether the result looks the same. [`expected`] turns the one into the other with [`TABLE`]:
//! which xLights effects, under which conditions, were measured (or judged) to come across the
//! same, close, or different. `pf-fidelity labels` holds the table against a scorecard.

use super::effects::Fidelity;
use serde::{Deserialize, Serialize};

/// How an imported effect looks next to the xLights original, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// A viewer wouldn't tell them apart at show distance.
    Matches,
    /// The same idea, with differences a viewer could notice.
    Close,
    /// It reads as a different look.
    Different,
    /// PixelFlow has no effect for it yet: shown as a stand-in, or left out.
    NoEquivalent,
}

impl Verdict {
    pub const ALL: [Verdict; 4] = [
        Verdict::Matches,
        Verdict::Close,
        Verdict::Different,
        Verdict::NoEquivalent,
    ];

    /// The verdict as a heading ("Looks close").
    pub fn title(self) -> &'static str {
        match self {
            Verdict::Matches => "Matches",
            Verdict::Close => "Close",
            Verdict::Different => "Different",
            Verdict::NoEquivalent => "No equivalent",
        }
    }

    /// The verdict in a sentence about one effect or several ("Bars (3 effects) look close").
    pub fn phrase(self, several: bool) -> &'static str {
        match (self, several) {
            (Verdict::Matches, true) => "come across the same",
            (Verdict::Matches, false) => "comes across the same",
            (Verdict::Close, true) => "look close",
            (Verdict::Close, false) => "looks close",
            (Verdict::Different, true) => "look different",
            (Verdict::Different, false) => "looks different",
            (Verdict::NoEquivalent, true) => "have no PixelFlow effect yet",
            (Verdict::NoEquivalent, false) => "has no PixelFlow effect yet",
        }
    }
}

/// A number for each verdict.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByVerdict<T> {
    pub matches: T,
    pub close: T,
    pub different: T,
    pub no_equivalent: T,
}

impl<T: Copy + std::ops::AddAssign> ByVerdict<T> {
    pub fn get(&self, verdict: Verdict) -> T {
        match verdict {
            Verdict::Matches => self.matches,
            Verdict::Close => self.close,
            Verdict::Different => self.different,
            Verdict::NoEquivalent => self.no_equivalent,
        }
    }

    pub fn add(&mut self, verdict: Verdict, amount: T) {
        match verdict {
            Verdict::Matches => self.matches += amount,
            Verdict::Close => self.close += amount,
            Verdict::Different => self.different += amount,
            Verdict::NoEquivalent => self.no_equivalent += amount,
        }
    }

    pub fn merge(&mut self, other: &ByVerdict<T>) {
        for verdict in Verdict::ALL {
            self.add(verdict, other.get(verdict));
        }
    }
}

/// One line on how a sequence comes across, from how long its effects of each verdict run
/// added up: "82% of this sequence comes across the same · 12% looks close · 6% looks
/// different". A verdict nothing got isn't mentioned (but for the first), and one that got next
/// to nothing says "under 1%". `None` for a sequence with no effects.
pub fn headline(ms: &ByVerdict<u64>) -> Option<String> {
    let total: u64 = Verdict::ALL.iter().map(|&v| ms.get(v)).sum();
    if total == 0 {
        return None;
    }
    let share = |verdict: Verdict| {
        let part = ms.get(verdict);
        // Whole percents that never claim all of it, or none of it, when that isn't so.
        match (part * 100 + total / 2) / total {
            0 if part > 0 => "under 1%".to_string(),
            100 if part < total => "99%".to_string(),
            percent => format!("{percent}%"),
        }
    };
    let mut line = format!(
        "{} of this sequence comes across the same",
        share(Verdict::Matches)
    );
    for (verdict, what) in [
        (Verdict::Close, "looks close"),
        (Verdict::Different, "looks different"),
        (Verdict::NoEquivalent, "has no PixelFlow effect yet"),
    ] {
        if ms.get(verdict) > 0 {
            line.push_str(&format!(" · {} {what}", share(verdict)));
        }
    }
    Some(line)
}

/// The effects of one kind that don't come across the same, and what a viewer would notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookGroup {
    pub verdict: Verdict,
    /// xLights' name for the effect.
    pub effect: String,
    pub effects: usize,
    /// How long those effects run, added up.
    pub ms: u64,
    /// What differs, in a few words each; one that holds for only some of the effects says how
    /// many ("fading tail not shown (2)").
    pub reasons: Vec<String>,
}

impl LookGroup {
    /// The group as a sentence ("Bars (2 effects) look close: fading tail not shown.").
    pub fn sentence(&self) -> String {
        let several = self.effects != 1;
        let what = format!(
            "{} ({} effect{}) {}",
            self.effect,
            self.effects,
            if several { "s" } else { "" },
            self.verdict.phrase(several)
        );
        if self.reasons.is_empty() {
            format!("{what}.")
        } else {
            format!("{what}: {}.", super::list(&self.reasons))
        }
    }
}

/// When a row of the table applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// Whatever the effect's settings.
    Always,
    /// The importer noted a difference whose text contains this.
    Noted(&'static str),
}

/// One line of the table: an xLights effect, a condition, and how it's expected to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// xLights' name for the effect; `"*"` for any effect.
    pub effect: &'static str,
    pub when: When,
    pub verdict: Verdict,
    /// What a viewer would notice, where no note of the importer's says it already.
    pub why: &'static str,
}

const fn always(effect: &'static str, verdict: Verdict, why: &'static str) -> Row {
    Row {
        effect,
        when: When::Always,
        verdict,
        why,
    }
}

const fn noted(effect: &'static str, text: &'static str, verdict: Verdict) -> Row {
    Row {
        effect,
        when: When::Noted(text),
        verdict,
        why: "",
    }
}

/// Where an effect is expected to look other than the rule gives: an effect with nothing noted
/// is expected to match, and one with a difference noted to look close, unless a row here says
/// otherwise; the worst row that applies wins.
///
/// The rule itself is measured: of the kinds `pf-fidelity` has judged with nothing else on
/// their pixels (Bars, Butterfly, Color Wash, Fan, Fill, Fire, Life, Morph, Pictures, Pinwheel,
/// Plasma, Spirals), those imported with nothing noted came across the same. So are the rows
/// marked measured. The rest are judged from what the note says a viewer would see, by the
/// same yardstick the measurements use: a look with the wrong colors, nothing moving where
/// xLights moves, or light on another part of the prop reads as a different look; a shape, a
/// direction, or a speed that is off is the same idea with a noticeable difference.
///
/// Chance on both sides is no difference. Where an effect draws its moments or its places by
/// chance in both programs (fireworks, snow, meteors, wandering balls), the importer notes
/// nothing for that and the rule expects a match: `pf-fidelity` holds such an effect against
/// PixelFlow's own renders from other seeds, and as much light, as bright, in the same colors
/// at other moments is the same look. For Fireworks that expectation is judged and not
/// measured. Its settings are carried over from what xLights' frames show, and the one in the
/// corpus lights too few pixels of its props for its look to say anything, either way.
pub const TABLE: &[Row] = &[
    // Judged: each is drawn as the nearest PixelFlow effect with its usual settings, a stand-in.
    // Fireworks was a fourth, and measured: it lit the prop steadily where xLights bursts and
    // goes dark. It has an effect of its own now.
    always("Snowstorm", Verdict::Different, ""),
    always("Galaxy", Verdict::Different, ""),
    always("Curtain", Verdict::Different, ""),
    // Judged: other colors altogether.
    noted("*", "rainbow", Verdict::Different),
    noted("Plasma", "colors shown in the palette", Verdict::Different),
    // Judged: still where xLights moves, or one picture where xLights plays several.
    noted("Text", "movement shown still", Verdict::Different),
    noted("Pictures", "the picture stays still", Verdict::Different),
    noted("Pictures", "shown as the first picture alone", Verdict::Different),
    // Judged: nothing, or something else, where xLights draws.
    noted(
        "Text",
        "words from a lyrics track or a file not shown",
        Verdict::Different,
    ),
    noted("VU Meter", "so it shows nothing", Verdict::Different),
    noted("VU Meter", "shown as volume bars", Verdict::Different),
    // Judged: light over the whole prop where xLights lights a part of it.
    noted("*", "not just the part xLights draws on", Verdict::Different),
    // A Shader effect runs the same file and, with nothing noted, is measured to come across
    // the same (nine of ten in the sequence that has them, the tenth on all but one prop).
    // Judged: a picture xLights moves or sizes that PixelFlow doesn't, and an input standing
    // still where xLights changes it, read as a different look.
    noted("Shader", "offset or zoom isn't applied", Verdict::Different),
    noted("Shader", "stays at its slider's value", Verdict::Different),
    // Judged: the heads still, or on a round path, where xLights moves them along another.
    noted("Moving Head", "drawn path not followed", Verdict::Different),
    noted("Moving Head", "pattern shown as a circle", Verdict::Different),
    noted("Moving Head", "no head's settings found", Verdict::Different),
];

/// How an effect imported as `fidelity` from the xLights effect `effect` is expected to look,
/// and what the table adds to the importer's notes about why.
pub fn expected(effect: &str, fidelity: &Fidelity) -> (Verdict, Vec<&'static str>) {
    expected_from(TABLE, effect, fidelity)
}

fn expected_from(table: &[Row], effect: &str, fidelity: &Fidelity) -> (Verdict, Vec<&'static str>) {
    let notes: &[String] = match fidelity {
        Fidelity::Placeholder | Fidelity::Skipped => return (Verdict::NoEquivalent, Vec::new()),
        Fidelity::Exact => &[],
        Fidelity::Approximate(notes) => notes,
    };
    let mut verdict = if notes.is_empty() {
        Verdict::Matches
    } else {
        Verdict::Close
    };
    let mut why = Vec::new();
    for row in table
        .iter()
        .filter(|row| row.effect == "*" || row.effect == effect)
    {
        let applies = match row.when {
            When::Always => true,
            When::Noted(text) => notes.iter().any(|n| n.contains(text)),
        };
        if applies {
            verdict = verdict.max(row.verdict);
            if !row.why.is_empty() && !why.contains(&row.why) {
                why.push(row.why);
            }
        }
    }
    (verdict, why)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: &[Row] = &[
        always("Text", Verdict::Close, "drawn in other lettering"),
        noted("*", "shown as a chase", Verdict::Different),
        noted("Bars", "straight movement", Verdict::Different),
    ];

    fn noting(note: &str) -> Fidelity {
        Fidelity::Approximate(vec![note.to_string()])
    }

    fn verdict(effect: &str, fidelity: &Fidelity) -> Verdict {
        expected_from(ROWS, effect, fidelity).0
    }

    #[test]
    fn an_effect_with_nothing_noted_matches_unless_a_row_says_otherwise() {
        assert_eq!(
            expected_from(ROWS, "On", &Fidelity::Exact),
            (Verdict::Matches, vec![])
        );
        assert_eq!(
            expected_from(ROWS, "Text", &Fidelity::Exact),
            (Verdict::Close, vec!["drawn in other lettering"])
        );
    }

    #[test]
    fn a_noted_difference_looks_close_and_the_rows_that_apply_can_make_it_worse() {
        assert_eq!(
            verdict("Bars", &noting("blur above 15 shown at 15")),
            Verdict::Close
        );
        assert_eq!(
            verdict("Bars", &noting("'Expand' direction shown as straight movement")),
            Verdict::Different
        );
        // A row for one effect says nothing of another.
        assert_eq!(
            verdict("Wave", &noting("shown as straight movement")),
            Verdict::Close
        );
        assert_eq!(
            verdict("Marquee", &noting("shown as a chase along the pixels")),
            Verdict::Different
        );
    }

    #[test]
    fn placeholders_and_effects_left_out_have_no_equivalent() {
        assert_eq!(verdict("Shader", &Fidelity::Placeholder), Verdict::NoEquivalent);
        assert_eq!(verdict("DMX", &Fidelity::Skipped), Verdict::NoEquivalent);
    }

    #[test]
    fn a_group_reads_as_a_sentence_about_how_it_looks() {
        let group = |verdict, effects, reasons: &[&str]| LookGroup {
            verdict,
            effect: "Bars".to_string(),
            effects,
            ms: 1000,
            reasons: reasons.iter().map(|r| r.to_string()).collect(),
        };
        assert_eq!(
            group(Verdict::Close, 2, &["fading tail not shown", "slower (1)"]).sentence(),
            "Bars (2 effects) look close: fading tail not shown, slower (1)."
        );
        assert_eq!(
            group(Verdict::Different, 1, &["drawn as a color wash"]).sentence(),
            "Bars (1 effect) looks different: drawn as a color wash."
        );
        assert_eq!(
            group(Verdict::NoEquivalent, 1, &[]).sentence(),
            "Bars (1 effect) has no PixelFlow effect yet."
        );
    }

    #[test]
    fn the_headline_shares_out_the_sequence_by_how_its_effects_look() {
        let of = |matches, close, different, no_equivalent| {
            headline(&ByVerdict {
                matches,
                close,
                different,
                no_equivalent,
            })
        };
        assert_eq!(
            of(82_000, 12_000, 6_000, 0).as_deref(),
            Some("82% of this sequence comes across the same · 12% looks close · 6% looks different")
        );
        assert_eq!(
            of(50_000, 0, 0, 50_000).as_deref(),
            Some("50% of this sequence comes across the same · 50% has no PixelFlow effect yet")
        );
        assert_eq!(
            of(4_000, 0, 0, 0).as_deref(),
            Some("100% of this sequence comes across the same")
        );
        // Nothing matching is still said; next to nothing isn't rounded away, or up to all.
        assert_eq!(
            of(0, 999_000, 1_000, 0).as_deref(),
            Some("0% of this sequence comes across the same · 99% looks close · under 1% looks different")
        );
        assert_eq!(of(0, 0, 0, 0), None);
    }

    #[test]
    fn counts_by_verdict_add_up() {
        let mut counts = ByVerdict::<u64>::default();
        counts.add(Verdict::Matches, 3);
        counts.add(Verdict::NoEquivalent, 2);
        let mut all = counts;
        all.merge(&counts);
        assert_eq!(
            (all.matches, all.close, all.different, all.no_equivalent),
            (6, 0, 0, 4)
        );
        assert_eq!(all.get(Verdict::NoEquivalent), 4);
    }

    #[test]
    fn the_table_gives_stand_ins_and_missing_looks_their_verdicts() {
        let (verdict, why) = expected("Snowstorm", &noting("shown as falling meteors"));
        assert_eq!((verdict, why.is_empty()), (Verdict::Different, true));
        // Fireworks has an effect of its own: as any effect, the same unless something is noted.
        // Bursting at random moments is chance on both sides, and the importer notes nothing
        // for it; a burst that keeps other time than xLights' is noted.
        assert_eq!(expected("Fireworks", &Fidelity::Exact).0, Verdict::Matches);
        assert_eq!(
            expected(
                "Fireworks",
                &noting("bursts follow the music at about the moments xLights' do")
            )
            .0,
            Verdict::Close
        );
        assert_eq!(expected("Twinkle", &Fidelity::Exact).0, Verdict::Matches);
        assert_eq!(
            expected("Twinkle", &noting("strobing twinkles shown as soft twinkles")).0,
            Verdict::Close
        );
        // A rainbow drawn in the palette's colors, on whatever effect.
        assert_eq!(
            expected(
                "SingleStrand",
                &noting("rainbow colors shown in the palette colors")
            )
            .0,
            Verdict::Different
        );
        assert_eq!(
            expected("Meteors", &noting("rainbow meteors shown in the palette colors")).0,
            Verdict::Different
        );
        assert_eq!(
            expected(
                "Pictures",
                &noting("'peekaboo' movement not shown: the picture stays still")
            )
            .0,
            Verdict::Different
        );
        assert_eq!(
            expected(
                "Pictures",
                &noting("'Scale To Fit' scaling shown pixel for pixel")
            )
            .0,
            Verdict::Close
        );
        assert_eq!(
            expected("Shader", &Fidelity::Placeholder).0,
            Verdict::NoEquivalent
        );
    }

    #[test]
    fn every_row_of_the_table_can_apply() {
        // A row that makes things better than the default would never win.
        for row in TABLE {
            match row.when {
                When::Always => assert!(row.verdict > Verdict::Matches, "{row:?}"),
                When::Noted(text) => {
                    assert!(row.verdict > Verdict::Close && !text.is_empty(), "{row:?}");
                }
            }
        }
    }
}
