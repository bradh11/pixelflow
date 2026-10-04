import { create } from "zustand";
import { type Backend, errorMessage } from "../api/backend";
import type { Edit, ShowSnapshot } from "../api/types";
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

  connect(backend: Backend): Promise<void>;
  setScreen(screen: Screen): void;
  setTheme(theme: Theme): void;
  setPaletteOpen(open: boolean): void;
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
}

export const useApp = create<AppState>((set, get) => ({
  backend: null,
  snapshot: null,
  started: false,
  screen: "layout",
  theme: storedTheme(),
  paletteOpen: false,
  error: null,
  busy: false,

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
  dismissError: () => set({ error: null }),

  async run(call) {
    const backend = get().backend;
    if (!backend) return false;
    set({ busy: true });
    try {
      const snapshot = await call(backend);
      set({ snapshot, error: null });
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
    const ok = await get().run((b) => b.newShow("Untitled Show"));
    if (ok) set({ started: true, screen: "layout" });
    return ok;
  },

  async openShow() {
    const backend = get().backend;
    if (!backend) return false;
    const path = await backend.pickOpenPath();
    if (!path) return false;
    const ok = await get().run((b) => b.openShow(path));
    if (ok) set({ started: true, screen: "layout" });
    return ok;
  },

  async save() {
    if (!get().snapshot?.path) return get().saveAs();
    return get().run((b) => b.saveShow());
  },

  async saveAs() {
    const backend = get().backend;
    const snapshot = get().snapshot;
    if (!backend || !snapshot) return false;
    const suggested = snapshot.path ? fileName(snapshot.path) : `${snapshot.show.name}.pixelflow.json`;
    const path = await backend.pickSavePath(suggested);
    if (!path) return false;
    return get().run((b) => b.saveShowAs(path));
  },
}));
