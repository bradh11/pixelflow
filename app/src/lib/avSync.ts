// Keeping the app's preview in step with what is heard. The music plays in the engine; the window
// asks where it is now and then (each answer a little old by the time it lands), asks for frames
// (which take a moment to come back), and paints them (which shows on the next screen refresh).
// So the window keeps its own estimate of where the music is, carried on between answers, and
// draws the moment that will be heard when the picture reaches the screen, moved by the user's
// preview sync offset. None of this touches what controllers, FPP, or exports get.

/** The preview sync offset's range either way (ms). */
export const SYNC_RANGE_MS = 300;

/** Where the music was (ms) at `atMs` (a `performance.now()` time), and how it moves from there. */
export interface ClockReading {
  musicMs: number;
  atMs: number;
  /** 1 as written, 0.5 at half speed. */
  speed: number;
  running: boolean;
}

/** A reading from an answer to a question sent at `sentAt` and answered at `gotAt`: the engine
 * looked about halfway between. */
export function readingFrom(musicMs: number, speed: number, running: boolean, sentAt: number, gotAt: number): ClockReading {
  return { musicMs, atMs: (sentAt + gotAt) / 2, speed, running };
}

/** Where the music is at `t` (a `performance.now()` time), going by `reading`. */
export function musicAt(reading: ClockReading, t: number): number {
  return reading.running ? reading.musicMs + (t - reading.atMs) * reading.speed : reading.musicMs;
}

/** Readings this close to where the last one says the music should be are the same playback,
 * only a little jittery; further off, the music jumped (a seek, a loop) or stalled. */
const SAME_PLAYBACK_MS = 40;
/** How much of the gap a new reading of the same playback closes. */
const SETTLE = 0.25;

/** The estimate after a new reading: a reading of the same playback nudges it (so the jitter of
 * each answer doesn't shake the playhead); anything else (a jump, a pause, a new speed) replaces it. */
export function nextReading(previous: ClockReading | null, next: ClockReading): ClockReading {
  if (!previous || !previous.running || !next.running || previous.speed !== next.speed) return next;
  const expected = musicAt(previous, next.atMs);
  const gap = next.musicMs - expected;
  if (Math.abs(gap) > SAME_PLAYBACK_MS) return next;
  return { ...next, musicMs: expected + gap * SETTLE };
}

/** The moment the preview shows for the music at `musicMs`: moved by the user's offset (positive:
 * the picture runs ahead), within the song. */
export function previewPosition(musicMs: number, offsetMs: number, durationMs: number): number {
  return Math.max(0, Math.min(durationMs, musicMs + offsetMs));
}

/** A running average: `previous` moved `weight` of the way to `sample`. */
export function smooth(previous: number | null, sample: number, weight = 0.2): number {
  return previous === null ? sample : previous + (sample - previous) * weight;
}

/** What a preview frame asked for at `t` should show: the music when it reaches the screen (`fetchMs`
 * to come back, `paintMs` more to be painted), moved by the offset. */
export function frameTarget(reading: ClockReading, t: number, fetchMs: number, paintMs: number, offsetMs: number, durationMs: number): number {
  return previewPosition(musicAt(reading, t + fetchMs + paintMs), offsetMs, durationMs);
}

// --- Tap along -------------------------------------------------------------------------------

/** How far a tap at `tapMs` (metronome time) is from the nearest click (clicks every `intervalMs`,
 * the first at 0): positive when the tap comes after it. */
export function tapLag(tapMs: number, intervalMs: number): number {
  const into = ((tapMs % intervalMs) + intervalMs) % intervalMs;
  return into > intervalMs / 2 ? into - intervalMs : into;
}

/** Taps needed before a suggestion. */
export const MIN_TAPS = 6;
/** A tap further than this from the others' middle is a slip, left out. */
const SLIP_MS = 80;

export interface TapResult {
  /** The offset that lines the picture up with what was heard (ms, within the range). */
  offsetMs: number;
  /** How late the taps came after the clicks on average (ms). */
  lagMs: number;
  /** How much the kept taps wandered (standard deviation, ms). */
  spreadMs: number;
  kept: number;
  left: number;
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

/**
 * The offset tapping along suggests, from taps (metronome time, ms) on clicks every `intervalMs`:
 * the taps' average lag behind the clicks, leaving out slips (taps far from the others' middle:
 * more than three times their usual distance from it, and more than a few ms). Taps that come late
 * mean the sound is heard later than the engine counts it, so the picture should run that much
 * behind. Null with fewer than MIN_TAPS kept.
 */
export function calibrateTaps(tapsMs: number[], intervalMs: number): TapResult | null {
  const lags = tapsMs.map((t) => tapLag(t, intervalMs));
  if (lags.length < MIN_TAPS) return null;
  const middle = median(lags);
  const usual = median(lags.map((l) => Math.abs(l - middle))) * 1.4826;
  const limit = Math.min(SLIP_MS, Math.max(15, 3 * usual));
  const kept = lags.filter((l) => Math.abs(l - middle) <= limit);
  if (kept.length < MIN_TAPS) return null;
  const lagMs = kept.reduce((a, b) => a + b, 0) / kept.length;
  const spreadMs = Math.sqrt(kept.reduce((a, l) => a + (l - lagMs) ** 2, 0) / kept.length);
  const offsetMs = Math.max(-SYNC_RANGE_MS, Math.min(SYNC_RANGE_MS, Math.round(-lagMs)));
  return { offsetMs, lagMs, spreadMs, kept: kept.length, left: lags.length - kept.length };
}

/** When an input event happened (a `performance.now()` time): its own time stamp, which doesn't
 * include how long it waited to be handled, where that is on the same clock. */
export function eventTime(e: { timeStamp: number }): number {
  const now = performance.now();
  return Math.abs(now - e.timeStamp) < 1000 ? Math.min(now, e.timeStamp) : now;
}

/** Calls `step` on every screen refresh (or about 60 times a second where there are none) until
 * the returned function is called. */
export function everyFrame(step: (t: number) => void): () => void {
  let stopped = false;
  if (typeof requestAnimationFrame === "function") {
    let id = 0;
    const tick = (t: number) => {
      if (stopped) return;
      step(t);
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => {
      stopped = true;
      cancelAnimationFrame(id);
    };
  }
  const timer = setInterval(() => step(performance.now()), 16);
  return () => clearInterval(timer);
}
