//! A benchmark's results as JSON and as Markdown tables.

use crate::effects::Breakdown;
use crate::measure::{Mode, RunResult};
use crate::pipeline::Wiring;
use crate::sequence::SequenceSummary;
use crate::show::{Category, Composition, tier_label};
use crate::stats::Spread;
use crate::system::Machine;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

/// What every report says first.
pub const CAVEAT: &str = "Synthetic shows and sequences, sent to the loopback interface or to nothing at all. \
     No real network, controller, or screen took part.";

/// One tier's show, and what it puts on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowInfo {
    pub tier: String,
    pub composition: Composition,
    pub wiring: Wiring,
}

/// One synthetic sequence on one tier's show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceInfo {
    pub tier: String,
    pub summary: SequenceSummary,
}

/// Everything a benchmark found, with what it ran on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub caveat: String,
    /// When the report was made (UTC).
    pub generated: String,
    pub machine: Machine,
    pub shows: Vec<ShowInfo>,
    pub sequences: Vec<SequenceInfo>,
    pub runs: Vec<RunResult>,
    pub effects: Vec<Breakdown>,
}

/// `seconds` since 1970 as `2026-10-10 21:30 UTC`.
fn utc(seconds: u64) -> String {
    let (days, of_day) = (seconds / 86_400, seconds % 86_400);
    // Days to a date (Howard Hinnant's `civil_from_days`).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        of_day / 3600,
        of_day % 3600 / 60
    )
}

