// Editing a song's lyrics tracks together. A lyrics timing comes as a family of tracks named after
// one another: "Lyrics" (lines), "Lyrics (words)", "Lyrics (syllables)", and "Lyrics (phonemes)".
// Each line holds words, each word syllables, each syllable mouth shapes, so when a mark moves or
// changes length, the marks under it on the tracks below move and stretch with it.

import type { Mark, Sequence, SequenceEdit, TimingTrack } from "../api/sequence";

/** A lyrics track and those below it, top to bottom. */
const LEVELS = ["lines", "words", "syllables", "phonemes"] as const;
type Level = (typeof LEVELS)[number];

/** Which level `track` is in its family, and the family's name ("Lyrics" for "Lyrics (words)"). */
function levelOf(track: TimingTrack): { level: Level; base: string } | null {
  const strip = (suffix: string) => (track.name.endsWith(suffix) ? track.name.slice(0, -suffix.length) : null);
  if (track.kind === "lyrics") return { level: "lines", base: track.name };
  if (track.kind === "words") {
    const base = strip(" (words)");
    return base === null ? null : { level: "words", base };
  }
  if (track.kind === "phonemes") {
    const base = strip(" (phonemes)");
    return base === null ? null : { level: "phonemes", base };
  }
  const base = track.kind === "custom" ? strip(" (syllables)") : null;
  return base === null ? null : { level: "syllables", base };
}

/** The lyrics tracks `trackId` belongs with, by level; empty for a track that isn't one of them. */
export function lyricFamily(doc: Sequence, trackId: string): Partial<Record<Level, TimingTrack>> {
  const track = doc.timingTracks.find((t) => t.id === trackId);
  const at = track && levelOf(track);
  if (!at) return {};
  const family: Partial<Record<Level, TimingTrack>> = {};
  for (const t of doc.timingTracks) {
    const other = levelOf(t);
    if (other && other.base === at.base && !family[other.level]) family[other.level] = t;
  }
  family[at.level] = track;
  return family;
}

/** The tracks below `trackId` in its lyrics family (its words' syllables and mouth shapes, say). */
export function lyricChildren(doc: Sequence, trackId: string): TimingTrack[] {
  const track = doc.timingTracks.find((t) => t.id === trackId);
  const at = track && levelOf(track);
  if (!at) return [];
  const family = lyricFamily(doc, trackId);
  return LEVELS.slice(LEVELS.indexOf(at.level) + 1).flatMap((level) => family[level] ?? []);
}

/** Whether `track` can be re-timed by tapping: a words or syllables track with marks. */
export function canTapTime(track: TimingTrack): boolean {
  const level = levelOf(track)?.level;
  return (level === "words" || level === "syllables" || track.kind === "words") && track.marks.length > 0;
}

/** The line a mark of `trackId` is sung in (the lyrics line its start falls in), or the mark
 * itself when there's no line around it. */
export function lineSpan(doc: Sequence, trackId: string, mark: { startMs: number; endMs: number }): { startMs: number; endMs: number } {
  const lines = lyricFamily(doc, trackId).lines;
  const line = lines?.marks.find((l) => l.startMs <= mark.startMs && mark.startMs < l.endMs);
  return line ? { startMs: line.startMs, endMs: line.endMs } : { startMs: mark.startMs, endMs: mark.endMs };
}

/** A mark moved or stretched: where it was and where it goes. */
export interface SpanMove {
  from: { startMs: number; endMs: number };
  to: { startMs: number; endMs: number };
}

/** `marks` (sorted) made tidy again: none overlapping the next (its end cut back to the next's
 * start), none empty. */
function tidy(marks: Mark[]): Mark[] {
  const sorted = [...marks].sort((a, b) => a.startMs - b.startMs);
  const out: Mark[] = [];
  for (let i = 0; i < sorted.length; i++) {
    const m = { ...sorted[i] };
    const next = sorted[i + 1];
    if (next && m.endMs > next.startMs) m.endMs = next.startMs;
    if (m.endMs > m.startMs) out.push(m);
  }
  return out;
}

/**
 * The edits that carry the marks under moved marks of `trackId` along on the tracks below it:
 * each mark below whose middle lies in a moved mark's old span lands at the same place within its
 * new span (stretched with it). One edit per track below that changes; none when nothing does.
 */
export function followEdits(doc: Sequence, trackId: string, moves: SpanMove[]): SequenceEdit[] {
  const moved = moves.filter((m) => m.from.startMs !== m.to.startMs || m.from.endMs !== m.to.endMs);
  if (moved.length === 0) return [];
  const edits: SequenceEdit[] = [];
  for (const child of lyricChildren(doc, trackId)) {
    let changed = false;
    const marks = child.marks.map((mark) => {
      const middle = (mark.startMs + mark.endMs) / 2;
      const move = moved.find((m) => m.from.startMs <= middle && middle < m.from.endMs);
      if (!move) return mark;
      const scale = (move.to.endMs - move.to.startMs) / Math.max(1, move.from.endMs - move.from.startMs);
      const at = (t: number) => Math.round(move.to.startMs + (t - move.from.startMs) * scale);
      const startMs = Math.max(move.to.startMs, at(mark.startMs));
      const endMs = Math.min(move.to.endMs, Math.max(startMs + 1, at(mark.endMs)));
      if (startMs === mark.startMs && endMs === mark.endMs) return mark;
      changed = true;
      return { ...mark, startMs, endMs };
    });
    if (changed) edits.push({ type: "updateTimingTrack", track: { ...child, marks: tidy(marks) } });
  }
  return edits;
}

/** Snap targets with the voice's onsets added (sorted, each once). */
export function withOnsets(targets: number[], onsets: readonly number[] | null | undefined): number[] {
  if (!onsets || onsets.length === 0) return targets;
  return [...new Set([...targets, ...onsets])].sort((a, b) => a - b);
}
