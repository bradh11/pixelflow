// The preview in a window of its own ("pop out"): the room it took goes back to the controls, and
// the lights can go to another display. There is one such window for the whole app; while it's
// open, the previews in the main window show a slim note instead (see components/preview/PopOut).
//
// The two windows stay in step through the engine, which both ask for frames and where playback
// is, and through a pair of notes (Backend.previewLink): the main window's says what its preview
// would show and how (PreviewFeed), the preview window's says what was chosen there
// (PreviewReply). The preview's window only reads; play/pause pressed there is counted in its
// note, and the screen in front here does it.

import { useEffect, useRef } from "react";
import { create } from "zustand";
import { errorMessage } from "../api/backend";
import type { SequenceTarget } from "../api/sequence";
import { type ViewOptions, readViewOptions, sameView } from "../lib/viewOptions";
import { usePreviewSync } from "./previewSync";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";
import { type LayoutMode, useView3d } from "./view3d";

/** The open sequence at its playhead (drawn by the engine for that moment), or the lights as the
 * engine is sending them now (a rendered sequence playing, a test pattern). */
export type PreviewSource = "sequence" | "live";

/** The main window's note: what the preview's window shows, and how. */
export interface PreviewFeed {
  source: PreviewSource;
  /** For the open sequence: where its playhead is, and how long it is. */
  playheadMs: number;
  durationMs: number;
  /** Changes whenever the picture at the playhead may have: an edit, another sequence, a picture read. */
  stamp: string;
  /** Only this row's pixels ("Selected row only"), or null for the whole display. */
  only: SequenceTarget | null;
  /** The show's revision: the window fetches the show again when it changes. */
  showRevision: number;
  /** What's showing, by name. */
  title: string | null;
  /** Whether the screen in front has a transport, for play/pause pressed there to work. */
  transport: boolean;
  mode: LayoutMode;
  glow: number;
  /** The other View options (names, pixels that are off, dot size, boost, the grid). */
  view: ViewOptions;
  /** The viewer's preview sync offset (see state/previewSync.ts), so both windows draw the same moment. */
  syncMs: number;
}

/** The preview window's note: what was chosen there. */
export interface PreviewReply {
  mode: LayoutMode;
  glow: number;
  view: ViewOptions;
  /** How many times play/pause was pressed there. */
  toggles: number;
}

const isMode = (v: unknown): v is LayoutMode => v === "2d" || v === "3d";
const isNumber = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);

/** The main window's note, when `value` is one. */
export function readFeed(value: unknown): PreviewFeed | null {
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  if ((v.source !== "sequence" && v.source !== "live") || !isMode(v.mode)) return null;
  if (![v.playheadMs, v.durationMs, v.showRevision, v.glow, v.syncMs].every(isNumber) || typeof v.stamp !== "string") return null;
  return {
    source: v.source,
    playheadMs: v.playheadMs as number,
    durationMs: v.durationMs as number,
    stamp: v.stamp,
    only: typeof v.only === "object" && v.only !== null ? (v.only as SequenceTarget) : null,
    showRevision: v.showRevision as number,
    title: typeof v.title === "string" ? v.title : null,
    transport: v.transport === true,
    mode: v.mode,
    glow: v.glow as number,
    view: readViewOptions(v.view),
    syncMs: v.syncMs as number,
  };
}

/** The preview window's note, when `value` is one. */
export function readReply(value: unknown): PreviewReply | null {
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  return isMode(v.mode) && isNumber(v.glow) && isNumber(v.toggles) ? { mode: v.mode, glow: v.glow, view: readViewOptions(v.view), toggles: v.toggles } : null;
}

const KEY = "pixelflow.previewPoppedOut";
/** How often the main window leaves its note and reads the other's, while the preview is popped out. */
const LINK_MS = 50;

function wasPoppedOut(): boolean {
  try {
    return localStorage.getItem(KEY) === "true";
  } catch {
    return false;
  }
}

function remember(poppedOut: boolean) {
  try {
    if (poppedOut) localStorage.setItem(KEY, "true");
    else localStorage.removeItem(KEY);
  } catch {
    // Storage unavailable: it's where it is until the app closes.
  }
}

interface PreviewWindowState {
  /** The preview is in its own window. */
  poppedOut: boolean;
  /** What the Play screen's preview shows (that screen says; the others' is worked out). */
  play: { source: PreviewSource; title: string | null };
  /** The Sequence screen's preview shows only the selected row's pixels. */
  rowOnly: boolean;
  /** Counts play/pause presses in the preview's window, for the screen in front to act on. */
  toggles: number;
  /** Opens the preview's window; false when it couldn't be (the error is shown unless `quiet`). */
  popOut(options?: { quiet?: boolean }): Promise<boolean>;
  /** Closes the preview's window: the previews here show again. */
  bringBack(): Promise<void>;
  setPlay(play: { source: PreviewSource; title: string | null }): void;
  setRowOnly(on: boolean): void;
}

/** Whether the Sequence screen is in front with a sequence open (its preview is the one shown). */
function onSequence(): boolean {
  return useApp.getState().screen === "sequence" && useSequencer.getState().doc !== null;
}

