// The demo assistant's sequence: what the real assistant does with its song tools (analyze the
// song, add Beats / Bars / Sections timing, place effects section by section), scripted, so the
// browser demo shows the whole flow. Pure: it returns the edits and the draft, and changes nothing.

import type { Change, SectionSummary, TimelineView } from "./assistant";
import { formatMs } from "./memorySequencer";
import { type Analysis, EFFECT_KINDS, type Effect, type EffectKind, type Row, type Sequence, type SequenceEdit, type TimingTrack, newEffect } from "./sequence";
import type { Show } from "./types";

/** The song's sections: a quiet intro, a verse, a loud chorus, and an outro, on bar lines. */
function sections(analysis: Analysis): { label: string; startMs: number; endMs: number; level: "low" | "medium" | "high" }[] {
  const end = analysis.durationMs;
  const bars = analysis.bars.length > 4 ? analysis.bars : [0];
  const at = (fraction: number) => bars.reduce((best, b) => (Math.abs(b - end * fraction) < Math.abs(best - end * fraction) ? b : best), 0);
  const cuts = [0, at(0.125), at(0.5), at(0.875), end];
  const labels = [
    ["Intro", "low"],
    ["Mid 1", "medium"],
    ["High 1", "high"],
    ["Outro", "low"],
  ] as const;
  return labels
    .map(([label, level], i) => ({ label, level, startMs: cuts[i], endMs: cuts[i + 1] }))
    .filter((s) => s.endMs > s.startMs);
}

/** Marks spanning from each time to the next. */
function spans(times: number[], end: number, label: (i: number) => string) {
  return times.map((t, i) => ({ startMs: t, endMs: times[i + 1] ?? end, label: label(i) })).filter((m) => m.endMs > m.startMs);
}

function track(name: string, kind: TimingTrack["kind"], marks: TimingTrack["marks"]): TimingTrack {
  return { id: crypto.randomUUID(), name, kind, marks };
}

function label(kind: EffectKind): string {
  return EFFECT_KINDS.find((k) => k.kind === kind)?.label ?? kind;
}

function rowName(row: Row, show: Show): string {
  const t = row.target;
  if ("prop" in t) return show.props.find((p) => p.id === t.prop)?.name ?? "a prop";
  if ("group" in t) return `group ${show.groups.find((g) => g.id === t.group)?.name ?? ""}`.trim();
  return show.props.find((p) => p.id === t.region.prop)?.name ?? "a submodel";
}

export interface Composed {
  summary: string;
  edits: SequenceEdit[];
  draft: Sequence;
  changes: Change[];
  sections: SectionSummary[];
  timeline: TimelineView;
}

/**
 * A sequence for `doc`'s song: timing tracks, then each section in its own look (soft and sparse
 * in the intro, a sweep across the props in the verse, everything moving in the chorus, a slow
 * fade out), groups for the big washes and props for the accents.
 */
