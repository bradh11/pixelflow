// Tap timing: re-timing a lyrics track's marks by ear, in order. Each key press starts the next
// mark there (and ends the one before, if its end is still open); holding the key down and
// letting go ends the mark there instead. Labels stay with their marks; only times change.

import type { Mark } from "../api/sequence";
import type { SpanMove } from "./lyricEdits";

/** A press held at least this long sets the mark's end where it's let go. */
export const HOLD_MS = 180;

export interface TapSession {
  /** The marks being timed, in order: their place on the track. */
  queue: number[];
  /** How many of them have been started. */
  done: number;
  /** Times set so far, in queue order: a start, and an end once set (by a hold or the next press). */
  times: { startMs: number; endMs: number | null }[];
  /** When the key went down for the mark being held, if it's down. */
  downAt: number | null;
}

/** A session for the marks of `marks` (sorted) from `fromMs` on. */
export function startTapSession(marks: readonly Mark[], fromMs: number): TapSession {
  const queue = marks.flatMap((m, i) => (m.startMs >= fromMs ? [i] : []));
  return { queue, done: 0, times: [], downAt: null };
}

/** The mark the next press starts (its place on the track), or null when all are timed. */
export function nextMark(session: TapSession): number | null {
  return session.queue[session.done] ?? null;
}

/** A press at `atMs` (song time): the mark before ends here if its end is still open, and the next
 * one starts here. Nothing when every mark is timed (or a key is already down). */
export function pressTap(session: TapSession, atMs: number): TapSession {
  if (session.downAt !== null || session.done >= session.queue.length) return session;
  const times = session.times.map((t) => ({ ...t }));
  const last = times[times.length - 1];
  // Two presses within a millisecond still make two marks, one after the other.
  const at = Math.max(Math.round(atMs), last ? last.startMs + 1 : 0);
  if (last && last.endMs === null) last.endMs = at;
  times.push({ startMs: at, endMs: null });
  return { ...session, done: session.done + 1, times, downAt: at };
}

/** The key let go at `atMs`: held long enough, the mark it started ends here. */
export function releaseTap(session: TapSession, atMs: number): TapSession {
  if (session.downAt === null) return session;
  const times = session.times.map((t) => ({ ...t }));
  const held = Math.round(atMs) - session.downAt >= HOLD_MS;
  if (held) times[times.length - 1].endMs = Math.round(atMs);
  return { ...session, times, downAt: null };
}

/**
 * The track's marks once the session ends, and how each mark moved (for the tracks below to
 * follow; see lyricEdits.followEdits). The timed marks get their new times, the last one still open
 * keeping its length. The mark before them ends by the first; those after them stay put unless the
 * timed marks now run past their start, in which case they all move later together, just enough.
 * Null when nothing was timed.
 */
export function finishTapSession(session: TapSession, marks: readonly Mark[], durationMs: number, minMs = 1): { marks: Mark[]; moves: SpanMove[] } | null {
  if (session.times.length === 0) return null;
  const out = marks.map((m) => ({ ...m }));
  const timed = session.queue.slice(0, session.times.length);
  timed.forEach((index, k) => {
    const t = session.times[k];
    const original = marks[index];
    const next = session.times[k + 1];
    let endMs = t.endMs ?? t.startMs + (original.endMs - original.startMs);
    if (next) endMs = Math.min(endMs, next.startMs);
    out[index].startMs = Math.min(t.startMs, durationMs - minMs);
    out[index].endMs = Math.max(out[index].startMs + minMs, Math.min(durationMs, endMs));
  });
  const first = timed[0];
  const last = timed[timed.length - 1];
  // The mark before the first timed one ends by its start.
  const before = out[first - 1];
  if (before && before.endMs > out[first].startMs) before.endMs = Math.max(before.startMs + minMs, out[first].startMs);
  // Those after move later together if the timed marks now run into them.
  const after = out.slice(last + 1);
  if (after.length > 0) {
    const room = after[0].startMs - (out[last].startMs + minMs);
    if (room < 0) {
      for (const m of after) {
        m.startMs = Math.min(durationMs - minMs, m.startMs - room);
        m.endMs = Math.min(durationMs, Math.max(m.startMs + minMs, m.endMs - room));
      }
    }
    out[last].endMs = Math.min(out[last].endMs, after[0].startMs);
  }
  const moves: SpanMove[] = [];
  out.forEach((m, i) => {
    const was = marks[i];
    if (m.startMs !== was.startMs || m.endMs !== was.endMs) moves.push({ from: { startMs: was.startMs, endMs: was.endMs }, to: { startMs: m.startMs, endMs: m.endMs } });
  });
  return { marks: out, moves };
}
