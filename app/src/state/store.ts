import { create } from "zustand";
import { type Backend, errorMessage } from "../api/backend";
import { useSequencer } from "./sequencer";
import type {
  Device,
  Edit,
  FileRole,
  FoundFile,
  ImportSummary,
  MissingFile,
  RecentShow,
  SequenceImportSummary,
  Show,
  ShowSnapshot,
  SilentPeer,
} from "../api/types";
import { fileName } from "../lib/format";
import { sameFile } from "../lib/showFiles";
import { useLayoutEditor } from "./layoutEditor";
import { toast } from "./toast";
import { edited, stepped, useUndoLabels } from "./undoLabels";
import { describeShowEdits } from "../lib/describeChange";
import { showViewKey, useView3d } from "./view3d";

/**
 * Edits to send: a fixed list, or a function that builds them from the show as it is when
 * their turn comes (after every earlier change has landed), so they never undo a change that
 * was still on its way.
 */
export type EditsFrom = Edit[] | ((show: Show) => Edit[]);

/** What a call to the engine does to the show's undo history. */
type TurnKind = "edit" | "undo" | "redo" | "other";

/** `quiet`: no "Saved …" toast (the caller says what it saved itself). */
export interface SaveOptions {
  quiet?: boolean;
}

/** Says the show or sequence was saved, unless asked not to (an event passed as options is ignored). */
export function saidSaved(ok: boolean, name: string | undefined, options?: SaveOptions): boolean {
  if (ok && name && options?.quiet !== true) toast(`Saved ${name}`);
  return ok;
}

export type Screen = "layout" | "wiring" | "devices" | "sequence" | "play" | "test" | "cameraMap" | "history" | "settings";

/**
 * Something that leaves the open show: a new show, the Open dialog, the xLights import, the demo
 * show, closing it, or one of the recent shows (opened, or located where it went).
 */
export type ReplaceKind = "new" | "open" | "xlights" | "sample" | "close" | { recent: string } | { locate: string };

/** A new show's name until the user gives it one. */
export const UNTITLED = "Untitled Show";
export type Theme = "dark" | "light";
/** The theme chosen: light, dark, or whatever the computer is set to. */
export type ThemeChoice = Theme | "system";

const THEME_KEY = "pixelflow.theme";

/**
 * The theme chosen on this computer. A new install follows the computer; one that has been used
 * before without choosing keeps the dark it always had (saved once, so it stays chosen).
 */
export function initialThemeChoice(): ThemeChoice {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "dark" || saved === "system") return saved;
    const usedBefore = Object.keys(localStorage).some((k) => k.startsWith("pixelflow."));
    const choice = usedBefore ? "dark" : "system";
    localStorage.setItem(THEME_KEY, choice);
    return choice;
  } catch {
    return "system";
  }
}