impl Report {
    /// An empty report from this machine, now.
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self {
            caveat: CAVEAT.to_string(),
            generated: utc(now),
            machine: Machine::here(),
            shows: Vec::new(),
            sequences: Vec::new(),
            runs: Vec::new(),
            effects: Vec::new(),
        }
    }

    /// Adds what `other` found (each show and sequence once).
    pub fn merge(&mut self, other: Report) {
        for show in other.shows {
            if !self.shows.iter().any(|s| s.tier == show.tier) {
                self.shows.push(show);
            }
        }
        for sequence in other.sequences {
            let same =
                |s: &SequenceInfo| s.tier == sequence.tier && s.summary.variant == sequence.summary.variant;
            if !self.sequences.iter().any(same) {
                self.sequences.push(sequence);
            }
        }
        self.runs.extend(other.runs);
        self.effects.extend(other.effects);
    }

    pub fn json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a report is plain data")
    }

    pub fn markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# PixelFlow scale benchmark\n\n**{}**\n", self.caveat);
        let _ = writeln!(out, "Generated {}.\n", self.generated);
        self.machine_table(&mut out);
        self.show_tables(&mut out);
        self.run_tables(&mut out);
        for breakdown in &self.effects {
            effect_tables(&mut out, breakdown);
        }
        out
    }

    fn machine_table(&self, out: &mut String) {
        let m = &self.machine;
        let memory = m.memory_gb.map_or("unknown".to_string(), |gb| format!("{gb} GB"));
        let rows = vec![
            vec!["CPU".to_string(), m.cpu.clone()],
            vec!["Cores".to_string(), m.cores.to_string()],
            vec!["Memory".to_string(), memory],
            vec!["OS".to_string(), m.os.clone()],
            vec!["Compiler".to_string(), m.rustc.clone()],
            vec!["Build".to_string(), format!("{}, {}", m.profile, m.target)],
        ];
        table(out, "Machine", &["", ""], rows);
    }

    fn show_tables(&self, out: &mut String) {
        let rows = self
            .shows
            .iter()
            .map(|show| {
                let c = &show.composition;
                let of = |category: Category| {
                    let count = c.categories.iter().find(|count| count.category == category);
                    count.map_or("0".to_string(), |n| {
                        format!("{} in {}", thousands(n.pixels), n.props)
                    })
                };
                let matrices: Vec<String> = c
                    .matrices
                    .iter()
                    .map(|m| format!("{} of {}x{}", m.count, m.columns, m.rows))
                    .collect();
                vec![
                    show.tier.clone(),
                    thousands(c.pixels),
                    c.props.to_string(),
                    of(Category::Outline),
                    of(Category::Arch),
                    of(Category::Tree),
                    of(Category::Accent),
                    of(Category::Matrix),
                    matrices.join(", "),
                ]
            })
            .collect();
        let headers = [
            "Tier",
            "Pixels",
            "Props",
            "Outlines and lines",
            "Arches",
            "Trees",
            "Stars, circles, canes",
            "Matrices",
            "Matrix sizes",
        ];
        table(out, "Shows (pixels in props)", &headers, rows);

        let rows = self
            .shows
            .iter()
            .map(|show| {
                let (c, w) = (&show.composition, &show.wiring);
                vec![
                    show.tier.clone(),
                    c.controllers.to_string(),
                    format!("{} px on {}", thousands(c.sacn_pixels), c.sacn_controllers),
                    w.universes.to_string(),
                    format!("{} px on {}", thousands(c.ddp_pixels), c.ddp_controllers),
                    (w.packets_per_pass - w.universes).to_string(),
                    w.packets_per_pass.to_string(),
                    format!("{:.0} KB", w.bytes_per_pass as f64 / 1024.0),
                    c.groups
                        .iter()
                        .map(|g| format!("{} ({})", g.name, g.members))
                        .collect::<Vec<_>>()
                        .join(", "),
                ]
            })
            .collect();
        let headers = [
            "Tier",
            "Controllers",
            "sACN",
            "Universes",
            "DDP",
            "DDP packets",
            "Packets a pass",
            "Bytes a pass",
            "Groups (members)",
        ];
        table(out, "Controllers and groups", &headers, rows);

        let rows = self
            .sequences
            .iter()
            .map(|sequence| {
                let s = &sequence.summary;
                vec![
                    sequence.tier.clone(),
                    s.variant.label().to_string(),
                    format!("{} s", s.seconds),
                    s.rows.to_string(),
                    format!("{} (up to {})", s.layered_rows, s.most_layers),
                    s.effects.to_string(),
                    format!("{:.0} / {}", s.effects_per_frame, s.most_effects_per_frame),
                    format!("{:.2} / {:.2}", s.layers_per_pixel, s.most_layers_per_pixel),
                    s.with_blur.to_string(),
                    s.with_sparkles.to_string(),
                    s.with_curves.to_string(),
                    s.with_fades.to_string(),
                ]
            })
            .collect();
        let headers = [
            "Tier",
            "Sequence",
            "Length",
            "Rows",
            "Layered rows",
            "Effects",
            "Effects a frame (mean / most)",
            "Layers a pixel (mean / most)",
            "Blur",
            "Sparkles",
            "Curves",
            "Fades",
        ];
        table(out, "Sequences", &headers, rows);
    }

    fn run_tables(&self, out: &mut String) {
        let of = |mode: Mode| -> Vec<&RunResult> { self.runs.iter().filter(|r| r.mode == mode).collect() };
        let named = |r: &RunResult| {
            vec![
                r.tier.clone(),
                r.variant.label().to_string(),
                r.sink.label().to_string(),
            ]
        };
        let cores = |r: &RunResult| {
            r.resources
                .cpu_cores
                .map_or("?".to_string(), |c| format!("{c:.2}"))
        };
        let megabytes = |mb: Option<f64>| mb.map_or("?".to_string(), |mb| format!("{mb:.0} MB"));
        let whole = |n: Option<f64>| n.map_or("?".to_string(), |n| format!("{n:.0}"));

        let paced = of(Mode::Paced);
        let rows = paced
            .iter()
            .map(|r| {
                let p = r.pacing.unwrap_or_default();
                let mut row = named(r);
                row.extend([
                    format!("{:.2}", r.fps),
                    format!("{} of {}", r.frames.dropped, r.frames.due),
                    quartet(&r.stages.player),
                    quartet(&r.stages.output_pass),
                    quartet(&p.shown),
                    r.frames.late.to_string(),
                    format!("{:.2}%", r.verdict.on_time_percent),
                    quartet(&p.latency),
                    format!("{:.2}", p.jitter),
                    cores(r),
                    megabytes(r.resources.peak_rss_mb),
                    verdict(r),
                ]);
                row
            })
            .collect();
        let headers = [
            "Tier",
            "Sequence",
            "Sink",
            "FPS",
            "Dropped",
            "Player: one frame, ms (p50 / p95 / p99 / max)",
            "Output: one pass, ms (p50 / p95 / p99 / max)",
            "Handed over after due, ms (p50 / p95 / p99 / max)",
            "Late",
            "On time",
            "On the wire after due, ms (p50 / p95 / p99 / max)",
            "Jitter ms",
            "CPU cores",
            "Peak RSS",
            "40 FPS",
        ];
        table(out, "In real time: 40 frames a second", &headers, rows);

        let rows = paced
            .iter()
            .map(|r| {
                let p = r.pacing.unwrap_or_default();
                let mut row = named(r);
                row.extend([
                    quartet(&r.stages.render),
                    pair(&r.stages.preview),
                    pair(&r.stages.map),
                    pair(&r.stages.packetize),
                    quartet(&r.stages.send),
                    format!("{:.1} of {}", p.passes_per_second, r.send_hz.unwrap_or(0)),
                    p.late_passes.to_string(),
                    quartet(&p.start_delay),
                    pair(&r.preview_poll),
                ]);
                row
            })
            .collect();
        let headers = [
            "Tier",
            "Sequence",
            "Sink",
            "Render (p50 / p95 / p99 / max)",
            "Preview copies (p50 / p99)",
            "Map (p50 / p99)",
            "Packetize (p50 / p99)",
            "Send (p50 / p95 / p99 / max)",
            "Passes a second",
            "Late passes",
            "Draw starts after due (p50 / p95 / p99 / max)",
            "Preview read (p50 / p99)",
        ];
        table(out, "In real time: each stage, in milliseconds", &headers, rows);

        let rows = of(Mode::Asap)
            .iter()
            .map(|r| {
                let mut row = named(r);
                row.extend([
                    format!("{:.0}", r.fps),
                    quartet(&r.stages.render),
                    pair(&r.stages.preview),
                    pair(&r.stages.map),
                    pair(&r.stages.packetize),
                    pair(&r.stages.send),
                    quartet(&r.stages.frame),
                    format!("{} of {}", r.frames.late, r.frames.due),
                    cores(r),
                    whole(r.resources.render_allocations_per_frame),
                    whole(r.resources.output_allocations_per_pass),
                ]);
                row
            })
            .collect();
        let headers = [
            "Tier",
            "Sequence",
            "Sink",
            "FPS",
            "Render (p50 / p95 / p99 / max)",
            "Preview copies (p50 / p99)",
            "Map (p50 / p99)",
            "Packetize (p50 / p99)",
            "Send (p50 / p99)",
            "Whole frame (p50 / p95 / p99 / max)",
            "Frames over 25 ms",
            "CPU cores",
            "Allocations drawing a frame",
            "Allocations sending it",
        ];
        table(out, "Flat out: each stage, in milliseconds", &headers, rows);

        let rows = self
            .runs
            .iter()
            .map(|r| {
                let (n, res) = (&r.network, &r.resources);
                let count = |n: Option<f64>| n.map_or("-".to_string(), |n| format!("{n:.0}"));
                // The engine's frames can't be timed or read from outside.
                let seen = |text: String| {
                    if r.mode == Mode::Engine {
                        "-".to_string()
                    } else {
                        text
                    }
                };
                vec![
                    r.tier.clone(),
                    r.variant.label().to_string(),
                    r.mode.label().to_string(),
                    r.sink.label().to_string(),
                    format!("{:.0}", n.packets_per_second),
                    format!("{:.0}", n.megabits_per_second),
                    format!("{} / {}", n.send_buffer_full, n.send_failures + n.refused),
                    n.lost_percent.map_or("-".to_string(), |p| format!("{p:.2}%")),
                    format!(
                        "{} / {}",
                        res.pipeline_threads,
                        res.process_threads.map_or("?".to_string(), |n| n.to_string())
                    ),
                    megabytes(res.peak_rss_mb),
                    megabytes(res.peak_heap_mb),
                    count(res.allocations_per_frame),
                    count(res.allocated_kb_per_frame),
                    seen(format!("{:.1}", r.warmup.first_frame)),
                    seen(format!("{:.1} / {:.1}", r.warmup.mean, r.warmup.max)),
                    seen(format!("{:.0}%", r.lit_percent)),
                    r.load_average.map_or("?".to_string(), |l| format!("{l:.1}")),
                ]
            })
            .collect();
        let headers = [
            "Tier",
            "Sequence",
            "Mode",
            "Sink",
            "Packets/s",
            "Mbit/s",
            "Send buffer full / failed",
            "Lost on loopback",
            "Threads (pipeline / process)",
            "Peak RSS",
            "Peak heap",
            "Allocations a frame",
            "KB allocated a frame",
            "First frame ms",
            "Warm-up ms (mean / max)",
            "Channels lit",
            "Load average",
        ];
        table(out, "Network, memory, and warm-up", &headers, rows);

        let rows = of(Mode::Engine)
            .iter()
            .map(|r| {
                let p = r.pacing.unwrap_or_default();
                let mut row = named(r);
                row.extend([
                    format!("{:.2}", r.fps),
                    format!("{} of {}", r.frames.dropped, r.frames.due),
                    r.frames.late.to_string(),
                    quartet(&p.shown),
                    format!("{:.1} of {}", p.passes_per_second, r.send_hz.unwrap_or(0)),
                    cores(r),
                    megabytes(r.resources.peak_rss_mb),
                    verdict(r),
                ]);
                row
            })
            .collect();
        let headers = [
            "Tier",
            "Sequence",
            "Sink",
            "FPS",
            "Dropped",
            "Late",
            "Drawn and handed over after due, ms (p50 / p95 / p99 / max)",
            "Passes a second",
            "CPU cores",
            "Peak RSS",
            "40 FPS",
        ];
        table(out, "The engine itself, watched from outside", &headers, rows);

        let mut notes: Vec<&String> = self.runs.iter().flat_map(|r| &r.notes).collect();
        notes.sort();
        notes.dedup();
        for note in notes {
            let _ = writeln!(out, "- {note}");
        }
        out.push('\n');
    }
}

