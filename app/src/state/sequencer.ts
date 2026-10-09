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
  type Row,
  type Sequence,
  type SequenceEdit,
  type SequenceEditResult,
  type SequenceIssue,
  type SequenceRecovery,
  type SequenceSnapshot,
} from "../api/sequence";
import type { ProviderId } from "../api/assistant";
import type { LyricsChoice, LyricsFound, LyricsOptions, SequencerApi } from "../api/sequencer";
import type { MissingFile, PlaybackStatus, VendorImportOptions, XlightsSequenceImported } from "../api/types";
import { clock, fileName, plural, shownPath } from "../lib/format";
import { folderOf } from "../lib/showFiles";
import { tapEdits } from "../lib/timelineMath";
import { type SaveOptions, saidSaved, useApp } from "./store";
import { describeSequenceEdits } from "../lib/describeChange";
import { edited, stepped, useUndoLabels } from "./undoLabels";

const RECENT_KEY = "pixelflow.recentSequences";
/** Whether playback loops, remembered on this computer. */
const LOOP_KEY = "pixelflow.sequenceLoop";
const RECENT_LIMIT = 12;

/** A sequence opened or saved lately, and the show (by path) it was used with. */
export interface RecentSequence {
  path: string;
  /** The show's file, or null when the show wasn't saved (or the entry is from before shows
   * were kept with sequences). */
  show: string | null;
}

export function loadRecent(): RecentSequence[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    if (!Array.isArray(saved)) return [];
    return saved
      .map((entry): RecentSequence | null => {
        if (typeof entry === "string") return { path: entry, show: null };
        if (typeof entry !== "object" || entry === null) return null;
        const { path, show } = entry as Record<string, unknown>;
        return typeof path === "string" ? { path, show: typeof show === "string" ? show : null } : null;
      })
      .filter((r): r is RecentSequence => r !== null)
      .slice(0, RECENT_LIMIT);
  } catch {
    return [];
  }
}

/** The recent sequences used with `show` first (newest first), then the others. */
export function recentFor(recent: RecentSequence[], show: string | null): { mine: RecentSequence[]; others: RecentSequence[] } {
  const mine = show ? recent.filter((r) => r.show === show) : [];
  return { mine, others: recent.filter((r) => !mine.includes(r)) };
}

const TIMING_HIDDEN_KEY = "pixelflow.timingTracksHidden";

/** Whether the timing tracks were folded away last time (a per-viewer convenience). */
function loadTimingHidden(): boolean {
  try {
    return localStorage.getItem(TIMING_HIDDEN_KEY) === "true";
  } catch {
    return false;
  }
}

function saveTimingHidden(on: boolean) {
  try {
    localStorage.setItem(TIMING_HIDDEN_KEY, String(on));
  } catch {
    // Storage unavailable: the tracks stay folded until the app closes.
  }
}

function loadLoop(): boolean {
  try {
    return localStorage.getItem(LOOP_KEY) === "true";
  } catch {
    return false;
  }
}

function saveLoop(on: boolean) {
  try {
    localStorage.setItem(LOOP_KEY, String(on));
  } catch {
    // Storage unavailable: looping still applies until the app closes.
  }
}