/** The computer's light or dark setting (dark where it can't be told). */
export function systemTheme(): Theme {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return "dark";
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

const resolveTheme = (choice: ThemeChoice): Theme => (choice === "system" ? systemTheme() : choice);

interface AppState {
  backend: Backend | null;
  snapshot: ShowSnapshot | null;
  /** False until the user leaves the welcome screen. */
  started: boolean;
  screen: Screen;
  /** Names the open show for this run of the app: a new one each time another show takes its place. */
  showId: string;
  /** The theme in use, light or dark. */
  theme: Theme;
  themeChoice: ThemeChoice;
  paletteOpen: boolean;
  error: string | null;
  busy: boolean;
  /** Set when something would leave the show while it (or the open sequence, which closes with
   * it) has unsaved changes: the question is showing. (Replacing only the open sequence asks
   * through the sequencer's own question: see `useSequencer.replaceAfterAsking`.) */
  pendingReplace: ReplaceKind | null;
  /** Shows opened or saved lately, newest first (the shell keeps the list). */
  recent: RecentShow[];
  /** What's being opened, said while it happens ("Opening House…"), or null. */
  opening: string | null;
  /** The show menu in the top bar: closed, open, or open with its recent shows in focus. */
  showMenu: "closed" | "open" | "recent";
  /** True while the show's name is being edited in the top bar. */
  renaming: boolean;
  /** Asking for the show's name before its first save: the name offered. */
  naming: string | null;
  /** What the last xLights import brought in, shown until dismissed. */
  importReport: { name: string; summary: ImportSummary; notes: string[] } | null;
  /** What the last xLights sequence import brought in, shown until dismissed. */
  sequenceImportReport: { name: string; summary: SequenceImportSummary; notes: string[] } | null;
  /** What the last search for missing files found, shown until dismissed. */
  filesReport: { found: FoundFile[]; stillMissing: MissingFile[]; gaveUp: boolean } | null;
  /** The show (by path) whose "files aren't where they were" notice was put away. */
  missingNoticeDismissed: string | null;
  /** Test screen target selection; kept here so it survives leaving the screen. */
  testTarget: string;
  /** Music volume (0–1) for playback; the engine keeps the same value. */
  musicVolume: number;
  /** The last device scan's results (kept while moving between screens). */
  /** Every controller found so far (remembered on this computer), plus the last scan's silent peers. */
  discovery: { devices: KnownDevice[]; silent: SilentPeer[] } | null;
  scanning: boolean;

  connect(backend: Backend): Promise<void>;
  setScreen(screen: Screen): void;
  /** Chooses light, dark, or the computer's setting ("system"), remembered on this computer. */
  setTheme(theme: ThemeChoice): void;
  setPaletteOpen(open: boolean): void;
  setTestTarget(value: string): void;
  setMusicVolume(volume: number): void;
  resolvePendingReplace(choice: "save" | "discard" | "cancel"): Promise<boolean>;
  dismissError(): void;
  /**
   * Runs a backend call that returns a new snapshot; errors become a message. Returns success.
   * Calls run one at a time, in the order they were made, so each starts from the show the
   * previous one left.
   */
  run(call: (backend: Backend) => Promise<ShowSnapshot>): Promise<boolean>;
  apply(edits: EditsFrom): Promise<boolean>;
  /**
   * Like `apply`, but resolves with the revision of the show that includes the edits (the
   * current one when there was nothing to change), or null when they were refused.
   */
  edit(edits: EditsFrom): Promise<number | null>;
  undo(): Promise<boolean>;
  redo(): Promise<boolean>;
  newShow(): Promise<boolean>;
  openShow(): Promise<boolean>;
  /** Opens a recent show through the same open as the Open dialog (asks about unsaved work first). */
  openRecent(path: string): Promise<boolean>;
  /** For a recent show that has moved: asks where it is now, and opens it. */
  locateRecent(path: string): Promise<boolean>;
  forgetRecent(path: string): Promise<void>;
  clearRecent(): Promise<void>;
  refreshRecent(): Promise<void>;
  /** Opens the demo show as a new, unsaved show. */
  openSample(): Promise<boolean>;
  /** Leaves the show (and its open sequence) for the start page, asking about unsaved work. */
  closeShow(): Promise<boolean>;
  /** Renames the show (one undo step); blank or unchanged names change nothing. */
  renameShow(name: string): Promise<boolean>;
  setShowMenu(menu: "closed" | "open" | "recent"): void;
  setRenaming(renaming: boolean): void;
  /** Answers "Name your show": the name, or null to cancel the save. */
  resolveNaming(name: string | null): void;
  /** Imports an xLights show folder as a new show (asks about unsaved changes first). */
  importXlights(): Promise<boolean>;
  dismissImportReport(): void;
  /** Imports an xLights sequence onto the open show and opens it in the sequence editor (asks
   * about the open sequence's unsaved changes first, like New and Open on the Sequence screen). */
  importXlightsSequence(): Promise<boolean>;
  dismissSequenceImportReport(): void;
  /** Saves the show (asking where the first time); a toast says so unless `quiet`. */
  save(options?: SaveOptions): Promise<boolean>;
  saveAs(options?: SaveOptions): Promise<boolean>;
  /**
   * Looks for the show's missing files (or only `file`) in the show's folder and points the show
   * at what it finds (one undo step), then shows what was found.
   */
  findMissingFiles(file?: FileRole): Promise<boolean>;
  /** Asks where a missing file is now and points the show at it (one undo step). */
  locateFile(file: FileRole): Promise<boolean>;
  dismissFilesReport(): void;
  /**
   * Asks the backend to look at the show's files (those not looked at yet, or `all`). It runs
   * beside edits, not in their line, so a slow drive never holds them up; one runs at a time.
   */
  checkFiles(all: boolean): Promise<void>;
  /** Puts away the "files aren't where they were" notice for the open show. */
  dismissMissingNotice(): void;
  /** Forgets a remembered controller. */
  forgetDevice(address: string): void;
  /** Looks for controllers and re-checks every remembered one; `hosts` checks only those
   * addresses. Found controllers are remembered; ones that don't answer stay, marked. */
  scan(hosts?: string[]): Promise<boolean>;
  /** From the welcome screen: start a new show, open Controllers, and scan. */
  discoverFromWelcome(): Promise<void>;
}

/** A controller PixelFlow has found, and whether it answered the last time it was checked. */
export type KnownDevice = Device & { responding: boolean; lastSeen: number };

const DEVICES_KEY = "pixelflow.devices";

/** A remembered controller read back from storage, or null when the entry isn't usable. */
function knownDevice(value: unknown): KnownDevice | null {
  if (typeof value !== "object" || value === null) return null;
  const d = value as Record<string, unknown>;
  if (typeof d.address !== "string" || typeof d.kind !== "string" || typeof d.name !== "string") return null;
  if (!Array.isArray(d.foundBy) || !d.foundBy.every((f) => typeof f === "string")) return null;
  const text = (v: unknown) => (typeof v === "string" ? v : "");
  return {
    address: d.address,
    kind: d.kind as KnownDevice["kind"],
    name: d.name,
    model: text(d.model),
    firmware: text(d.firmware),
    mode: typeof d.mode === "string" ? d.mode : null,
    foundBy: d.foundBy as KnownDevice["foundBy"],
    responding: d.responding === true,
    lastSeen: typeof d.lastSeen === "number" && Number.isFinite(d.lastSeen) ? d.lastSeen : 0,
  };
}

function loadKnownDevices(): KnownDevice[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(DEVICES_KEY) ?? "[]");
    if (!Array.isArray(saved)) return [];
    return saved.map(knownDevice).filter((d): d is KnownDevice => d !== null);
  } catch {
    return [];
  }
}

