// The Sequence screen's state. The engine owns the document; this keeps a copy brought up to date
// from each edit's light reply (applySequenceChanges), plus view state: selection, playhead, zoom,
// collapsed rows, the clipboard. Every engine call for the sequencer goes through here.

import { create } from "zustand";
import { errorMessage } from "../api/backend";
import {
  applySequenceChanges,
  type Effect,
  type EffectInfo,
  type ExportSummary,
  type Sequence,
  type SequenceEdit,
  type SequenceEditResult,
  type SequenceIssue,
  type SequenceRecovery,
  type SequenceSnapshot,
} from "../api/sequence";
import type { SequencerApi } from "../api/sequencer";
import type { PlaybackStatus, XlightsSequenceImported } from "../api/types";
import { clock, fileName, plural } from "../lib/format";
import { useApp } from "./store";

const RECENT_KEY = "pixelflow.recentSequences";
const RECENT_LIMIT = 6;

function loadRecent(): string[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(saved) ? saved.filter((p): p is string => typeof p === "string").slice(0, RECENT_LIMIT) : [];
  } catch {
    return [];
  }
}

function saveRecent(paths: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(paths));
  } catch {
    // Storage unavailable; the list still works for this session.
  }
}

/**
 * Sequence edits to send: a fixed list, or a function that builds them from the document as it is
 * when their turn comes (after every earlier edit has landed), so a quick second change never
 * undoes a first one still on its way.
 */
export type SequenceEditsFrom = SequenceEdit[] | ((doc: Sequence) => SequenceEdit[]);

/** A new id for one gesture (a drag, a held key, a slider pull): its edits make one undo step. */
export function newGesture(): string {
  return crypto.randomUUID();
}

/** A short message about something that finished (an export), shown until dismissed. */
export interface Notice {
  /** Something finished ("done"), or was stopped on purpose ("info"). */
  tone: "done" | "info";
  text: string;
  /** Extra lines (an export's notes). */
  notes: string[];
  /** Offer to save the show (the playlist add changed it). */
  saveShow: boolean;
}

/** A copied effect and the row it came from. */
export interface Copied {
  rowId: string;
  effect: Effect;
}

interface SequencerState {
  api: SequencerApi | null;
  catalog: EffectInfo[];
  doc: Sequence | null;
  revision: number;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  issues: SequenceIssue[];
  /** Selected effect ids. */
  selection: string[];
  /** The row the keyboard and new effects go to (the last row clicked). */
  activeRow: string | null;
  playheadMs: number;
  /** The authored sequence playing, or null. */
  status: PlaybackStatus | null;
  sendToControllers: boolean;
  snapping: boolean;
  collapsed: string[];
  clipboard: Copied[];
  recent: string[];
  /** An export in progress (0–100), or null. */
  exporting: number | null;
  /** Set right after a new sequence with music: offer to find its beats. */
  suggestBeats: boolean;
  detecting: boolean;
  /** Changes when a different document is opened or started (not when it's saved). */
  docKey: number;
  /** Bumped to ask the timeline to bring the selection (or the playhead) and the active row into view. */
  revealAt: number;
  /** Unsaved sequences an earlier run kept, to offer back. */
  recoveries: SequenceRecovery[];
  notice: Notice | null;
  /** Something that replaces the open sequence (New, Open, Recover, an xLights import), waiting
   * on the answer to "Save changes to …?" because the open sequence has unsaved changes. */
  replacing: (() => void) | null;

