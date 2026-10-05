// Timing marks, worked out the way the engine does (pf-sequence's timing.rs): marks on a track in
// order without overlaps, marks at a fixed interval or from every Nth mark of another track, lyrics
// spread by letter count, and phrases broken into words. The in-memory sequencer applies timing
// edits with these, so the browser demo and the tests behave like the desktop app; a shared table
// of cases (timingEditCases.json) is checked against both.

import type { Mark, TimingTrack } from "./sequence";

/** Closest two generated marks may be. */
export const MIN_MARK_INTERVAL_MS = 10;
/** Most timing marks in a sequence. */
export const MAX_MARKS = 500_000;

/** Like the engine's format_ms: 1:02.500, or 1:02:03.000 past an hour. */
export function formatMs(ms: number): string {
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  const milli = String(ms % 1000).padStart(3, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}.${milli}` : `${m}:${ss}.${milli}`;
}

/** True when two marks share some time (touching marks don't overlap). */
export function marksOverlap(a: { startMs: number; endMs: number }, b: { startMs: number; endMs: number }): boolean {
  return a.startMs < b.endMs && b.startMs < a.endMs;
}

/** A refused timing change, explained. */
export class TimingError extends Error {}

function fail(message: string): never {
  throw new TimingError(message);
}

export function checkMark(mark: Mark) {
  if (mark.endMs <= mark.startMs) fail("A mark must end after it starts.");
}

/** Where a mark starting at `startMs` goes to keep the marks in order (after any starting then). */
export function insertIndex(marks: Mark[], startMs: number): number {
  let lo = 0;
  let hi = marks.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (marks[mid].startMs <= startMs) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** The first mark (by index) overlapping `mark`, skipping the marks at `skip`; -1 when none. */
export function overlapWith(marks: Mark[], mark: { startMs: number; endMs: number }, skip: number[] = []): number {
  return marks.findIndex((m, i) => !skip.includes(i) && marksOverlap(m, mark));
}

export function overlapMessage(track: TimingTrack, other: Mark): string {
  return `That would overlap the mark at ${formatMs(other.startMs)} on '${track.name}'; marks on a timing track can't overlap.`;
}

/** Adds `marks` to `track` in place, each where it belongs; nothing changes when one is refused. */
export function addMarks(track: TimingTrack, marks: Mark[]) {
  const next = [...track.marks];
  for (const mark of marks) {
    checkMark(mark);
    const at = overlapWith(next, mark);
    if (at >= 0) fail(overlapMessage(track, next[at]));
    next.splice(insertIndex(next, mark.startMs), 0, { ...mark });
  }
  track.marks = next;
}

/** Takes out every mark of `track` sharing time with `fromMs..toMs`. */
export function clearRange(track: TimingTrack, fromMs: number, toMs: number) {
  track.marks = track.marks.filter((m) => !marksOverlap(m, { startMs: fromMs, endMs: toMs }));
}

function checkRange(fromMs: number, toMs: number) {
  if (toMs <= fromMs) fail("Choose a time range that ends after it starts.");
}

function checkCount(count: number) {
  if (count > MAX_MARKS) fail(`That would make ${count} marks; at most ${MAX_MARKS} are allowed.`);
}

/** A mark every `everyMs` from `fromMs` to `toMs` (the last one ends at `toMs`). */
export function fixedMarks(everyMs: number, fromMs: number, toMs: number): Mark[] {
  if (everyMs < MIN_MARK_INTERVAL_MS) fail(`Marks must be at least ${MIN_MARK_INTERVAL_MS} ms apart.`);
  checkRange(fromMs, toMs);
  checkCount(Math.ceil((toMs - fromMs) / everyMs));
  const marks: Mark[] = [];
  for (let start = fromMs; start < toMs; start += everyMs) marks.push({ startMs: start, endMs: Math.min(start + everyMs, toMs), label: "" });
  return marks;
}

/** Every `every`th mark of `source`, each lasting until the next one taken; labels come along. */
export function everyNthMark(source: Mark[], every: number): Mark[] {
  if (every < 1) fail("Take every 1st, 2nd, 3rd… mark: the step must be at least 1.");
  const marks: Mark[] = [];
  for (let i = 0; i < source.length; i += every) {
    const next = i + every;
    const endMs = next < source.length ? source[next].startMs : source[Math.min(i + every - 1, source.length - 1)].endMs;
    if (endMs > source[i].startMs) marks.push({ startMs: source[i].startMs, endMs, label: source[i].label });
  }
  return marks;
}

/** How much time a piece of text gets: its letters and digits (at least one). */
function weight(text: string): number {
  return Math.max(1, (text.match(/[\p{L}\p{N}]/gu) ?? []).length);
}

/** `fromMs..toMs` in one span per weight, by share, each at least 1 ms; null when too short. */
function divide(fromMs: number, toMs: number, weights: number[]): [number, number][] | null {
  const n = weights.length;
  const length = toMs - fromMs;
  if (n === 0 || length < n) return null;
  const total = weights.reduce((a, b) => a + b, 0);
  const spans: [number, number][] = [];
  let start = fromMs;
  let sum = 0;
  weights.forEach((w, i) => {
    sum += w;
    const left = n - 1 - i;
    const ideal = fromMs + Math.floor((length * sum + Math.floor(total / 2)) / total);
    const end = left === 0 ? toMs : Math.min(toMs - left, Math.max(start + 1, ideal));
    spans.push([start, end]);
    start = end;
  });
  return spans;
}

/** The lines of pasted lyrics: one phrase per line, trimmed, blank lines left out. */
export function lyricLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

/** One mark per phrase over `fromMs..toMs`, back to back, each as long as its share of the letters. */
export function spreadPhrases(lines: string[], fromMs: number, toMs: number): Mark[] {
  const kept = lines.map((l) => l.trim()).filter((l) => l.length > 0);
  if (kept.length === 0) fail("Paste at least one line of lyrics.");
  checkRange(fromMs, toMs);
  checkCount(kept.length);
  const spans = divide(fromMs, toMs, kept.map(weight)) ?? fail(`${formatMs(toMs - fromMs)} is too short for ${kept.length} lines of lyrics.`);
  return spans.map(([startMs, endMs], i) => ({ startMs, endMs, label: kept[i] }));
}

/** One mark per word of a phrase (its label split on spaces), sharing its time by letter count. */
export function splitWords(phrase: Mark): Mark[] {
  const words = phrase.label.split(/\s+/).filter((w) => w.length > 0);
  if (words.length === 0) return [];
  const spans =
    divide(phrase.startMs, phrase.endMs, words.map(weight)) ??
    fail(`The phrase at ${formatMs(phrase.startMs)} is too short to split into ${words.length} words.`);
  return spans.map(([startMs, endMs], i) => ({ startMs, endMs, label: words[i] }));
}