impl Default for Report {
    fn default() -> Self {
        Self::new()
    }
}

/// A run's verdict in a word or two.
pub fn verdict(run: &RunResult) -> String {
    let v = &run.verdict;
    if v.pass {
        return "PASS".to_string();
    }
    let mut why = Vec::new();
    if !v.sustained {
        why.push("frames late or dropped");
    }
    if !v.within_budget {
        why.push("over budget");
    }
    if !v.clean {
        why.push("packets dropped");
    }
    format!("FAIL: {}", why.join(", "))
}

fn effect_tables(out: &mut String, b: &Breakdown) {
    let _ = writeln!(
        out,
        "## Effects at {} ({} pixels)\n\nAn empty frame (clearing and writing every pixel) takes {} ms; every figure below is on top of that.\n",
        b.tier,
        thousands(b.pixels),
        millis(b.empty_frame_ms)
    );
    let count = |n: Option<f64>| n.map_or("?".to_string(), |n| format!("{n:.0}"));
    let mut kinds = b.kinds.clone();
    kinds.sort_by(|x, y| y.on_props_ms.total_cmp(&x.on_props_ms));
    let rows = kinds
        .iter()
        .map(|k| {
            vec![
                k.kind.clone(),
                millis(k.on_props_ms),
                format!("{:.1}", k.on_props_ns),
                count(k.on_props_allocations),
                count(k.on_props_kb),
                millis(k.on_house_ms),
                format!("{:.1}", k.on_house_ns),
            ]
        })
        .collect();
    let headers = [
        "Effect",
        "On every prop, ms a frame",
        "ns a pixel",
        "Allocations a frame",
        "KB allocated a frame",
        "On the whole-house group, ms a frame",
        "ns a pixel",
    ];
    table(out, "Each kind alone", &headers, rows);
    let rows = b
        .extras
        .iter()
        .map(|e| vec![e.what.clone(), millis(e.ms), format!("{:.1}", e.ns)])
        .collect();
    table(
        out,
        "Blur, sparkles, and curves (on every prop)",
        &["Effect", "ms a frame", "ns a pixel"],
        rows,
    );
    for s in &b.sequences {
        let mut rows: Vec<Vec<String>> = s
            .kinds
            .iter()
            .map(|k| {
                vec![
                    k.kind.clone(),
                    k.effects.to_string(),
                    format!("{:.3}", k.layers_per_pixel),
                    millis(k.ms),
                    format!("{:.0}%", k.percent),
                    format!("{:.1}", k.ns),
                ]
            })
            .collect();
        let summed: f64 = s.kinds.iter().map(|k| k.ms).sum();
        rows.push(vec![
            "**All kinds, added up**".to_string(),
            s.kinds.iter().map(|k| k.effects).sum::<usize>().to_string(),
            format!("{:.3}", s.kinds.iter().map(|k| k.layers_per_pixel).sum::<f64>()),
            millis(summed),
            "100%".to_string(),
            String::new(),
        ]);
        let title = format!(
            "The {} sequence, first {} s: {} ms a frame",
            s.variant.label(),
            s.seconds,
            millis(s.ms)
        );
        let headers = [
            "Effect",
            "Effects",
            "Layers a pixel",
            "Alone, ms a frame",
            "Share",
            "ns a pixel drawn",
        ];
        table(out, &title, &headers, rows);
        let _ = writeln!(
            out,
            "Prop rows alone take {} ms a frame and group rows alone {} ms. Of the whole sequence's time, blur accounts for {} ms a frame, sparkles for {} ms, and curves on other settings for {} ms (what the sequence saves without them). Drawing it allocates {} times a frame, {} KB in all.\n",
            millis(s.prop_rows_ms),
            millis(s.group_rows_ms),
            millis(s.blur_ms),
            millis(s.sparkles_ms),
            millis(s.curves_ms),
            count(s.allocations),
            count(s.kb),
        );
    }
}

