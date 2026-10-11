//! Holding the importer's table of expected looks against what was measured.
//!
//! The importer labels every effect by how it expects it to look (`pf_xlights::sequence::look`),
//! from a table that has to be right for sequences nobody has measured. A scorecard lists, for
//! each kind of effect and each set of differences the importer noted, the verdicts those
//! effects earned (`labels.rows`). [`check`] puts the two side by side: where they disagree, the
//! table needs a row, or the effect needs fixing.

use pf_xlights::sequence::look::expected;
use pf_xlights::sequence::{ByVerdict, Fidelity, Verdict};
use serde::Deserialize;
use std::fmt::Write as _;

/// The fewest effects judged alone that a row needs before it can say the table is wrong.
const ENOUGH: usize = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    matches: usize,
    close: usize,
    different: usize,
    no_equivalent: usize,
}

impl From<Counts> for ByVerdict<usize> {
    fn from(counts: Counts) -> Self {
        Self {
            matches: counts.matches,
            close: counts.close,
            different: counts.different,
            no_equivalent: counts.no_equivalent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Written {
    kind: String,
    notes: Vec<String>,
    expected: Verdict,
    effects: usize,
    measured: Counts,
    measured_alone: Counts,
    score: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Labels {
    rows: Vec<Written>,
}

#[derive(Debug, Deserialize)]
struct Card {
    labels: Labels,
}

/// One kind of effect with one set of noted differences, as a scorecard measured it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub kind: String,
    pub notes: Vec<String>,
    /// What the importer expected when the scorecard was made.
    pub expected: Verdict,
    pub effects: usize,
    pub measured: ByVerdict<usize>,
    pub measured_alone: ByVerdict<usize>,
    pub score: Option<f64>,
}

/// The rows of a scorecard (a sequence's or a corpus's), from its JSON.
pub fn read(json: &str) -> Result<Vec<Row>, serde_json::Error> {
    let card: Card = serde_json::from_str(json)?;
    Ok(card
        .labels
        .rows
        .into_iter()
        .map(|row| Row {
            kind: row.kind,
            notes: row.notes,
            expected: row.expected,
            effects: row.effects,
            measured: row.measured.into(),
            measured_alone: row.measured_alone.into(),
            score: row.score,
        })
        .collect())
}

fn total(counts: &ByVerdict<usize>) -> usize {
    Verdict::ALL.iter().map(|&v| counts.get(v)).sum()
}

/// A row with what the table says of it now.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub row: Row,
    /// What the importer's table gives such an effect today.
    pub table: Verdict,
    /// What most of its effects earned: those judged alone when there are any, all otherwise;
    /// the worse verdict on a tie. `None` when none was judged.
    pub earned: Option<Verdict>,
    /// Whether that rests on effects judged alone.
    pub alone: bool,
}

impl Checked {
    /// The table and the measurement disagree, on enough effects judged alone to say so. What
    /// was only ever judged with other effects mixed in is a hint: the difference may be theirs.
    pub fn disagrees(&self) -> bool {
        self.alone
            && self.earned.is_some_and(|earned| earned != self.table)
            && total(&self.row.measured_alone) >= ENOUGH
    }
}

/// Holds each row against the importer's table as it is now.
pub fn check(rows: &[Row]) -> Vec<Checked> {
    rows.iter()
        .map(|row| {
            let fidelity = if row.expected == Verdict::NoEquivalent {
                Fidelity::Placeholder
            } else if row.notes.is_empty() {
                Fidelity::Exact
            } else {
                Fidelity::Approximate(row.notes.clone())
            };
            let alone = total(&row.measured_alone) > 0;
            let counts = if alone { &row.measured_alone } else { &row.measured };
            // A placeholder is "no equivalent" by what it is, not by how it scored.
            let earned = if row.expected == Verdict::NoEquivalent {
                (total(counts) > 0).then_some(Verdict::NoEquivalent)
            } else {
                Verdict::ALL
                    .into_iter()
                    .filter(|&v| counts.get(v) > 0)
                    .max_by_key(|&v| (counts.get(v), v))
            };
            Checked {
                row: row.clone(),
                table: expected(&row.kind, &fidelity).0,
                earned,
                alone,
            }
        })
        .collect()
}

/// The check as a table, the rows that disagree first.
pub fn markdown(checked: &[Checked]) -> String {
    let mut out = String::new();
    let counts = |by: &ByVerdict<usize>| {
        format!(
            "{} / {} / {} / {}",
            by.matches, by.close, by.different, by.no_equivalent
        )
    };
    let wrong = checked.iter().filter(|c| c.disagrees()).count();
    let _ = writeln!(
        out,
        "{} of {} rows disagree with the importer's table (counts are matches / close / different / \
         no equivalent).\n",
        wrong,
        checked.len()
    );
    let _ = writeln!(
        out,
        "| | Effect | The importer noted | Table says | Earned | Effects | Judged | Judged alone | Look |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|");
    let mut ordered: Vec<&Checked> = checked.iter().collect();
    ordered.sort_by_key(|c| !c.disagrees());
    for c in ordered {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            if c.disagrees() { "✗" } else { "" },
            c.row.kind.replace('|', "\\|"),
            if c.row.notes.is_empty() {
                "–".to_string()
            } else {
                c.row.notes.join("; ").replace('|', "\\|")
            },
            c.table.title(),
            c.earned.map_or_else(
                || "–".to_string(),
                |v| format!("{}{}", v.title(), if c.alone { "" } else { " (layered)" })
            ),
            c.row.effects,
            counts(&c.row.measured),
            counts(&c.row.measured_alone),
            c.row.score.map_or_else(|| "–".to_string(), |s| format!("{s:.0}")),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, notes: &[&str], alone: [usize; 3], all: [usize; 3]) -> Row {
        let by = |c: [usize; 3]| ByVerdict {
            matches: c[0],
            close: c[1],
            different: c[2],
            no_equivalent: 0,
        };
        Row {
            kind: kind.to_string(),
            notes: notes.iter().map(|n| n.to_string()).collect(),
            expected: Verdict::Matches,
            effects: all.iter().sum(),
            measured: by(all),
            measured_alone: by(alone),
            score: Some(90.0),
        }
    }

    #[test]
    fn a_row_earns_what_most_of_its_effects_judged_alone_did() {
        // Alone: two match, one is close. Layered, most look different, which isn't held
        // against the kind.
        let checked = check(&[row("On", &[], [2, 1, 0], [2, 1, 5])]);
        assert_eq!(
            (checked[0].earned, checked[0].alone),
            (Some(Verdict::Matches), true)
        );
        assert_eq!(checked[0].table, Verdict::Matches);
        assert!(!checked[0].disagrees());
        // None alone: all of them, the worse verdict on a tie; a hint, since what they were
        // layered with may be what differs.
        let checked = check(&[row("On", &[], [0, 0, 0], [2, 2, 0])]);
        assert_eq!(
            (checked[0].earned, checked[0].alone),
            (Some(Verdict::Close), false)
        );
        assert!(!checked[0].disagrees());
        assert!(markdown(&checked).contains("Close (layered)"));
    }

    #[test]
    fn one_effect_isnt_enough_to_call_the_table_wrong_and_none_says_nothing() {
        let checked = check(&[
            row("On", &[], [0, 0, 1], [0, 0, 1]),
            row("On", &[], [0, 0, 0], [0, 0, 0]),
            row("On", &[], [0, 0, 2], [0, 0, 2]),
        ]);
        assert_eq!(
            checked.iter().map(Checked::disagrees).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(checked[1].earned, None);
        let text = markdown(&checked);
        assert!(text.starts_with("1 of 3 rows disagree"), "{text}");
        // The row that disagrees is listed first.
        assert!(
            text.lines().nth(4).is_some_and(|l| l.starts_with("| ✗ | On")),
            "{text}"
        );
    }

    #[test]
    fn rows_are_read_from_a_scorecards_json() {
        let json = r#"{ "schema": 2, "labels": { "judged": 3, "agree": 3, "rows": [
            { "kind": "Bars", "notes": ["fading tail not shown"], "expected": "close", "effects": 3,
              "measured": { "matches": 0, "close": 3, "different": 0, "noEquivalent": 0 },
              "measuredAlone": { "matches": 0, "close": 2, "different": 0, "noEquivalent": 0 },
              "score": 71.5 } ] } }"#;
        let rows = read(json).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].kind.as_str(), rows[0].expected),
            ("Bars", Verdict::Close)
        );
        assert_eq!(rows[0].measured_alone.close, 2);
        let checked = check(&rows);
        // A noted difference is "close" unless the table says otherwise.
        assert_eq!(
            (checked[0].table, checked[0].earned),
            (Verdict::Close, Some(Verdict::Close))
        );
        assert!(read("{}").is_err());
    }
}