  connect(api: SequencerApi): Promise<void>;
  newSequence(name: string, durationMs: number, audio: string | null): Promise<boolean>;
  open(path: string): Promise<boolean>;
  /** Imports the xLights sequence at `path` and opens it (unsaved), replacing the open one
   * without asking; the import report, or null when it failed (the error is shown). */
  importXlights(path: string): Promise<XlightsSequenceImported | null>;
  /**
   * Runs `action`, which replaces the open sequence, and returns what it returns; or, when the open
   * sequence has unsaved changes, asks first (Save / Don't save / Cancel) and returns null. The
   * action then runs once the answer allows it. New, Open, Recover, and the xLights import all
   * ask through here, so there is one question for all of them.
   */
  replaceAfterAsking<T>(action: () => T): T | null;
  /** Answers the question: Save (then replace; it keeps asking if the save fails or is
   * cancelled), Don't save, or Cancel. True when the waiting action ran. */
  resolveReplacing(choice: "save" | "discard" | "cancel"): Promise<boolean>;
  save(): Promise<boolean>;
  saveAs(): Promise<boolean>;
  /**
   * Applies edits as one undo step (or merged into `gesture`'s step), in order after every earlier
   * call. Edits given as a function are built from the latest document when their turn comes; an
   * empty build sends nothing and counts as done.
   */
  edit(edits: SequenceEditsFrom, gesture?: string): Promise<boolean>;
  undo(): Promise<boolean>;
  redo(): Promise<boolean>;
  detectBeats(): Promise<boolean>;
  select(ids: string[], activeRow?: string | null): void;
  setActiveRow(id: string | null): void;
  setPlayhead(ms: number): void;
  toggleCollapsed(rowId: string): void;
  setSnapping(on: boolean): void;
  setSendToControllers(on: boolean): Promise<void>;
  copy(): void;
  play(): Promise<void>;
  pause(): Promise<void>;
  stop(): Promise<void>;
  seek(ms: number): Promise<void>;
  /** Polls playback while it runs (the screen calls this on a timer). */
  pollPlayback(): Promise<void>;
  /** Exports an .fseq (asking where), optionally adding it to the show's playlist. */
  exportFseq(addToShow: boolean): Promise<ExportSummary | null>;
  cancelExport(): Promise<void>;
  dismissBeats(): void;
  /** Brings the selected effect (or else the playhead) and the active row into view on the timeline. */
  reveal(): void;
  /** Opens a kept unsaved sequence (ask first if the open one has changes). */
  recover(id: string): Promise<boolean>;
  discardRecovery(id: string): Promise<void>;
  /** Fetches the sequence's problems again (the show changed: props may have gone or come back). */
  refreshIssues(): Promise<void>;
  dismissNotice(): void;
}

function report(e: unknown) {
  useApp.setState({ error: errorMessage(e) });
}