function saveRecent(recent: RecentSequence[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
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
  /** What Find lyrics found, when that's what this says: its source line, and other lyrics to
   * pick instead. */
  lyrics?: LyricsFound;
  /** How that Find lyrics ran, to find again the same way. */
  lyricsRun?: { provider: ProviderId; upload: boolean };
}

/** Selected timing marks: on one track, picked by start time. */
export interface MarkSelection {
  track: string;
  starts: number[];
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
  /** Selected timing marks (selecting effects clears them, and the other way round). */
  markSelection: MarkSelection | null;
  /** The timing track picked last (its header or a mark clicked): T taps marks onto it. */
  activeTrack: string | null;
  /** The row the keyboard and new effects go to (the last row clicked). */
  activeRow: string | null;
  playheadMs: number;
  /** The authored sequence playing, or null. */
  status: PlaybackStatus | null;
  sendToControllers: boolean;
  /** Playback goes round again from the top at the end (the engine jumps the music back too). */
  looping: boolean;
  snapping: boolean;
  /** The timing tracks are folded away above the rows (snapping still uses them). */
  timingHidden: boolean;
  collapsed: string[];
  clipboard: Copied[];
  /** Sequences opened or saved lately, with the show each was used with. */
  recent: RecentSequence[];
  /** An export in progress (0–100), or null. */
  exporting: number | null;
  /** Set right after a new sequence with music: offer to find its beats. */
  suggestBeats: boolean;
  detecting: boolean;
  /** While Find lyrics runs: what it's doing ("Looking up published lyrics"); else null. */
  findingLyrics: string | null;
  /** While Re-time to vocals runs. */
  retimingLyrics: boolean;
  /** Changes when a different document is opened or started (not when it's saved). */
  docKey: number;
  /** Bumped to ask the timeline to bring the selection (or the playhead) and the active row into view. */
  revealAt: number;
  /** What the last reveal asked for: the selection (else the playhead), or the playhead alone. */
  revealTarget: "selection" | "playhead";
  /** Unsaved sequences an earlier run kept, to offer back. */
  recoveries: SequenceRecovery[];
  notice: Notice | null;
  /** Something that replaces the open sequence (New, Open, Recover, an xLights import), waiting
   * on the answer to "Save changes to …?" because the open sequence has unsaved changes. */
  replacing: (() => void) | null;

  connect(api: SequencerApi): Promise<void>;
  /** Closes the open sequence (the show it belongs to is being left); unsaved changes are
   * dropped, so ask first. */
  closeDocument(): Promise<void>;
  /** Starts a new sequence (with `rows`, when given: see `rowsForShow`). */
  newSequence(name: string, durationMs: number, audio: string | null, rows?: Row[]): Promise<boolean>;
  open(path: string): Promise<boolean>;
  /** Imports the xLights sequence at `path` (a vendor's, with `options`: see
   * SequencerApi.importXlightsSequence) and opens it (unsaved), replacing the open one without
   * asking; the import report, or null when it failed (the error is shown). */
  importXlights(path: string, options?: VendorImportOptions): Promise<XlightsSequenceImported | null>;
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
  /** Saves the sequence (asking where the first time); a toast says so unless `quiet`. */
  save(options?: SaveOptions): Promise<boolean>;
  saveAs(options?: SaveOptions): Promise<boolean>;
  /**
   * Applies edits as one undo step (or merged into `gesture`'s step), in order after every earlier
   * call. Edits given as a function are built from the latest document when their turn comes; an
   * empty build sends nothing and counts as done, and a build that throws sends nothing and shows
   * its message as an error (it isn't done).
   */
  edit(edits: SequenceEditsFrom, gesture?: string): Promise<boolean>;
  undo(): Promise<boolean>;
  redo(): Promise<boolean>;
  detectBeats(): Promise<boolean>;
  /** Finds the song's lyrics with the assistant's `provider` (sending its audio to OpenAI only
   * with `upload`) and adds Lyrics, Lyrics (words), Lyrics (syllables), Lyrics (phonemes), and
   * Vocals tracks as one undo step; a notice says where they came from. Null when it failed (the
   * error is shown) or was stopped. `options`: the language expected when nothing else says
   * (English by default), and whether to find again without what's kept for the song. */
  findLyrics(provider: ProviderId, upload: boolean, options?: LyricsOptions): Promise<LyricsFound | null>;
  /** Lines the lyrics up again with another candidate or pasted lyrics (one undo step), asking no
   * one. Null when it failed (the error is shown). */
  chooseLyrics(choice: LyricsChoice): Promise<LyricsFound | null>;
  /** Stops Find lyrics (nothing is added). */
  cancelLyrics(): Promise<void>;
  /** Makes syllables and mouth shapes again from the words track `trackId` (one undo step). */
  syllablesFromWords(trackId: string): Promise<boolean>;
  /** Moves the lyrics tracks `trackId` belongs with by `ms` together (one undo step). */
  nudgeLyrics(trackId: string, ms: number): Promise<boolean>;
  /** Locks the words on the lyrics tracks `trackId` belongs with onto the song's voice again
   * (one undo step), saying how far they moved. */
  retimeLyrics(trackId: string): Promise<boolean>;
  select(ids: string[], activeRow?: string | null): void;
  /** Selects marks on `track` by their start times (and makes it the active track). */
  selectMarks(track: string, starts: number[]): void;
  setActiveTrack(id: string | null): void;
  /** Tap to time: drops a mark at the playhead on the active timing track (ending the mark the
   * last tap started). Each tap is one undo step. */
  tap(): void;
  /** Imports an .xtiming or Audacity labels file (asking which) as new timing tracks. */
  importTiming(): Promise<boolean>;
  /** Exports a timing track (asking where) as .xtiming or Audacity labels. */
  exportTiming(trackId: string): Promise<boolean>;
  setActiveRow(id: string | null): void;
  setPlayhead(ms: number): void;
  toggleCollapsed(rowId: string): void;
  toggleTimingHidden(): void;
  setSnapping(on: boolean): void;
  setSendToControllers(on: boolean): Promise<void>;
  /** Turns looping on or off (remembered on this computer); a playing sequence switches at once. */
  setLooping(on: boolean): void;
  copy(): void;
  play(): Promise<void>;
  pause(): Promise<void>;
  /** Stops playback, leaving the playhead where it is; pressed again while stopped, goes back to
   * the start (playhead, timeline, and preview). */
  stop(): Promise<void>;
  seek(ms: number): Promise<void>;
  /** Polls playback while it runs (the screen calls this on a timer). */
  pollPlayback(): Promise<void>;
  /** Exports an .fseq (asking where), optionally adding it to the show's playlist. */
  exportFseq(addToShow: boolean): Promise<ExportSummary | null>;
  /** Resolves once every edit made so far has reached the engine (before it exports or sends). */
  settled(): Promise<void>;
  cancelExport(): Promise<void>;
  dismissBeats(): void;
  /** Brings the selected effect (or else the playhead) and the active row into view on the
   * timeline; with "playhead", the playhead alone. */
  reveal(target?: "selection" | "playhead"): void;
  /** Opens a kept unsaved sequence (ask first if the open one has changes). */
  recover(id: string): Promise<boolean>;
  discardRecovery(id: string): Promise<void>;
  /** Fetches the sequence's problems again (the show changed: props may have gone or come back). */
  refreshIssues(): Promise<void>;
  dismissNotice(): void;
  /** The open sequence's music when it isn't where the sequence says (see checkMusic). */
  musicMissing: MissingFile | null;
  /** Asks whether the open sequence's music is where it says (after opening, or new music). */
  checkMusic(): Promise<void>;
  /** Looks for the missing music in the sequence's and the show's folders and uses it if found
   * (one undo step on the sequence), saying what happened. */
  findMusic(): Promise<boolean>;
  /** Asks where the music is now and uses that file (one undo step on the sequence). */
  locateMusic(): Promise<boolean>;
}

/** The notice for what Find lyrics found. */
function lyricsNotice(found: LyricsFound, lyricsRun?: Notice["lyricsRun"]): Notice {
  const notes = [...found.notes];
  if (found.unsureWords > 0) {
    notes.push(`${plural(found.unsureWords, "word")} ${found.unsureWords === 1 ? "has" : "have"} rough timing: drag ${found.unsureWords === 1 ? "it" : "them"} on Lyrics (words) if needed.`);
  }
  const timing = found.timingNote ? ` ${found.timingNote}` : "";
  return { tone: "done", text: `Found ${plural(found.lines, "line")} and ${plural(found.words, "word")}. ${found.summary}${timing}`, notes, saveShow: false, lyrics: found, lyricsRun };
}

function report(e: unknown) {
  useApp.setState({ error: errorMessage(e) });
}

export const useSequencer = create<SequencerState>((set, get) => {
  /** Bumped by every play, pause, seek, and stop, so an older answer never undoes a newer one. */
  let transport = 0;
  /** When the playhead last came from the player, to tell where the music is between polls. */
  let playheadAt = 0;
  /** The mark the last tap started (tap to time ends it at the next tap). A run of taps only
   * carries on while the music plays on: a seek, play, pause, stop, another track, or another
   * document starts a fresh run. */
  let lastTap: { track: string; startMs: number } | null = null;
  /** Set when the user cancels the running export, so its failure isn't reported as an error. */
  let cancelled = false;

  /** Lets go of the player (if one is running), leaving the playhead where it is. */
  async function halt() {
    const backend = useApp.getState().backend;
    lastTap = null;
    if (!backend || !get().status) return;
    ++transport;
    // Stopped as far as the screen is concerned at once; late answers are ignored.
    set({ status: null });
    await guarded(() => backend.stopPlayback());
  }

  /** The next document's key; a new document starts tap to time afresh. */
  function newDocKey() {
    lastTap = null;
    return get().docKey + 1;
  }

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
    const show = useApp.getState().snapshot?.path ?? null;
    const recent = [{ path, show }, ...get().recent.filter((r) => r.path !== path)].slice(0, RECENT_LIMIT);
    saveRecent(recent);
    set({ recent });
  }

  /** Like `absorb`, for undo and redo: when the step also took back (or brought back) a show change
   * made together with it (an assistant proposal), the show fetches itself again. */
  async function absorbPaired(result: SequenceEditResult, from: SequencerApi) {
    await absorb(result, from);
    const show = useApp.getState().snapshot;
    if (result.changed && result.showRevision !== undefined && show && result.showRevision > show.revision) {
      void useApp.getState().run((backend) => backend.getSnapshot());
    }
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

  /** Undo (or redo) in the sequence, keeping the Undo and Redo names in step. */
  async function step(redo: boolean): Promise<boolean> {
    const { api } = get();
    if (!api || !get().doc) return false;
    const ok = await serial(() =>
      guarded(async () => {
        const from = get().revision;
        await absorbPaired(await (redo ? api.redoSequence() : api.undoSequence()), api);
        if (get().revision !== from) useUndoLabels.setState({ sequence: stepped(useUndoLabels.getState().sequence, from, get().revision, redo) });
        return true;
      }),
    );
    return ok === true;
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
    markSelection: null,
    activeTrack: null,
    activeRow: null,
    playheadMs: 0,
    status: null,
    sendToControllers: false,
    looping: loadLoop(),
    snapping: true,
    timingHidden: loadTimingHidden(),
    collapsed: [],
    clipboard: [],
    recent: loadRecent(),
    exporting: null,
    suggestBeats: false,
    detecting: false,
    findingLyrics: null,
    docKey: 0,
    revealAt: 0,
    revealTarget: "selection",
    recoveries: [],
    notice: null,
    retimingLyrics: false,
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
        await api.setSequenceDocLoop(get().looping);
      });
    },

    async closeDocument() {
      const { api, doc } = get();
      if (!api || !doc) return;
      await serial(async () => {
        await halt();
        await guarded(() => api.closeSequenceDoc());
        set({
          doc: null,
          path: null,
          dirty: false,
          canUndo: false,
          canRedo: false,
          issues: [],
          selection: [],
          markSelection: null,
          activeTrack: null,
          activeRow: null,
          playheadMs: 0,
          collapsed: [],
          suggestBeats: false,
          notice: null,
          musicMissing: null,
          docKey: newDocKey(),
        });
      });
    },

    async newSequence(name, durationMs, audio, rows) {
      const { api } = get();
      if (!api) return false;
      const ok = await serial(() =>
        guarded(async () => {
          await halt();
          // With its music from the start: nothing to undo, nothing unsaved.
          adopt(await api.newSequenceDoc(name, durationMs, audio, rows));
          set({ selection: [], markSelection: null, activeTrack: null, playheadMs: 0, collapsed: [], suggestBeats: audio !== null, docKey: newDocKey(), notice: null });
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
          await halt();
          adopt(await api.openSequenceDoc(path));
          set({ selection: [], markSelection: null, activeTrack: null, playheadMs: 0, collapsed: [], suggestBeats: false, docKey: newDocKey(), notice: null });
          return true;
        } catch (e) {
          // A recent file that can't be opened any more (moved or deleted) comes off the list.
          const recent = get().recent;
          if (recent.some((r) => r.path === path)) {
            const left = recent.filter((r) => r.path !== path);
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

    async importXlights(path, options) {
      const { api } = get();
      if (!api) return null;
      return serial(() =>
        guarded(async () => {
          await halt();
          const imported = options ? await api.importXlightsSequence(path, options) : await api.importXlightsSequence(path);
          // Opened like any other document: unsaved, so it's kept (autosaved) until it's saved.
          adopt(imported.snapshot);
          set({ selection: [], markSelection: null, activeTrack: null, playheadMs: 0, collapsed: [], suggestBeats: false, docKey: newDocKey(), notice: null });
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

    async save(options) {
      const { api, path } = get();
      if (!api || !get().doc) return false;
      if (!path) return get().saveAs(options);
      const ok = await serial(() => guarded(async () => (adopt(await api.saveSequenceDoc()), true)));
      return saidSaved(ok === true, get().doc?.name, options);
    },

    async saveAs(options) {
      const { api, doc, path } = get();
      if (!api || !doc) return false;
      const target = await guarded(() => api.pickSequenceDocSavePath(path ? fileName(path) : `${doc.name}.pfseq.json`));
      if (!target) return false;
      const ok = await serial(() => guarded(async () => (adopt(await api.saveSequenceDocAs(target)), true)));
      return saidSaved(ok === true, get().doc?.name, options);
    },

    async edit(edits, gesture) {
      const { api } = get();
      if (!api || (Array.isArray(edits) && edits.length === 0)) return false;
      const ok = await serial(() =>
        guarded(async () => {
          const doc = get().doc;
          const batch = typeof edits === "function" ? (doc ? edits(doc) : []) : edits;
          if (batch.length === 0) return true;
          const from = get().revision;
          await absorb(await api.editSequence(batch, gesture), api);
          if (doc && get().revision !== from) {
            const catalog = get().catalog;
            const label = describeSequenceEdits(batch, doc, (kind) => catalog.find((c) => c.kind === kind)?.label ?? kind);
            useUndoLabels.setState({ sequence: edited(useUndoLabels.getState().sequence, from, get().revision, label, gesture ?? null) });
          }
          return true;
        }),
      );
      return ok === true;
    },

    async undo() {
      return step(false);
    },

    async redo() {
      return step(true);
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

    async findLyrics(provider, upload, options = { language: "en", fresh: false }) {
      const { api } = get();
      if (!api || get().findingLyrics !== null) return null;
      set({ findingLyrics: "Starting", notice: null });
      try {
        let found: LyricsFound;
        try {
          found = await api.findLyrics(provider, upload, options, (label) => set({ findingLyrics: label }));
        } catch (e) {
          if (errorMessage(e) !== "Stopped.") report(e);
          return null;
        }
        await serial(() => guarded(() => absorb(found.result, api)));
        set({ notice: lyricsNotice(found, { provider, upload }) });
        return found;
      } finally {
        set({ findingLyrics: null });
      }
    },

    async chooseLyrics(choice) {
      const { api } = get();
      if (!api || get().findingLyrics !== null) return null;
      const run = get().notice?.lyricsRun;
      set({ findingLyrics: "Lining up the words" });
      try {
        let found: LyricsFound;
        try {
          found = await api.chooseLyrics(choice);
        } catch (e) {
          report(e);
          return null;
        }
        await serial(() => guarded(() => absorb(found.result, api)));
        set({ notice: lyricsNotice(found, run) });
        return found;
      } finally {
        set({ findingLyrics: null });
      }
    },

    async cancelLyrics() {
      await get().api?.cancelLyrics();
    },

    async nudgeLyrics(trackId, ms) {
      const { api } = get();
      if (!api || !get().doc || ms === 0) return false;
      const done = await serial(() =>
        guarded(async () => {
          await absorb(await api.nudgeLyrics(trackId, ms), api);
          return true;
        }),
      );
      return done === true;
    },

    async retimeLyrics(trackId) {
      const { api } = get();
      if (!api || !get().doc || get().retimingLyrics) return false;
      set({ retimingLyrics: true });
      try {
        let retimed;
        try {
          retimed = await api.retimeLyrics(trackId);
        } catch (e) {
          report(e);
          return false;
        }
        await serial(() => guarded(() => absorb(retimed.result, api)));
        set({ notice: { tone: "done", text: retimed.note, notes: [], saveShow: false } });
        return true;
      } finally {
        set({ retimingLyrics: false });
      }
    },

    async syllablesFromWords(trackId) {
      const { api } = get();
      if (!api || !get().doc) return false;
      const done = await serial(() =>
        guarded(async () => {
          await absorb(await api.syllablesFromWords(trackId), api);
          return true;
        }),
      );
      return done === true;
    },

    setActiveTrack: (activeTrack) => {
      if (activeTrack !== get().activeTrack) lastTap = null;
      set({ activeTrack });
    },

    select: (ids, activeRow) => {
      const marks = { markSelection: null };
      set(activeRow === undefined ? { selection: ids, ...marks } : { selection: ids, activeRow, ...marks });
    },
    selectMarks: (track, starts) => {
      if (track !== get().activeTrack) lastTap = null;
      set({ markSelection: starts.length > 0 ? { track, starts } : null, activeTrack: track, selection: starts.length > 0 ? [] : get().selection });
    },

    tap() {
      const { doc, activeTrack, status, playheadMs } = get();
      if (!doc) return;
      const track = doc.timingTracks.find((t) => t.id === activeTrack);
      if (!track) {
        report("Pick a timing track first: click its name, then press T in time with the music.");
        return;
      }
      if (track.kind === "phonemes") {
        report("Phoneme tracks can't be edited mark by mark; tap onto another track.");
        return;
      }
      // Between polls the music has moved on from the last playhead the player reported.
      const atMs = status?.state === "playing" ? Math.min(doc.durationMs, playheadMs + (performance.now() - playheadAt)) : playheadMs;
      const id = track.id;
      let started: { track: string; startMs: number } | null = null;
      void get()
        .edit((latest) => {
          const now = latest.timingTracks.find((t) => t.id === id);
          if (!now) return [];
          const tapped = tapEdits(now, atMs, lastTap?.track === id ? lastTap.startMs : null, latest.durationMs);
          if (!tapped) return [];
          // The next tap (built after this one lands) ends the mark this one starts.
          started = lastTap = { track: id, startMs: tapped.startMs };
          return tapped.edits;
        })
        .then((ok) => {
          // Refused: the next tap starts a fresh run rather than ending a mark that isn't there.
          if (!ok && started && lastTap === started) lastTap = null;
        });
    },

    async importTiming() {
      const { api } = get();
      if (!api || !get().doc) return false;
      const path = await guarded(() => api.pickTimingFilePath());
      if (!path) return false;
      // Reading the file takes a moment; edits carry on meanwhile, and the tracks are taken in at
      // their turn.
      const imported = await guarded(() => api.importTimingFile(path));
      if (!imported) return false;
      await serial(() => guarded(() => absorb(imported.result, api)));
      const names = imported.tracks.map((n) => `'${n}'`).join(", ");
      set({
        notice: { tone: "done", text: `Added ${plural(imported.tracks.length, "timing track")} from ${fileName(path)}: ${names}.`, notes: imported.notes, saveShow: false },
      });
      return true;
    },

    async exportTiming(trackId) {
      const { api, doc } = get();
      const track = doc?.timingTracks.find((t) => t.id === trackId);
      if (!api || !track) return false;
      // A file name can't hold the characters some names have ("AC/DC").
      const target = await guarded(() => api.pickTimingExportPath(`${track.name.replace(/[\\/:*?"<>|]/g, "-")}.xtiming`));
      if (!target) return false;
      // Export what's on screen: every edit made so far lands first.
      await serial(async () => undefined);
      const count = await guarded(() => api.exportTimingTrack(trackId, target));
      if (count === null) return false;
      set({ notice: { tone: "done", text: `Exported '${track.name}' (${plural(count, "mark")}) to ${fileName(target)}.`, notes: [], saveShow: false } });
      return true;
    },
    setActiveRow: (activeRow) => set({ activeRow }),
    setPlayhead: (ms) => {
      const duration = get().doc?.durationMs ?? 0;
      set({ playheadMs: Math.max(0, Math.min(duration, Math.round(ms))) });
    },
    toggleCollapsed: (rowId) => {
      const collapsed = get().collapsed;
      set({ collapsed: collapsed.includes(rowId) ? collapsed.filter((id) => id !== rowId) : [...collapsed, rowId] });
    },
    toggleTimingHidden: () => {
      const timingHidden = !get().timingHidden;
      set({ timingHidden });
      saveTimingHidden(timingHidden);
    },
    setSnapping: (snapping) => set({ snapping }),

    async setSendToControllers(on) {
      set({ sendToControllers: on });
      const status = await guarded(() => get().api!.setSequenceDocOutput(on));
      if (status?.authored) set({ status });
    },

    setLooping(on) {
      set({ looping: on });
      saveLoop(on);
      const api = get().api;
      if (!api) return;
      const turn = transport;
      void guarded(() => api.setSequenceDocLoop(on)).then((status) => {
        if (status?.authored && turn === transport && get().status) set({ status });
      });
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
      lastTap = null;
      if (status && status.state === "paused") {
        const next = await guarded(() => backend.pausePlayback(false));
        if (next && turn === transport) set({ status: next });
        return;
      }
      const from = playheadMs >= doc.durationMs ? 0 : playheadMs;
      const next = await guarded(() => api.playSequenceDoc(from));
      if (next && turn === transport) {
        playheadAt = performance.now();
        set({ status: next, playheadMs: next.positionMs });
      }
    },

    async pause() {
      const backend = useApp.getState().backend;
      if (!backend || !get().status) return;
      const turn = ++transport;
      lastTap = null;
      const next = await guarded(() => backend.pausePlayback(true));
      if (next && turn === transport) {
        playheadAt = performance.now();
        set({ status: next, playheadMs: next.positionMs });
      }
    },

    async stop() {
      if (get().status) {
        await halt();
        return;
      }
      lastTap = null;
      get().setPlayhead(0);
      get().reveal("playhead");
    },

    async seek(ms) {
      lastTap = null;
      get().setPlayhead(ms);
      playheadAt = performance.now();
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
        playheadAt = performance.now();
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

    settled: () => serial(async () => undefined),

    async cancelExport() {
      cancelled = true;
      await guarded(() => get().api!.cancelSequenceExport());
    },

    async recover(id) {
      const { api } = get();
      if (!api) return false;
      const ok = await serial(() =>
        guarded(async () => {
          await halt();
          adopt(await api.recoverSequence(id));
          set({
            selection: [],
            markSelection: null,
            activeTrack: null,
            playheadMs: 0,
            collapsed: [],
            suggestBeats: false,
            docKey: newDocKey(),
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

    musicMissing: null,

    async checkMusic() {
      const { api } = get();
      if (!api || !get().doc?.audio) {
        set({ musicMissing: null });
        return;
      }
      const missing = await guarded(() => api.sequenceMusicMissing());
      if (get().api === api) set({ musicMissing: missing });
    },

    async findMusic() {
      const { api } = get();
      const missing = get().musicMissing;
      if (!api || !missing) return false;
      const ok = await serial(() =>
        guarded(async () => {
          const { found, result, gaveUp } = await api.findSequenceMusic();
          if (result) await absorb(result, api);
          set({
            notice: found
              ? { tone: "done", text: `Found ${found.name} in ${shownPath(folderOf(found.to))}. Undo puts the old place back.`, notes: [], saveShow: false }
              : {
                  tone: "info",
                  text: gaveUp
                    ? `PixelFlow stopped looking for ${missing.name} before it had checked every folder. Use Locate… to choose it.`
                    : `PixelFlow couldn't find ${missing.name} in the sequence's or the show's folder. Use Locate… to choose it.`,
                  notes: [],
                  saveShow: false,
                },
          });
          return found !== null;
        }),
      );
      await get().checkMusic();
      return ok === true;
    },

    async locateMusic() {
      const { api } = get();
      if (!api || !get().doc) return false;
      const ok = await serial(() =>
        guarded(async () => {
          const result = await api.locateSequenceMusic();
          if (result) await absorb(result, api);
          return result !== null;
        }),
      );
      await get().checkMusic();
      return ok === true;
    },

    dismissBeats: () => set({ suggestBeats: false }),
    reveal: (target = "selection") => set({ revealAt: get().revealAt + 1, revealTarget: target }),
  };
});