/** What the preview in front would show now. */
export function currentFeed(): PreviewFeed {
  const app = useApp.getState();
  const seq = useSequencer.getState();
  const view = useView3d.getState();
  const own = usePreviewWindow.getState();
  const sequence = onSequence();
  const onPlay = app.screen === "play";
  const doc = sequence || (onPlay && own.play.source === "sequence") ? seq.doc : null;
  const row = sequence && own.rowOnly ? doc?.rows.find((r) => r.id === seq.activeRow) : undefined;
  return {
    source: doc ? "sequence" : "live",
    playheadMs: seq.playheadMs,
    durationMs: doc?.durationMs ?? 0,
    stamp: `${seq.docKey}:${seq.revision}:${seq.picturesArrived}`,
    only: row?.target ?? null,
    showRevision: app.snapshot?.revision ?? -1,
    title: doc ? doc.name : onPlay ? own.play.title : null,
    transport: sequence || onPlay,
    mode: sequence ? view.sequenceMode : view.playMode,
    glow: view.glow,
    view: readViewOptions(view),
    syncMs: usePreviewSync.getState().offsetMs,
  };
}

/** Takes on what was chosen in the preview's window since its last note (`seen`). */
function adopt(reply: PreviewReply, seen: PreviewReply | null) {
  const view = useView3d.getState();
  if (reply.glow !== seen?.glow && reply.glow !== view.glow) view.setGlow(reply.glow);
  if ((!seen || !sameView(reply.view, seen.view)) && !sameView(reply.view, readViewOptions(view))) view.setViewOptions(reply.view);
  if (reply.mode !== seen?.mode) {
    if (onSequence()) {
      if (view.sequenceMode !== reply.mode) view.setSequenceMode(reply.mode);
    } else if (view.playMode !== reply.mode) {
      view.setPlayMode(reply.mode);
    }
  }
  if (seen && reply.toggles > seen.toggles) usePreviewWindow.setState((s) => ({ toggles: s.toggles + 1 }));
}

export const usePreviewWindow = create<PreviewWindowState>((set, get) => {
  /** The window being opened, so asking twice in a row opens one. */
  let opening: Promise<boolean> | null = null;

  async function open(quiet: boolean): Promise<boolean> {
    const backend = useApp.getState().backend;
    if (!backend) return false;
    try {
      // What to show is said as the window is asked for (both at once: a browser only opens a
      // window straight from the click), so it's there before the window reads it.
      const [, opened] = await Promise.all([backend.previewLink("main", currentFeed()), backend.openPreviewWindow()]);
      if (!opened) {
        remember(false);
        if (!quiet) {
          useApp.setState({ error: "The preview's window couldn't be opened: your browser blocked it. Allow pop-ups for PixelFlow and try again." });
        }
        return false;
      }
    } catch (e) {
      if (!quiet) useApp.setState({ error: errorMessage(e) });
      return false;
    }
    remember(true);
    set({ poppedOut: true });
    return true;
  }

  return {
    poppedOut: false,
    play: { source: "live", title: null },
    rowOnly: false,
    toggles: 0,

    popOut(options) {
      opening ??= open(options?.quiet === true).finally(() => {
        opening = null;
      });
      return opening;
    },

    async bringBack() {
      if (!get().poppedOut) return;
      remember(false);
      set({ poppedOut: false });
      try {
        await useApp.getState().backend?.closePreviewWindow();
      } catch (e) {
        useApp.setState({ error: errorMessage(e) });
      }
    },

    setPlay: (play) => {
      const now = get().play;
      if (now.source !== play.source || now.title !== play.title) set({ play });
    },
    setRowOnly: (rowOnly) => set({ rowOnly }),
  };
});

/**
 * Keeps the preview's own window in step while it's open (for the app, once): leaves the main
 * window's note whenever it changes, takes on what was chosen there, and notices the window
 * being closed. It also opens the window again, once a show is up, when it was open as PixelFlow
 * last closed.
 */
export function usePreviewLink() {
  const backend = useApp((s) => s.backend);
  const started = useApp((s) => s.started);
  const poppedOut = usePreviewWindow((s) => s.poppedOut);

  useEffect(() => {
    if (backend && started && wasPoppedOut() && !usePreviewWindow.getState().poppedOut) void usePreviewWindow.getState().popOut({ quiet: true });
  }, [backend, started]);

  useEffect(() => {
    if (!backend || !poppedOut) return;
    let live = true;
    let busy = false;
    let sent = "";
    let seen: PreviewReply | null = null;
    const turn = async () => {
      if (busy) return;
      busy = true;
      try {
        const feed = currentFeed();
        const json = JSON.stringify(feed);
        const linked = await backend.previewLink("main", json === sent ? undefined : feed);
        sent = json;
        if (!live) return;
        if (!linked.open) {
          // Closed from its own window: the previews here show again.
          remember(false);
          usePreviewWindow.setState({ poppedOut: false });
          return;
        }
        const reply = readReply(linked.theirs);
        if (reply) {
          adopt(reply, seen);
          seen = reply;
        }
      } catch {
        // The next turn tries again.
      } finally {
        busy = false;
      }
    };
    const timer = setInterval(() => void turn(), LINK_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [backend, poppedOut]);
}

/**
 * Runs `toggle` each time play/pause is pressed in the preview's own window (for the screen in
 * front, when it has a transport).
 */
export function usePreviewToggles(toggle: () => void) {
  const toggles = usePreviewWindow((s) => s.toggles);
  // Presses from before this screen was in front aren't its to act on.
  const seen = useRef(toggles);
  useEffect(() => {
    if (toggles === seen.current) return;
    seen.current = toggles;
    toggle();
    // Only a new press runs it, not a new `toggle`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [toggles]);
}