/// A Markdown table under a heading (nothing when it has no rows).
fn table(out: &mut String, title: &str, headers: &[&str], rows: Vec<Vec<String>>) {
    if rows.is_empty() {
        return;
    }
    let _ = writeln!(out, "## {title}\n");
    let _ = writeln!(out, "| {} |", headers.join(" | "));
    let _ = writeln!(out, "|{}|", vec![" --- "; headers.len()].join("|"));
    for row in rows {
        let _ = writeln!(out, "| {} |", row.join(" | "));
    }
    out.push('\n');
}

/// Milliseconds to three figures or so: 0.016, 0.22, 5.21, 20.4.
fn millis(ms: f64) -> String {
    if ms.abs() < 0.1 {
        format!("{ms:.3}")
    } else if ms.abs() < 10.0 {
        format!("{ms:.2}")
    } else {
        format!("{ms:.1}")
    }
}

fn quartet(s: &Spread) -> String {
    format!(
        "{} / {} / {} / {}",
        millis(s.p50),
        millis(s.p95),
        millis(s.p99),
        millis(s.max)
    )
}

fn pair(s: &Spread) -> String {
    format!("{} / {}", millis(s.p50), millis(s.p99))
}

/// 200000 as "200,000".
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A show's entry for a report.
pub fn show_info(composition: Composition, wiring: Wiring) -> ShowInfo {
    ShowInfo {
        tier: tier_label(composition.pixels),
        composition,
        wiring,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_numbers_read_as_people_write_them() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc(1_791_667_740), "2026-10-10 21:29 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(thousands(200_000), "200,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(999), "999");
        assert_eq!(
            [millis(0.0164), millis(5.0), millis(12.345)],
            ["0.016", "5.00", "12.3"]
        );
    }

    #[test]
    fn an_empty_report_says_what_it_is_and_has_no_empty_tables() {
        let report = Report::new();
        let markdown = report.markdown();
        assert!(markdown.contains(CAVEAT));
        assert!(markdown.contains("## Machine"));
        assert!(!markdown.contains("## In real time"));
        let back: Report = serde_json::from_str(&report.json()).unwrap();
        assert_eq!(back, report);
    }
}
