// Preview sync on this computer: the user's offset (kept here: each computer and sound output has
// its own delay), what the window measures about its own delays, and where the playing music is
// now (see lib/avSync). It moves only the app's preview and the timeline's playhead.

import { create } from "zustand";
import { type ClockReading, SYNC_RANGE_MS, musicAt, nextReading, smooth } from "../lib/avSync";

const OFFSET_KEY = "pixelflow.previewSyncMs";

function loadOffset(): number {
  try {
    const value = Number(localStorage.getItem(OFFSET_KEY));
    return Number.isFinite(value) ? clampOffset(value) : 0;
  } catch {
    return 0;
  }
}

function saveOffset(ms: number) {
  try {
    localStorage.setItem(OFFSET_KEY, String(ms));
  } catch {
    // Storage unavailable: the offset still applies until the app closes.
  }
}

export function clampOffset(ms: number): number {
  return Math.round(Math.max(-SYNC_RANGE_MS, Math.min(SYNC_RANGE_MS, ms)));
}

interface PreviewSyncState {
  /** Positive: the picture runs ahead of the engine's count of what's heard. */
  offsetMs: number;
  setOffset(ms: number): void;
}

export const usePreviewSync = create<PreviewSyncState>((set) => ({
  offsetMs: loadOffset(),
  setOffset: (ms) => {
    const offsetMs = clampOffset(ms);
    set({ offsetMs });
    saveOffset(offsetMs);
  },
}));

/** Measurements this many apart are logged (one line, for the developer console). */
const LOG_EVERY = 200;

/** The window's own delays, measured as it plays (ms, running averages; not state, as they change
 * many times a second). */
export const pipeline = {
  /** Asking the engine where the music is, there and back. */
  askMs: null as number | null,
  /** Asking for a preview frame until it's there. */
  fetchMs: null as number | null,
  /** One screen refresh (what's drawn shows on the next one); 60 Hz until measured. */
  refreshMs: 1000 / 60,
  samples: 0,
  noteAsk(ms: number) {
    this.askMs = smooth(this.askMs, ms);
    this.logNow();
  },
  noteFetch(ms: number) {
    this.fetchMs = smooth(this.fetchMs, ms);
  },
  noteRefresh(ms: number) {
    // A frame that took far longer (a busy moment, a hidden window) says nothing about the screen.
    if (ms > 4 && ms < 100) this.refreshMs = smooth(this.refreshMs, ms, 0.05);
  },
  logNow() {
    if (++this.samples % LOG_EVERY !== 0) return;
    const ms = (v: number | null) => (v === null ? "–" : v.toFixed(1));
    console.info(
      `[preview sync] asking where the music is: ${ms(this.askMs)} ms there and back; a preview frame: ${ms(this.fetchMs)} ms; screen refresh: ${ms(this.refreshMs)} ms; offset: ${usePreviewSync.getState().offsetMs} ms`,
    );
  },
};

let reading: ClockReading | null = null;

/** Where the sequence playing on the Sequence screen is, as the window last heard it. */
export const playClock = {
  /** A new reading from the engine (null: nothing plays). */
  set(next: ClockReading | null) {
    reading = next && nextReading(reading, next);
  },
  /** Where the music is at `t` (a `performance.now()` time); null when nothing plays. */
  musicAt(t: number): number | null {
    return reading && musicAt(reading, t);
  },
  reading(): ClockReading | null {
    return reading;
  },
};
