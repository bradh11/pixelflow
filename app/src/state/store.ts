import { create } from "zustand";
import { type Backend, errorMessage } from "../api/backend";
import type { SequencerApi } from "../api/sequencer";
import type { Device, Edit, ImportSummary, SequenceImportSummary, Show, ShowSnapshot, SilentPeer } from "../api/types";
import { fileName } from "../lib/format";
import { useLayoutEditor } from "./layoutEditor";

/**
 * Edits to send: a fixed list, or a function that builds them from the show as it is when
 * their turn comes (after every earlier change has landed), so they never undo a change that
 * was still on its way.
 */
export type EditsFrom = Edit[] | ((show: Show) => Edit[]);

export type Screen = "layout" | "wiring" | "devices" | "sequence" | "play" | "test" | "history";
export type Theme = "dark" | "light";

const THEME_KEY = "pixelflow.theme";

function storedTheme(): Theme {
  try {
    return localStorage.getItem(THEME_KEY) === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

interface AppState {
  backend: Backend | null;
  snapshot: ShowSnapshot | null;
  /** False until the user leaves the welcome screen. */
  started: boolean;
  screen: Screen;
  theme: Theme;
  paletteOpen: boolean;
  error: string | null;
  busy: boolean;
  /** Set when New/Open was asked for while the show has unsaved changes. */
  pendingReplace: "new" | "open" | "xlights" | null;
  /** What the last xLights import brought in, shown until dismissed. */
  importReport: { name: string; summary: ImportSummary; notes: string[] } | null;
  /** What the last xLights sequence import brought in, shown until dismissed. */
  sequenceImportReport: { name: string; summary: SequenceImportSummary; notes: string[] } | null;
  /** The sequencer side of the engine (authored sequences), once connected. */
  sequencer: SequencerApi | null;
  /** Test screen target selection; kept here so it survives leaving the screen. */
  testTarget: string;
  /** Music volume (0–1) for playback; the engine keeps the same value. */
  musicVolume: number;
  /** The last device scan's results (kept while moving between screens). */
  /** Every controller found so far (remembered on this computer), plus the last scan's silent peers. */
  discovery: { devices: KnownDevice[]; silent: SilentPeer[] } | null;
  scanning: boolean;

  connect(backend: Backend): Promise<void>;
  connectSequencer(sequencer: SequencerApi): void;
  setScreen(screen: Screen): void;
  setTheme(theme: Theme): void;
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
  /** Imports an xLights show folder as a new show (asks about unsaved changes first). */
  importXlights(): Promise<boolean>;
  dismissImportReport(): void;
  /** Imports an xLights sequence onto the open show and opens it in the sequence editor. */
  importXlightsSequence(): Promise<boolean>;
  dismissSequenceImportReport(): void;
  save(): Promise<boolean>;
  saveAs(): Promise<boolean>;
  /** Forgets a remembered controller. */
  forgetDevice(address: string): void;
  /** Looks for controllers and re-checks every remembered one; `hosts` checks only those
   * addresses. Found controllers are remembered; ones that don't answer stay, marked. */
  scan(hosts?: string[]): Promise<boolean>;
  /** From the welcome screen: start a new show, open Devices, and scan. */
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

export const useApp = create<AppState>((set, get) => {
  /** Replaces the current show without checking for unsaved changes. */
  async function replaceShow(kind: "new" | "open" | "xlights"): Promise<boolean> {
    const backend = get().backend;
    if (!backend) return false;
    let ok: boolean;
    if (kind === "xlights") {
      const folder = await backend.pickShowFolder();
      if (!folder) return false;
      ok = await get().run(async (b) => {
        const imported = await b.importXlights(folder);
        set({ importReport: { name: imported.snapshot.show.name, summary: imported.summary, notes: imported.notes } });
        return imported.snapshot;
      });
    } else if (kind === "new") {
      ok = await get().run((b) => b.newShow("Untitled Show"));
    } else {
      const path = await backend.pickOpenPath();
      if (!path) return false;
      ok = await get().run((b) => b.openShow(path));
    }
    if (ok) {
      set({ started: true, screen: "layout" });
      // A different show: start the layout editor fresh, fitted to it.
      useLayoutEditor.setState({ selected: [], view: null, editPhoto: false, photoDraft: null, tool: "select", nudge: null });
    }
    return ok;
  }

  /** Controllers forgotten while a scan was running, so its results don't bring them back. */
  const forgottenDuringScan = new Set<string>();

  /** The end of the line of backend calls; each new call waits for the one before it. */
  let queue: Promise<unknown> = Promise.resolve();

  /** Runs `call` once every earlier call has finished; resolves with its snapshot, or null on failure. */
  function runInTurn(call: (backend: Backend) => Promise<ShowSnapshot>): Promise<ShowSnapshot | null> {
    const turn = queue.then(async () => {
      const backend = get().backend;
      if (!backend) return null;
      set({ busy: true });
      try {
        const snapshot = await call(backend);
        const current = get().snapshot;
        // Engine revisions only increase, so never go backwards.
        if (!current || snapshot.revision >= current.revision) {
          set({ snapshot });
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
    return runInTurn(async (backend) => {
      const current = get().snapshot;
      const batch = typeof edits === "function" ? (current ? edits(current.show) : []) : edits;
      if (batch.length === 0 && current) return current;
      return backend.applyEdits(batch);
    });
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
  theme: storedTheme(),
  paletteOpen: false,
  error: null,
  busy: false,
  pendingReplace: null,
  importReport: null,
  sequenceImportReport: null,
  sequencer: null,
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
      set({ snapshot: await backend.getSnapshot() });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  setScreen: (screen) => set({ screen }),

  setTheme(theme) {
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Storage unavailable; the theme still applies for this session.
    }
    set({ theme });
  },

  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  setTestTarget: (testTarget) => set({ testTarget }),
  setMusicVolume: (musicVolume) => set({ musicVolume }),
  dismissError: () => set({ error: null }),

  run: async (call) => (await runInTurn(call)) !== null,
  apply: async (edits) => (await sendEdits(edits)) !== null,
  edit: async (edits) => (await sendEdits(edits))?.revision ?? null,
  undo: () => get().run((b) => b.undo()),
  redo: () => get().run((b) => b.redo()),

  async newShow() {
    if (get().started && get().snapshot?.dirty) {
      set({ pendingReplace: "new" });
      return false;
    }
    return replaceShow("new");
  },

  async openShow() {
    if (get().started && get().snapshot?.dirty) {
      set({ pendingReplace: "open" });
      return false;
    }
    return replaceShow("open");
  },

  async importXlights() {
    if (get().started && get().snapshot?.dirty) {
      set({ pendingReplace: "xlights" });
      return false;
    }
    return replaceShow("xlights");
  },

  dismissImportReport: () => set({ importReport: null }),

  connectSequencer: (sequencer) => set({ sequencer }),

  async importXlightsSequence() {
    const sequencer = get().sequencer;
    if (!sequencer) return false;
    const path = await sequencer.pickXlightsSequencePath();
    if (!path) return false;
    set({ busy: true });
    try {
      const imported = await sequencer.importXlightsSequence(path);
      set({
        sequenceImportReport: {
          name: imported.snapshot.sequence.name,
          summary: imported.summary,
          notes: imported.notes,
        },
        error: null,
      });
      return true;
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    } finally {
      set({ busy: false });
    }
  },

  dismissSequenceImportReport: () => set({ sequenceImportReport: null }),

  async resolvePendingReplace(choice) {
    const kind = get().pendingReplace;
    if (!kind) return false;
    if (choice === "cancel") {
      set({ pendingReplace: null });
      return false;
    }
    if (choice === "save" && !(await get().save())) return false;
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

  async save() {
    commitFocusedField();
    if (!get().snapshot?.path) return get().saveAs();
    return get().run((b) => b.saveShow());
  },

  async saveAs() {
    commitFocusedField();
    const backend = get().backend;
    const snapshot = get().snapshot;
    if (!backend || !snapshot) return false;
    const suggested = snapshot.path ? fileName(snapshot.path) : `${snapshot.show.name}.pixelflow.json`;
    const path = await backend.pickSavePath(suggested);
    if (!path) return false;
    return get().run((b) => b.saveShowAs(path));
  },
};
});