export const useSequencer = create<SequencerState>((set, get) => {
  /** Bumped by every play, pause, seek, and stop, so an older answer never undoes a newer one. */
  let transport = 0;
  /** Set when the user cancels the running export, so its failure isn't reported as an error. */
  let cancelled = false;

  /** Engine calls that change the document run one at a time, in order. */
  let queue: Promise<unknown> = Promise.resolve();
  function serial<T>(task: () => Promise<T>): Promise<T> {
    const next = queue.then(task, task);
    queue = next.catch(() => undefined);
    return next;
  }

  function adopt(snapshot: SequenceSnapshot) {
    const known = new Set<string>();
    for (const row of snapshot.sequence.rows) for (const layer of row.layers) for (const e of layer.effects) known.add(e.id);
    set({
      doc: snapshot.sequence,
      revision: snapshot.revision,
      path: snapshot.path,
      dirty: snapshot.dirty,
      canUndo: snapshot.canUndo,
      canRedo: snapshot.canRedo,
      issues: snapshot.issues,
      selection: get().selection.filter((id) => known.has(id)),
    });
    if (snapshot.path) remember(snapshot.path);
  }

  function remember(path: string) {
    const recent = [path, ...get().recent.filter((p) => p !== path)].slice(0, RECENT_LIMIT);
    saveRecent(recent);
    set({ recent });
  }

  /** Brings the copy up to date from a light reply, or fetches the whole document if it fell behind. */
  async function absorb(result: SequenceEditResult, from: SequencerApi) {
    const { doc, revision, api } = get();
    // A reply from an engine this store has since let go of.
    if (api !== from) return;
    // Older than the copy (a resync already brought it in): nothing to do.
    if (result.revision < revision || (result.changed && result.revision === revision)) return;
    const meta = { dirty: result.dirty, canUndo: result.canUndo, canRedo: result.canRedo, issues: result.issues };
    if (!result.changed) {
      set(meta);
      return;
    }
    if (doc && result.revision === revision + 1) {
      const next = applySequenceChanges(doc, result.changes);
      const known = new Set<string>();
      for (const row of next.rows) for (const layer of row.layers) for (const e of layer.effects) known.add(e.id);
      set({ ...meta, doc: next, revision: result.revision, selection: get().selection.filter((id) => known.has(id)) });
      return;
    }
    const snapshot = await api?.getSequenceDoc();
    if (snapshot) adopt(snapshot);
  }

  async function guarded<T>(call: () => Promise<T>): Promise<T | null> {
    try {
      return await call();
    } catch (e) {
      report(e);
      return null;
    }
  }

  return {
    api: null,
    catalog: [],
    doc: null,
    revision: 0,
    path: null,
    dirty: false,
    canUndo: false,
    canRedo: false,
    issues: [],
    selection: [],
    activeRow: null,
    playheadMs: 0,
    status: null,
    sendToControllers: false,
    snapping: true,
    collapsed: [],
    clipboard: [],
    recent: loadRecent(),
    exporting: null,
    suggestBeats: false,
    detecting: false,
    docKey: 0,
    revealAt: 0,
    recoveries: [],
    notice: null,
    replacing: null,

    async connect(api) {
      // Calls still waiting on a previous engine have nothing to do with this one.
      queue = Promise.resolve();
      set({ api });
      await guarded(async () => {
        const [catalog, snapshot, recoveries] = await Promise.all([api.effectCatalog(), api.getSequenceDoc(), api.sequenceRecoveries()]);
        set({ catalog, recoveries });
        if (snapshot) adopt(snapshot);
        // Editing shouldn't light up the house until asked.
        await api.setSequenceDocOutput(get().sendToControllers);
      });
    },

    async newSequence(name, durationMs, audio) {
      const { api } = get();
      if (!api) return false;
      const ok = await serial(() =>
        guarded(async () => {
          await get().stop();
          // With its music from the start: nothing to undo, nothing unsaved.
          adopt(await api.newSequenceDoc(name, durationMs, audio));
          set({ selection: [], playheadMs: 0, collapsed: [], suggestBeats: audio !== null, docKey: get().docKey + 1, notice: null });
          return true;
        }),
      );
      return ok === true;
    },

    async open(path) {
      const { api } = get();
      if (!api) return false;
      const ok = await serial(async () => {
        try {
          await get().stop();
          adopt(await api.openSequenceDoc(path));
          set({ selection: [], playheadMs: 0, collapsed: [], suggestBeats: false, docKey: get().docKey + 1, notice: null });
          return true;
        } catch (e) {
          // A recent file that can't be opened any more (moved or deleted) comes off the list.
          const recent = get().recent;
          if (recent.includes(path)) {
            const left = recent.filter((p) => p !== path);
            saveRecent(left);
            set({ recent: left });
            useApp.setState({ error: `${errorMessage(e)} It's been taken off your recent sequences.` });
          } else {
            report(e);
          }
          return false;
        }
      });
      return ok === true;
    },

    async importXlights(path) {
      const { api } = get();
      if (!api) return null;
      return serial(() =>
        guarded(async () => {
          await get().stop();
          const imported = await api.importXlightsSequence(path);
          // Opened like any other document: unsaved, so it's kept (autosaved) until it's saved.
          adopt(imported.snapshot);
          set({ selection: [], playheadMs: 0, collapsed: [], suggestBeats: false, docKey: get().docKey + 1, notice: null });
          return imported;
        }),
      );
    },

    replaceAfterAsking(action) {
      const { doc, dirty } = get();
      if (doc && dirty) {
        set({ replacing: () => void action() });
        return null;
      }
      return action();
    },

    async resolveReplacing(choice) {
      const action = get().replacing;
      if (!action) return false;
      if (choice === "cancel") {
        set({ replacing: null });
        return false;
      }
      if (choice === "save" && !(await get().save())) return false;
      set({ replacing: null });
      action();
      return true;
    },

    async save() {
      const { api, path } = get();
      if (!api || !get().doc) return false;
      if (!path) return get().saveAs();
      const ok = await serial(() => guarded(async () => (adopt(await api.saveSequenceDoc()), true)));
      return ok === true;
    },

    async saveAs() {
      const { api, doc, path } = get();
      if (!api || !doc) return false;
      const target = await guarded(() => api.pickSequenceDocSavePath(path ? fileName(path) : `${doc.name}.pfseq.json`));
      if (!target) return false;
      const ok = await serial(() => guarded(async () => (adopt(await api.saveSequenceDocAs(target)), true)));
      return ok === true;
    },

    async edit(edits, gesture) {
      const { api } = get();
      if (!api || (Array.isArray(edits) && edits.length === 0)) return false;
      const ok = await serial(() =>
        guarded(async () => {
          const doc = get().doc;
          const batch = typeof edits === "function" ? (doc ? edits(doc) : []) : edits;
          if (batch.length > 0) await absorb(await api.editSequence(batch, gesture), api);
          return true;
        }),
      );
      return ok === true;
    },

    async undo() {
      const { api } = get();
      if (!api || !get().doc) return false;
      const ok = await serial(() => guarded(async () => (await absorb(await api.undoSequence(), api), true)));
      return ok === true;
    },

    async redo() {
      const { api } = get();
      if (!api || !get().doc) return false;
      const ok = await serial(() => guarded(async () => (await absorb(await api.redoSequence(), api), true)));
      return ok === true;
    },

    async detectBeats() {
      const { api } = get();
      if (!api) return false;
      set({ detecting: true, suggestBeats: false });
      try {
        // Finding the beats takes seconds; edits carry on meanwhile, and the new tracks are taken
        // in at their turn.
        const result = await guarded(() => api.detectBeats());
        if (!result) return false;
        await serial(() => guarded(() => absorb(result, api)));
        return true;
      } finally {
        set({ detecting: false });
      }
    },

    select: (ids, activeRow) => set(activeRow === undefined ? { selection: ids } : { selection: ids, activeRow }),
    setActiveRow: (activeRow) => set({ activeRow }),
    setPlayhead: (ms) => {
      const duration = get().doc?.durationMs ?? 0;
      set({ playheadMs: Math.max(0, Math.min(duration, Math.round(ms))) });
    },
    toggleCollapsed: (rowId) => {
      const collapsed = get().collapsed;
      set({ collapsed: collapsed.includes(rowId) ? collapsed.filter((id) => id !== rowId) : [...collapsed, rowId] });
    },
    setSnapping: (snapping) => set({ snapping }),

    async setSendToControllers(on) {
      set({ sendToControllers: on });
      const status = await guarded(() => get().api!.setSequenceDocOutput(on));
      if (status?.authored) set({ status });
    },

    copy() {
      const { doc, selection } = get();
      if (!doc) return;
      const chosen = new Set(selection);
      const clipboard: Copied[] = [];
      for (const row of doc.rows) {
        for (const layer of row.layers) {
          for (const effect of layer.effects) if (chosen.has(effect.id)) clipboard.push({ rowId: row.id, effect: structuredClone(effect) });
        }
      }
      if (clipboard.length > 0) set({ clipboard });
    },

    async play() {
      const { api, doc, playheadMs, status } = get();
      const backend = useApp.getState().backend;
      if (!api || !doc || !backend) return;
      const turn = ++transport;
      if (status && status.state === "paused") {
        const next = await guarded(() => backend.pausePlayback(false));
        if (next && turn === transport) set({ status: next });
        return;
      }
      const from = playheadMs >= doc.durationMs ? 0 : playheadMs;
      const next = await guarded(() => api.playSequenceDoc(from));
      if (next && turn === transport) set({ status: next, playheadMs: next.positionMs });
    },

    async pause() {
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      const turn = ++transport;
      const next = await guarded(() => backend.pausePlayback(true));
      if (next && turn === transport) set({ status: next, playheadMs: next.positionMs });
    },

    async stop() {
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      ++transport;
      // Stopped as far as the screen is concerned at once; late answers are ignored.
      set({ status: null });
      await guarded(() => backend.stopPlayback());
    },

    async seek(ms) {
      get().setPlayhead(ms);
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      const turn = ++transport;
      const next = await guarded(() => backend.seekPlayback(get().playheadMs));
      if (next && turn === transport) set({ status: next });
    },

    async pollPlayback() {
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      const turn = transport;
      try {
        const next = await backend.playbackStatus();
        // Play, pause, seek, or stop since the question: this answer is out of date.
        if (turn !== transport || !get().status) return;
        if (!next || !next.authored) {
          set({ status: null });
          return;
        }
        if (next.state === "ended") {
          // The song is over: let go of the player, so a seek or a click on the ruler only moves
          // the playhead (and Play starts again from the top).
          ++transport;
          set({ status: null, playheadMs: next.positionMs });
          await guarded(() => backend.stopPlayback());
          return;
        }
        set({ status: next, playheadMs: next.positionMs });
      } catch {
        // The next poll tries again.
      }
    },

    async exportFseq(addToShow) {
      const { api, doc, path } = get();
      if (!api || !doc) return null;
      const base = path ? fileName(path).replace(/\.pfseq\.json$|\.json$/i, "") : doc.name;
      const target = await guarded(() => api.pickExportPath(`${base}.fseq`));
      if (!target) return null;
      set({ exporting: 0, notice: null });
      cancelled = false;
      // Export what's on screen: every edit made so far lands first.
      await serial(async () => undefined);
      let summary: ExportSummary;
      try {
        summary = await api.exportSequenceDoc(target, (p) => set({ exporting: p.percent }));
      } catch (e) {
        if (cancelled) set({ notice: { tone: "info", text: "Export cancelled. No file was written.", notes: [], saveShow: false } });
        else report(e);
        return null;
      } finally {
        set({ exporting: null });
      }
      const done = `Exported ${plural(summary.frames, "frame")} (${clock(summary.durationMs / 1000)}) to ${fileName(target)}.`;
      if (!addToShow) {
        set({ notice: { tone: "done", text: done, notes: summary.notes, saveShow: false } });
        return summary;
      }
      // One undo step on the show, in line with its other changes.
      const added = await useApp.getState().run(() => api.addSequenceDocToShow(target));
      set({
        notice: added
          ? { tone: "done", text: `${done} It's on the show's playlist; save the show to keep it there.`, notes: summary.notes, saveShow: true }
          : { tone: "done", text: `${done} It couldn't be added to the show's playlist.`, notes: summary.notes, saveShow: false },
      });
      return summary;
    },

    async cancelExport() {
      cancelled = true;
      await guarded(() => get().api!.cancelSequenceExport());
    },

    async recover(id) {
      const { api } = get();
      if (!api) return false;
      const ok = await serial(() =>
        guarded(async () => {
          await get().stop();
          adopt(await api.recoverSequence(id));
          set({
            selection: [],
            playheadMs: 0,
            collapsed: [],
            suggestBeats: false,
            docKey: get().docKey + 1,
            recoveries: get().recoveries.filter((r) => r.id !== id),
            notice: null,
          });
          return true;
        }),
      );
      return ok === true;
    },

    async discardRecovery(id) {
      const { api } = get();
      if (!api) return;
      await guarded(() => api.discardSequenceRecovery(id));
      set({ recoveries: get().recoveries.filter((r) => r.id !== id) });
    },

    async refreshIssues() {
      const { api } = get();
      if (!api || !get().doc) return;
      await serial(() =>
        guarded(async () => {
          const snapshot = await api.getSequenceDoc();
          if (!snapshot || get().api !== api) return;
          if (snapshot.revision === get().revision) set({ issues: snapshot.issues });
          else adopt(snapshot);
        }),
      );
    },

    dismissNotice: () => set({ notice: null }),

    dismissBeats: () => set({ suggestBeats: false }),
    reveal: () => set({ revealAt: get().revealAt + 1 }),
  };
});