function saveKnownDevices(devices: KnownDevice[]) {
  try {
    localStorage.setItem(DEVICES_KEY, JSON.stringify(devices));
  } catch {
    // Storage unavailable; the list still works for this session.
  }
}

const KIND_ORDER: Record<string, number> = { fpp: 0, falcon: 1, wled: 2 };

/** Kind, then address in numeric order (10.0.0.9 before 10.0.0.10). */
function byKindThenAddress(a: KnownDevice, b: KnownDevice): number {
  const key = (d: KnownDevice) => d.address.split(".").map((part) => part.padStart(3, "0")).join(".");
  return (KIND_ORDER[a.kind] ?? 9) - (KIND_ORDER[b.kind] ?? 9) || key(a).localeCompare(key(b));
}

/**
 * After a show undo or redo: when it also took back (or brought back) a sequence change made
 * together with it (an assistant proposal), the open sequence fetches itself again.
 */
async function withPairedSequence(done: Promise<boolean>): Promise<boolean> {
  const ok = await done;
  const paired = useApp.getState().snapshot?.sequenceRevision;
  const sequencer = useSequencer.getState();
  if (ok && typeof paired === "number" && sequencer.doc && paired !== sequencer.revision) {
    await sequencer.refreshIssues();
  }
  return ok;
}

/** Which show a "files aren't where they were" notice belongs to. */
export function missingNoticeKey(snapshot: ShowSnapshot | null): string {
  return snapshot?.path ?? "(unsaved)";
}