export function composeDemoSequence(doc: Sequence, show: Show, analysis: Analysis): Composed {
  const end = doc.durationMs;
  const parts = sections(analysis);
  const beats = track("Beats", "beats", spans(analysis.beats, end, (i) => String((i % 4) + 1)));
  const bars = track("Bars", "bars", spans(analysis.bars, end, (i) => String(i + 1)));
  const sectionTrack = track("Sections", "sections", parts.map((s) => ({ startMs: s.startMs, endMs: s.endMs, label: s.label })));
  const edits: SequenceEdit[] = [beats, bars, sectionTrack].map((t) => ({ type: "addTimingTrack", track: t }));
  const draft: Sequence = structuredClone(doc);
  draft.timingTracks.push(beats, bars, sectionTrack);

  const groupRows = draft.rows.filter((r) => "group" in r.target);
  const propRows = draft.rows.filter((r) => !("group" in r.target));
  const marksIn = (t: TimingTrack, from: number, to: number) => t.marks.filter((m) => m.startMs >= from && m.startMs < to);
  const place = (row: Row, layer: number, kind: EffectKind, from: number, to: number, colors: string[], fade = 0) => {
    while (row.layers.length <= layer) row.layers.push({ effects: [] });
    const effect: Effect = { ...newEffect(kind, from, Math.min(to, end), colors), fadeInMs: fade, fadeOutMs: fade };
    row.layers[layer].effects.push(effect);
    edits.push({ type: "addEffect", row: row.id, layer, effect });
  };
  const cool = ["#1e3a8a", "#ffffff"];
  const warm = ["#ffb347", "#ff6b35"];
  const festive = ["#ff1a1a", "#1aff4a", "#ffffff"];

  for (const part of parts) {
    const { startMs: from, endMs: to } = part;
    const barMarks = marksIn(bars, from, to);
    const beatMarks = marksIn(beats, from, to);
    if (part.label === "Intro") {
      for (const row of groupRows) place(row, 0, "colorWash", from, to, cool, 400);
      propRows.forEach((row, i) => barMarks.forEach((m, j) => (i + j) % 2 === 0 && place(row, 1, "twinkle", m.startMs, m.endMs, cool)));
    } else if (part.label === "Mid 1") {
      for (const row of groupRows) place(row, 0, "shimmer", from, to, warm);
      beatMarks.forEach((m, j) => {
        const row = propRows[j % Math.max(1, propRows.length)];
        if (row) place(row, 1, "on", m.startMs, m.endMs, warm);
      });
    } else if (part.label === "High 1") {
      for (const row of groupRows) barMarks.forEach((m, j) => place(row, 0, j % 2 === 0 ? "bars" : "spiral", m.startMs, m.endMs, festive));
      propRows.forEach((row, i) => barMarks.forEach((m) => place(row, 1, i % 2 === 0 ? "chase" : "meteors", m.startMs, m.endMs, festive)));
    } else {
      for (const row of groupRows) place(row, 0, "fade", from, to, cool);
      propRows.forEach((row) => place(row, 1, "twinkle", from, to, cool, 600));
    }
  }

  const added = edits.flatMap((e) => (e.type === "addEffect" ? [{ row: draft.rows.find((r) => r.id === e.row)!, effect: e.effect }] : []));
  const changes: Change[] = [
    ...[beats, bars, sectionTrack].map((t) => ({
      section: "timingTrack" as const,
      action: "added" as const,
      name: t.name,
      id: t.id,
      details: [`kind: ${t.kind}`, `marks: ${t.marks.length}`],
      warnings: [],
    })),
    ...added.map(({ row, effect }) => ({
      section: "effect" as const,
      action: "added" as const,
      name: `${label(effect.params.kind)} on ${rowName(row, show)} at ${formatMs(effect.startMs)}–${formatMs(effect.endMs)}`,
      id: effect.id,
      details: [`colors: ${effect.palette.colors.join(", ")}`],
      warnings: [],
    })),
  ];
  const summaries: SectionSummary[] = parts.map((part) => {
    const here = added.filter(({ effect }) => effect.startMs >= part.startMs && effect.startMs < part.endMs);
    return {
      label: part.label,
      startMs: part.startMs,
      endMs: part.endMs,
      rows: new Set(here.map(({ row }) => row.id)).size,
      added: here.length,
      changed: 0,
      removed: 0,
      kinds: [...new Set(here.map(({ effect }) => label(effect.params.kind)))],
    };
  });
  const lit = draft.rows.filter((r) => r.layers.some((l) => l.effects.length > 0));
  const timeline: TimelineView = {
    durationMs: end,
    sections: parts.map(({ label, startMs, endMs }) => ({ label, startMs, endMs })),
    rows: lit.slice(0, 48).map((row) => ({
      name: rowName(row, show),
      effects: row.layers
        .flatMap((l) => l.effects)
        .map((e) => ({ startMs: e.startMs, endMs: e.endMs, color: e.palette.colors[0] ?? "#ffffff" }))
        .sort((a, b) => a.startMs - b.startMs),
    })),
    moreRows: Math.max(0, lit.length - 48),
  };
  const tempo = analysis.tempoBpm ? `${Math.round(analysis.tempoBpm)} BPM ` : "";
  return {
    summary: `A ${tempo}light show for "${doc.name}": a soft blue intro, a warm sweep across the props in the verse, everything moving in red and green for the chorus, then a slow fade. Synced to the beats and bars.`,
    edits,
    draft,
    changes,
    sections: summaries,
    timeline,
  };
}
