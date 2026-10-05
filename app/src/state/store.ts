import { create } from "zustand";
import { type Backend, errorMessage } from "../api/backend";
import type { Discovery, Edit, ShowSnapshot } from "../api/types";
import { fileName } from "../lib/format";

export type Screen = "layout" | "wiring" | "devices" | "test" | "history";
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
  pendingReplace: "new" | "open" | null;
  /** Test screen target selection; kept here so it survives leaving the screen. */
  testTarget: string;
  /** The last device scan's results (kept while moving between screens). */
  discovery: Discovery | null;
  scanning: boolean;

  connect(backend: Backend): Promise<void>;
  setScreen(screen: Screen): void;
  setTheme(theme: Theme): void;
  setPaletteOpen(open: boolean): void;
  setTestTarget(value: string): void;
  resolvePendingReplace(choice: "save" | "discard" | "cancel"): Promise<boolean>;
  dismissError(): void;
  /** Runs a backend call that returns a new snapshot; errors become a message. Returns success. */
  run(call: (backend: Backend) => Promise<ShowSnapshot>): Promise<boolean>;
  apply(edits: Edit[]): Promise<boolean>;
  undo(): Promise<boolean>;
  redo(): Promise<boolean>;
  newShow(): Promise<boolean>;
  openShow(): Promise<boolean>;
  save(): Promise<boolean>;
  saveAs(): Promise<boolean>;
  /** Looks for controllers; `hosts` adds typed addresses. Results merge into `discovery`. */
  scan(hosts?: string[]): Promise<boolean>;
  /** From the welcome screen: start a new show, open Devices, and scan. */
  discoverFromWelcome(): Promise<void>;
}

export const useApp = create<AppState>((set, get) => {
  /** Replaces the current show without checking for unsaved changes. */
  async function replaceShow(kind: "new" | "open"): Promise<boolean> {
    const backend = get().backend;
    if (!backend) return false;
    let ok: boolean;
    if (kind === "new") {
      ok = await get().run((b) => b.newShow("Untitled Show"));
    } else {
      const path = await backend.pickOpenPath();
      if (!path) return false;
      ok = await get().run((b) => b.openShow(path));
    }
    if (ok) set({ started: true, screen: "layout" });
    return ok;
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
  testTarget: "show",
  discovery: null,
  scanning: false,

  async connect(backend) {
    set({ backend });
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
  dismissError: () => set({ error: null }),

  async run(call) {
    const backend = get().backend;
    if (!backend) return false;
    set({ busy: true });
    try {
      const snapshot = await call(backend);
      const current = get().snapshot;
      // Calls can resolve out of order; engine revisions only increase, so never go backwards.
      if (!current || snapshot.revision >= current.revision) set({ snapshot });
      set({ error: null });
      return true;
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    } finally {
      set({ busy: false });
    }
  },

  apply: (edits) => get().run((b) => b.applyEdits(edits)),
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

  async scan(hosts = []) {
    const backend = get().backend;
    if (!backend) return false;
    set({ scanning: true });
    try {
      const found = await backend.discoverDevices(hosts);
      const previous = hosts.length ? get().discovery : null;
      const devices = [...(previous?.devices ?? []).filter((d) => !found.devices.some((f) => f.address === d.address)), ...found.devices];
      const silent = found.silent.filter((s) => !devices.some((d) => d.address === s.address));
      set({ discovery: { devices, silent }, error: null });
      return true;
    } catch (e) {
      set({ error: errorMessage(e) });
      return false;
    } finally {
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
