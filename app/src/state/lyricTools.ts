// Tools for timing lyrics precisely on the timeline: the vocals lane (shown or not, kept on this
// computer) and tap timing (re-timing a words or syllables track by ear, see lib/tapTiming).

import { create } from "zustand";
import type { Mark } from "../api/sequence";
import { previewPosition } from "../lib/avSync";
import { followEdits } from "../lib/lyricEdits";
import { type TapSession, finishTapSession, pressTap, releaseTap, startTapSession } from "../lib/tapTiming";
import { playClock, usePreviewSync } from "./previewSync";
import { useSequencer } from "./sequencer";

const VOCALS_KEY = "pixelflow.vocalsLane";
/** Playback starts this far before the first mark to time, to catch the beat. */
export const LEAD_IN_MS = 2_000;
/** Speeds tap timing offers. */
export const TAP_SPEEDS = [0.5, 0.75, 1] as const;

function loadVocals(): boolean {
  try {
    return localStorage.getItem(VOCALS_KEY) === "true";
  } catch {
    return false;
  }
}

function saveVocals(on: boolean) {
  try {
    localStorage.setItem(VOCALS_KEY, String(on));
  } catch {
    // Storage unavailable: the lane stays as it is until the app closes.
  }
}

/** Tap timing on a track: getting ready (picking the speed), then running. */
export interface TapTiming {
  track: string;
  /** Where the marks to time start (the playhead when it was opened). */
  fromMs: number;
  speed: number;
  /** The track's marks when it started, to check nothing else changed them meanwhile. */
  marks: Mark[];
  session: TapSession | null;
}

interface LyricToolsState {
  vocalsShown: boolean;
  setVocalsShown(on: boolean): void;
  tap: TapTiming | null;
  /** Gets tap timing ready on `trackId` from the playhead (nothing plays yet). */
  openTap(trackId: string): void;
  setTapSpeed(speed: number): void;
  /** Plays from a little before the first mark (at the chosen speed) and starts listening for taps. */
  beginTap(): Promise<void>;
  /** A key went down or came up at `t` (a `performance.now()` time). */
  press(t: number): void;
  release(t: number): void;
  /** Stops playback and puts the new times on the track (with the marks below it following), as
   * one undo step. False when nothing was tapped or the edit failed. */
  finishTap(): Promise<boolean>;
  /** Stops without changing anything. */
  cancelTap(): Promise<void>;
}

/** Where the music heard at `t` is, as the preview shows it (the preview sync offset takes in how
 * late the sound reaches the ears). */
function heardAt(t: number): number | null {
  const music = playClock.musicAt(t);
  const doc = useSequencer.getState().doc;
  return music === null || !doc ? null : previewPosition(music, usePreviewSync.getState().offsetMs, doc.durationMs);
}

/** Stops the music if tap timing started it, back at full speed. */
async function stopMusic() {
  const seq = useSequencer.getState();
  if (!seq.status) return;
  await seq.pause();
  await seq.setPlaybackSpeed(1);
}

export const useLyricTools = create<LyricToolsState>((set, get) => ({
  vocalsShown: loadVocals(),
  setVocalsShown: (on) => {
    set({ vocalsShown: on });
    saveVocals(on);
  },
  tap: null,

  openTap(trackId) {
    const { doc, playheadMs } = useSequencer.getState();
    const track = doc?.timingTracks.find((t) => t.id === trackId);
    if (!track) return;
    useSequencer.getState().setActiveTrack(trackId);
    set({ tap: { track: trackId, fromMs: playheadMs, speed: get().tap?.speed ?? 0.75, marks: track.marks, session: null } });
  },

  setTapSpeed(speed) {
    const tap = get().tap;
    if (!tap) return;
    set({ tap: { ...tap, speed } });
    if (tap.session) void useSequencer.getState().setPlaybackSpeed(speed);
  },

  async beginTap() {
    const tap = get().tap;
    const seq = useSequencer.getState();
    const track = seq.doc?.timingTracks.find((t) => t.id === tap?.track);
    if (!tap || !track) return;
    const session = startTapSession(track.marks, tap.fromMs);
    const first = track.marks[session.queue[0]];
    if (!first) {
      useSequencer.setState({ notice: { tone: "info", text: `${track.name} has no marks after the playhead to time. Move the playhead before them and try again.`, notes: [], saveShow: false } });
      set({ tap: null });
      return;
    }
    if (seq.status) await seq.pause();
    seq.setPlayhead(Math.max(0, first.startMs - LEAD_IN_MS));
    set({ tap: { ...tap, marks: track.marks, session } });
    await useSequencer.getState().play();
    if (tap.speed !== 1) await useSequencer.getState().setPlaybackSpeed(tap.speed);
  },

  press(t) {
    const tap = get().tap;
    const at = heardAt(t);
    if (!tap?.session || at === null) return;
    set({ tap: { ...tap, session: pressTap(tap.session, at) } });
  },

  release(t) {
    const tap = get().tap;
    const at = heardAt(t);
    if (!tap?.session || at === null) return;
    set({ tap: { ...tap, session: releaseTap(tap.session, at) } });
  },

  async finishTap() {
    const tap = get().tap;
    set({ tap: null });
    await stopMusic();
    if (!tap?.session || tap.session.times.length === 0) return false;
    const { session, marks, track: trackId } = tap;
    const seq = useSequencer.getState();
    let timed: number[] = [];
    const ok = await seq.edit((doc) => {
      const track = doc.timingTracks.find((t) => t.id === trackId);
      if (!track) return [];
      if (JSON.stringify(track.marks) !== JSON.stringify(marks)) {
        throw new Error(`${track.name} changed while you were tapping, so the taps weren't used. Try again.`);
      }
      const done = finishTapSession(session, marks, doc.durationMs, Math.max(1, Math.min(doc.frameMs, 10)));
      if (!done) return [];
      timed = session.queue.slice(0, session.times.length).map((i) => done.marks[i].startMs);
      return [{ type: "updateTimingTrack", track: { ...track, marks: done.marks } }, ...followEdits(doc, trackId, done.moves)];
    });
    if (ok && timed.length > 0) useSequencer.getState().selectMarks(trackId, timed);
    return ok;
  },

  async cancelTap() {
    const running = get().tap?.session != null;
    set({ tap: null });
    if (running) await stopMusic();
  },
}));