export const useApp = create<AppState>((set, get) => {
  /** Says what's happening while a show opens (and logs how long it took, for debugging). */
  async function opening<T>(text: string, work: () => Promise<T>): Promise<T> {
    set({ opening: text });
    try {
      return await work();
    } finally {
      set({ opening: null });
    }
  }

  /** Replaces the current show without checking for unsaved changes. The open sequence goes
   * with it once the new show is open. */
  async function replaceShow(kind: ReplaceKind): Promise<boolean> {
    const backend = get().backend;
    if (!backend) return false;
    const asked = performance.now();
    let picked = asked;
    let ok = false;
    try {
      if (kind === "xlights") {
        const folder = await opening("Opening the folder dialog…", () => backend.pickShowFolder());
        if (!folder) return false;
        picked = performance.now();
        ok = await opening(`Importing ${fileName(folder)}…`, () =>
          get().run(async (b) => {
            const imported = await b.importXlights(folder);
            set({ importReport: { name: imported.snapshot.show.name, summary: imported.summary, notes: imported.notes } });
            return imported.snapshot;
          }),
        );
      } else if (kind === "new" || kind === "close") {
        ok = await get().run((b) => b.newShow(UNTITLED));
      } else if (kind === "sample") {
        ok = await opening("Opening the demo show…", () => get().run((b) => b.openSampleShow()));
      } else if (kind === "open") {
        const path = await opening("Opening the file dialog…", () => backend.pickOpenPath());
        if (!path) return false;
        picked = performance.now();
        ok = await opening(`Opening ${fileName(path)}…`, () => get().run((b) => b.openShow(path)));
      } else if ("recent" in kind) {
        const path = kind.recent;
        const name = get().recent.find((r) => r.path === path)?.name ?? fileName(path);
        ok = await opening(`Opening ${name}…`, () => get().run((b) => b.openShow(path)));
        if (!ok) {
          // It stays on the list, marked, with Locate… and Remove from list.
          await get().refreshRecent();
          const error = get().error;
          set({ error: `${error ?? `${name} couldn't be opened.`} It's still in your recent shows: use Locate… to find it, or remove it from the list.` });
        }
      } else {
        const path = kind.locate;
        let cancelled = false;
        ok = await opening("Opening the file dialog…", () =>
          get().run(async (b) => {
            const located = await b.locateRecentShow(path);
            if (located) return located;
            cancelled = true;
            return b.getSnapshot();
          }),
        );
        if (cancelled) return false;
      }
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    }
    if (!ok) return false;
    set({ showId: crypto.randomUUID() });
    // The open sequence belongs to the show being left.
    await useSequencer.getState().closeDocument();
    if (kind === "close") {
      set({ started: false, showMenu: "closed" });
    } else {
      set({ started: true, screen: "layout", showMenu: "closed" });
      // A different show: start the layout editor fresh, fitted to it.
      useLayoutEditor.setState({ selected: [], view: null, editPhoto: false, photoDraft: null, tool: "select", nudge: null, highlight: null });
    }
    void get().refreshRecent();
    if (typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() =>
        console.debug(
          `[pixelflow] ${typeof kind === "string" ? kind : Object.keys(kind)[0]}: chosen after ${Math.round(picked - asked)} ms, ` +
            `ready ${Math.round(performance.now() - picked)} ms after that`,
        ),
      );
    }
    return true;
  }

  /** Leaves the show for `kind`, first asking about unsaved work (the show's and the open
   * sequence's, in one question) when there is any. */
  function leaveShow(kind: ReplaceKind): Promise<boolean> {
    const sequencer = useSequencer.getState();
    const unsaved = (get().started && get().snapshot?.dirty) || (sequencer.doc !== null && sequencer.dirty);
    if (unsaved) {
      set({ pendingReplace: kind, showMenu: "closed" });
      return Promise.resolve(false);
    }
    return replaceShow(kind);
  }

  /** Resolves the "Name your show" question. */
  let answerName: ((name: string | null) => void) | null = null;

  /** Asks for the show's name (before its first save); null when cancelled. */
  function askName(offered: string): Promise<string | null> {
    answerName?.(null);
    set({ naming: offered });
    return new Promise((resolve) => {
      answerName = (name) => {
        answerName = null;
        set({ naming: null });
        resolve(name);
      };
    });
  }

  /** Picks an xLights sequence and opens its import on the Sequence screen, replacing the open
   * sequence without checking for unsaved changes. */
  async function replaceSequenceWithImport(): Promise<boolean> {
    const sequencer = useSequencer.getState();
    if (!sequencer.api) return false;
    let path: string | null;
    try {
      path = await sequencer.api.pickXlightsSequencePath();
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    }
    if (!path) return false;
    set({ busy: true, opening: `Importing ${fileName(path)}…` });
    try {
      const imported = await sequencer.importXlights(path);
      if (!imported) return false;
      set({
        sequenceImportReport: {
          name: imported.snapshot.sequence.name,
          summary: imported.summary,
          notes: imported.notes,
        },
        error: null,
        started: true,
        screen: "sequence",
      });
      return true;
    } finally {
      set({ busy: false, opening: null });
    }
  }

  /** Controllers forgotten while a scan was running, so its results don't bring them back. */
  const forgottenDuringScan = new Set<string>();

  /** The end of the line of backend calls; each new call waits for the one before it. */
  let queue: Promise<unknown> = Promise.resolve();

  /** Set while a check of the show's files runs (one at a time). */
  let checkingFiles = false;

  /**
   * Keeps the Undo and Redo names in step with a call that took the show from `before` to
   * `after`: an edit (named by `label`), an undo, a redo, or anything else (which puts the names
   * aside until the next edit).
   */
  function trackUndoNames(before: ShowSnapshot | null, after: ShowSnapshot, kind: TurnKind, label: (() => string) | undefined) {
    if (!before || before.revision === after.revision) return;
    const names = useUndoLabels.getState().show;
    const next =
      kind === "edit" && label
        ? edited(names, before.revision, after.revision, label())
        : kind === "undo" || kind === "redo"
          ? stepped(names, before.revision, after.revision, kind === "redo")
          : { undo: [], redo: [], at: null };
    useUndoLabels.setState({ show: next });
  }

  /** Runs `call` once every earlier call has finished; resolves with its snapshot, or null on failure. */
  function runInTurn(
    call: (backend: Backend) => Promise<ShowSnapshot>,
    kind: TurnKind = "other",
    label?: (before: ShowSnapshot) => string,
  ): Promise<ShowSnapshot | null> {
    const turn = queue.then(async () => {
      const backend = get().backend;
      if (!backend) return null;
      set({ busy: true });
      try {
        const before = get().snapshot;
        const snapshot = await call(backend);
        trackUndoNames(before, snapshot, kind, before && label ? () => label(before) : undefined);
        const current = get().snapshot;
        // Engine revisions only increase, so never go backwards.
        if (!current || snapshot.revision >= current.revision) {
          set({ snapshot });
          if (!snapshot.filesChecked) void get().checkFiles(false);
          // The photo was removed (or its adding undone): there's nothing left to move.
          const editor = useLayoutEditor.getState();
          if (!snapshot.show.background && (editor.editPhoto || editor.photoDraft)) {
            useLayoutEditor.setState({ editPhoto: false, photoDraft: null });
          }
        }
        set({ error: null });
        return snapshot;
      } catch (e) {
        set({ error: errorMessage(e) });
        return null;
      } finally {
        set({ busy: false });
      }
    });
    queue = turn;
    return turn;
  }

  /** Sends the edits (built from the latest show, if they're a function) when their turn comes. */
  function sendEdits(edits: EditsFrom): Promise<ShowSnapshot | null> {
    let sent: Edit[] = [];
    return runInTurn(
      async (backend) => {
        const current = get().snapshot;
        const batch = typeof edits === "function" ? (current ? edits(current.show) : []) : edits;
        sent = batch;
        if (batch.length === 0 && current) return current;
        return backend.applyEdits(batch);
      },
      "edit",
      (before) => describeShowEdits(sent, before.show),
    );
  }

  /** Commits an in-progress text edit (e.g. a prop rename) before saving. */
  function commitFocusedField() {
    if (typeof document === "undefined") return;
    const active = document.activeElement as HTMLElement | null;
    if (active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA")) active.blur();
  }

  return {
  backend: null,
  snapshot: null,
  started: false,
  screen: "layout",
  showId: crypto.randomUUID(),
  ...(() => {
    const themeChoice = initialThemeChoice();
    return { themeChoice, theme: resolveTheme(themeChoice) };
  })(),
  paletteOpen: false,
  error: null,
  busy: false,
  pendingReplace: null,
  recent: [],
  opening: null,
  showMenu: "closed",
  renaming: false,
  naming: null,
  importReport: null,
  sequenceImportReport: null,
  filesReport: null,
  missingNoticeDismissed: null,
  testTarget: "show",
  musicVolume: 1,
  discovery: null,
  scanning: false,

  async connect(backend) {
    // Calls still waiting on a previous backend have nothing to do with this one.
    queue = Promise.resolve();
    const known = loadKnownDevices();
    set({ backend, discovery: known.length ? { devices: known.sort(byKindThenAddress), silent: [] } : get().discovery });
    try {
      const snapshot = await backend.getSnapshot();
      set({ snapshot });
      if (!snapshot.filesChecked) void get().checkFiles(false);
      void get().refreshRecent();
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  setScreen: (screen) => set({ screen }),

  setTheme(choice) {
    try {
      localStorage.setItem(THEME_KEY, choice);
    } catch {
      // Storage unavailable; the theme still applies for this session.
    }
    set({ themeChoice: choice, theme: resolveTheme(choice) });
  },

  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  setTestTarget: (testTarget) => set({ testTarget }),
  setMusicVolume: (musicVolume) => set({ musicVolume }),
  dismissError: () => set({ error: null }),

  run: async (call) => (await runInTurn(call)) !== null,
  apply: async (edits) => (await sendEdits(edits)) !== null,
  edit: async (edits) => (await sendEdits(edits))?.revision ?? null,
  undo: () => withPairedSequence(runInTurn((b) => b.undo(), "undo").then((s) => s !== null)),
  redo: () => withPairedSequence(runInTurn((b) => b.redo(), "redo").then((s) => s !== null)),

  newShow: () => leaveShow("new"),
  openShow: () => leaveShow("open"),
  importXlights: () => leaveShow("xlights"),
  openSample: () => leaveShow("sample"),
  openRecent: (path) => leaveShow({ recent: path }),
  locateRecent: (path) => leaveShow({ locate: path }),

  async closeShow() {
    if (!get().started) return false;
    return leaveShow("close");
  },

  async refreshRecent() {
    const backend = get().backend;
    if (!backend) return;
    try {
      const recent = await backend.listRecentShows();
      if (get().backend === backend) set({ recent });
    } catch {
      // Shown again the next time the list is looked at.
    }
  },

  async forgetRecent(path) {
    const backend = get().backend;
    if (!backend) return;
    set({ recent: get().recent.filter((r) => r.path !== path) });
    try {
      await backend.forgetRecentShow(path);
    } catch (e) {
      set({ error: errorMessage(e) });
    }
    await get().refreshRecent();
  },

  async clearRecent() {
    const backend = get().backend;
    if (!backend) return;
    set({ recent: [] });
    try {
      await backend.clearRecentShows();
    } catch (e) {
      set({ error: errorMessage(e) });
    }
    await get().refreshRecent();
  },

  async renameShow(name) {
    const trimmed = name.trim();
    set({ renaming: false });
    if (!trimmed || trimmed === get().snapshot?.show.name) return false;
    return get().apply([{ type: "renameShow", name: trimmed }]);
  },

  setShowMenu: (showMenu) => set({ showMenu }),
  setRenaming: (renaming) => set({ renaming, showMenu: "closed" }),
  resolveNaming: (name) => answerName?.(name),

  dismissImportReport: () => set({ importReport: null }),

  async importXlightsSequence() {
    // False while asking: the import then runs once the answer allows it.
    return (await useSequencer.getState().replaceAfterAsking(replaceSequenceWithImport)) ?? false;
  },

  dismissSequenceImportReport: () => set({ sequenceImportReport: null }),

  async resolvePendingReplace(choice) {
    const kind = get().pendingReplace;
    if (!kind) return false;
    if (choice === "cancel") {
      set({ pendingReplace: null });
      return false;
    }
    if (choice === "save") {
      // Everything unsaved: the open sequence (it closes with the show), then the show. A save
      // that fails or is cancelled keeps the question up.
      const sequencer = useSequencer.getState();
      if (sequencer.doc && sequencer.dirty && !(await sequencer.save())) return false;
      if (get().started && get().snapshot?.dirty && !(await get().save())) return false;
    }
    set({ pendingReplace: null });
    return replaceShow(kind);
  },

  forgetDevice(address) {
    if (get().scanning) forgottenDuringScan.add(address);
    const discovery = get().discovery;
    if (!discovery) return;
    const devices = discovery.devices.filter((d) => d.address !== address);
    saveKnownDevices(devices);
    set({ discovery: { ...discovery, devices } });
  },

  async scan(hosts = []) {
    const backend = get().backend;
    if (!backend) return false;
    set({ scanning: true });
    forgottenDuringScan.clear();
    try {
      const network = hosts.length === 0;
      // A full scan also checks every remembered controller directly, so it's refreshed even
      // if the network sweep misses it.
      const checking = network ? (get().discovery?.devices ?? []).map((d) => d.address) : hosts;
      const result = await backend.discoverDevices(checking, network);
      const found = { ...result, devices: result.devices.filter((d) => !forgottenDuringScan.has(d.address)) };
      // Merge into the list as it is now: controllers may have been forgotten meanwhile.
      const known = get().discovery?.devices ?? [];
      const now = Date.now();
      const answered = new Map(found.devices.map((d) => [d.address, d]));
      const devices: KnownDevice[] = known.map((d) => {
        const fresh = answered.get(d.address);
        if (fresh) return { ...fresh, responding: true, lastSeen: now };
        return checking.includes(d.address) || network ? { ...d, responding: false } : d;
      });
      for (const d of found.devices) {
        if (!known.some((k) => k.address === d.address)) devices.push({ ...d, responding: true, lastSeen: now });
      }
      devices.sort(byKindThenAddress);
      const silent = found.silent.filter((s) => !devices.some((d) => d.address === s.address));
      saveKnownDevices(devices);
      set({ discovery: { devices, silent }, error: null });
      return true;
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    } finally {
      forgottenDuringScan.clear();
      set({ scanning: false });
    }
  },

  async discoverFromWelcome() {
    if (await get().newShow()) {
      set({ screen: "devices" });
      await get().scan();
    }
  },

  async save(options) {
    commitFocusedField();
    if (!get().snapshot?.path) return get().saveAs(options);
    const ok = await get().run((b) => b.saveShow());
    if (ok) void get().refreshRecent();
    return saidSaved(ok, get().snapshot?.show.name, options);
  },

  findMissingFiles: (file) =>
    get().run(async (b) => {
      const report = await b.findMissingFiles(file);
      set({ filesReport: { found: report.found, stillMissing: report.stillMissing, gaveUp: report.gaveUp } });
      return report.snapshot;
    }),

  async locateFile(file) {
    const ok = await get().run(async (b) => (await b.locateFile(file)) ?? (await b.getSnapshot()));
    // A file located from the search's report comes off its "still missing" list.
    const report = get().filesReport;
    if (ok && report) {
      const missing = get().snapshot?.missingFiles ?? [];
      const stillMissing = report.stillMissing.filter((m) => missing.some((x) => sameFile(x.file, m.file)));
      set({ filesReport: { ...report, stillMissing } });
    }
    return ok;
  },

  dismissFilesReport: () => set({ filesReport: null }),

  async checkFiles(all) {
    const backend = get().backend;
    if (!backend || checkingFiles) return;
    checkingFiles = true;
    let newer = false;
    try {
      const snapshot = await backend.checkFiles(all);
      const current = get().snapshot;
      if (get().backend === backend && (!current || snapshot.revision >= current.revision)) set({ snapshot });
      // An edit landed meanwhile with files of its own to look at.
      newer = get().backend === backend && get().snapshot?.filesChecked === false && snapshot.revision < (get().snapshot?.revision ?? 0);
    } catch {
      // Looked at again on the next change or focus.
    } finally {
      checkingFiles = false;
    }
    if (newer) await get().checkFiles(false);
  },

  dismissMissingNotice: () => set({ missingNoticeDismissed: missingNoticeKey(get().snapshot) }),

  async saveAs(options) {
    commitFocusedField();
    const backend = get().backend;
    let snapshot = get().snapshot;
    if (!backend || !snapshot) return false;
    // A show saved for the first time gets a real name, not "Untitled Show".
    if (!snapshot.path && snapshot.show.name === UNTITLED) {
      const name = (await askName(snapshot.show.name))?.trim();
      if (!name) return false;
      if (name !== snapshot.show.name && !(await get().apply([{ type: "renameShow", name }]))) return false;
      snapshot = get().snapshot ?? snapshot;
    }
    const suggested = snapshot.path ? fileName(snapshot.path) : `${snapshot.show.name}.pixelflow.json`;
    const path = await backend.pickSavePath(suggested);
    if (!path) return false;
    const ok = await get().run(async (b) => {
      const before = get().snapshot ?? snapshot;
      const saved = await b.saveShowAs(path);
      // Before the new path reaches the screens: the 3D camera and photo depth follow the show.
      useView3d.getState().carryShow(showViewKey(before.path, before.show.name), showViewKey(saved.path, saved.show.name));
      return saved;
    });
    if (ok) void get().refreshRecent();
    return saidSaved(ok, get().snapshot?.show.name, options);
  },
};
});
