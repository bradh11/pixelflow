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
  type SequenceSnapshot,
} from "../api/sequence";
import type { SequencerApi } from "../api/sequencer";
import type { PlaybackStatus, XlightsSequenceImported } from "../api/types";
import { fileName } from "../lib/format";
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

  connect(api: SequencerApi): Promise<void>;
  newSequence(name: string, durationMs: number, audio: string | null): Promise<boolean>;
  open(path: string): Promise<boolean>;
  /** Imports the xLights sequence at `path` and opens it (unsaved), replacing the open one
   * without asking; the import report, or null when it failed (the error is shown). */
  importXlights(path: string): Promise<XlightsSequenceImported | null>;
  save(): Promise<boolean>;
  saveAs(): Promise<boolean>;
  /** Applies edits as one undo step (or merged into `gesture`'s step). */
  edit(edits: SequenceEdit[], gesture?: string): Promise<boolean>;
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
}

function report(e: unknown) {
  useApp.setState({ error: errorMessage(e) });
}

export const useSequencer = create<SequencerState>((set, get) => {
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
  async function absorb(result: SequenceEditResult) {
    const { doc, revision, api } = get();
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

    async connect(api) {
      set({ api });
      await guarded(async () => {
        const [catalog, snapshot] = await Promise.all([api.effectCatalog(), api.getSequenceDoc()]);
        set({ catalog });
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
          const snapshot = await api.newSequenceDoc(name, durationMs);
          adopt(snapshot);
          set({ selection: [], playheadMs: 0, collapsed: [], suggestBeats: audio !== null });
          if (audio) {
            const sequence = snapshot.sequence;
            await absorb(await api.editSequence([{ type: "updateInfo", name: sequence.name, audio, durationMs, frameMs: sequence.frameMs }]));
          }
          return true;
        }),
      );
      return ok === true;
    },

    async open(path) {
      const { api } = get();
      if (!api) return false;
      const ok = await serial(() =>
        guarded(async () => {
          await get().stop();
          adopt(await api.openSequenceDoc(path));
          set({ selection: [], playheadMs: 0, collapsed: [], suggestBeats: false });
          return true;
        }),
      );
      return ok === true;
    },

    async importXlights(path) {
      const { api } = get();
      if (!api) return null;
      return serial(() =>
        guarded(async () => {
          await get().stop();
          const imported = await api.importXlightsSequence(path);
          adopt(imported.snapshot);
          set({ selection: [], playheadMs: 0, collapsed: [], suggestBeats: false });
          return imported;
        }),
      );
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
      if (!api || edits.length === 0) return false;
      const ok = await serial(() => guarded(async () => (await absorb(await api.editSequence(edits, gesture)), true)));
      return ok === true;
    },

    async undo() {
      const { api } = get();
      if (!api || !get().doc) return false;
      const ok = await serial(() => guarded(async () => (await absorb(await api.undoSequence()), true)));
      return ok === true;
    },

    async redo() {
      const { api } = get();
      if (!api || !get().doc) return false;
      const ok = await serial(() => guarded(async () => (await absorb(await api.redoSequence()), true)));
      return ok === true;
    },

    async detectBeats() {
      const { api } = get();
      if (!api) return false;
      set({ detecting: true, suggestBeats: false });
      try {
        const ok = await serial(() => guarded(async () => (await absorb(await api.detectBeats()), true)));
        return ok === true;
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
      if (status && status.state === "paused") {
        const next = await guarded(() => backend.pausePlayback(false));
        if (next) set({ status: next });
        return;
      }
      const from = playheadMs >= doc.durationMs ? 0 : playheadMs;
      const next = await guarded(() => api.playSequenceDoc(from));
      if (next) set({ status: next, playheadMs: next.positionMs });
    },

    async pause() {
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      const next = await guarded(() => backend.pausePlayback(true));
      if (next) set({ status: next, playheadMs: next.positionMs });
    },

    async stop() {
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      await guarded(() => backend.stopPlayback());
      set({ status: null });
    },

    async seek(ms) {
      get().setPlayhead(ms);
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      const next = await guarded(() => backend.seekPlayback(get().playheadMs));
      if (next) set({ status: next });
    },

    async pollPlayback() {
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      try {
        const next = await backend.playbackStatus();
        if (!get().status) return;
        if (!next || !next.authored) {
          set({ status: null });
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
      set({ exporting: 0 });
      try {
        const summary = await api.exportSequenceDoc(target, (p) => set({ exporting: p.percent }));
        if (addToShow) {
          const snapshot = await api.addSequenceDocToShow(target);
          useApp.setState({ snapshot });
        }
        return summary;
      } catch (e) {
        report(e);
        return null;
      } finally {
        set({ exporting: null });
      }
    },

    async cancelExport() {
      await guarded(() => get().api!.cancelSequenceExport());
    },

    dismissBeats: () => set({ suggestBeats: false }),
  };
});
