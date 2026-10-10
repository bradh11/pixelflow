// How the layout is shown: in 2D or 3D, how much lit pixels glow in every preview, and the 3D
// view's settings. None of it is part of the show: it's how this viewer likes to look at it,
// remembered on this computer (the 3D camera and the photo's depth for each show file).

import { create } from "zustand";
import { type Box3, type Orbit, type Preset, parseOrbit } from "../lib/layout3d";

export type LayoutMode = "2d" | "3d";

/** Something for the open 3D view to do with its camera. */
export type CameraAction = { kind: "fit" } | { kind: "preset"; preset: Preset } | { kind: "zoom"; factor: number };

interface View3dState {
  /** The Layout screen's view. */
  mode: LayoutMode;
  /** The Play screen's preview. */
  playMode: LayoutMode;
  /** The Sequence screen's preview. */
  sequenceMode: LayoutMode;
  /**
   * How much lit pixels glow in every preview (2D and 3D; Layout, Sequence and Play), from 0
   * (none: crisp dots, as bare bulbs look) to 1 (as lights behind diffusers look).
   */
  glow: number;
  /** The ground and its grid. */
  ground: boolean;
  /** The latest camera request; `seq` tells a repeat from the one before. */
  command: { seq: number; action: CameraAction } | null;
  /** The show whose view settings these are (see `showViewKey`), and its photo's depth. */
  showKey: string | null;
  photoDepth: number;
  /** The show's settings last moved to a new key because it was saved under a new name. */
  carried: { from: string; to: string } | null;
  /** The house model the 3D view has loaded, and its box as its file has it. */
  loadedModel: { path: string; natural: Box3 } | null;

  setMode(mode: LayoutMode): void;
  setPlayMode(mode: LayoutMode): void;
  setSequenceMode(mode: LayoutMode): void;
  setGlow(level: number): void;
  setGround(on: boolean): void;
  camera(action: CameraAction): void;
  /** Reads the remembered settings for the show `key`. */
  openShow(key: string): void;
  /** The same show is now kept under `to` (saved for the first time, or with Save As): its settings go with it. */
  carryShow(from: string, to: string): void;
  setPhotoDepth(depth: number): void;
}

const MODE_KEY = "pixelflow.layoutMode";
const PLAY_MODE_KEY = "pixelflow.playMode";
const SEQUENCE_MODE_KEY = "pixelflow.sequenceMode";
const OPTIONS_KEY = "pixelflow.view3dOptions";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage unavailable: the setting still applies for this session.
  }
}

const glowLevel = (level: number) => (Number.isFinite(level) ? Math.min(1, Math.max(0, level)) : 0);

/**
 * The options remembered on this computer. Settings saved without a glow level may hold the 3D
 * view's Glow button, on or off, instead (`bloom`): on reads as half way, about how that looked.
 */
export function loadViewOptions(): { glow: number; ground: boolean } {
  try {
    const o = JSON.parse(read(OPTIONS_KEY) ?? "{}") as Record<string, unknown>;
    const glow = typeof o.glow === "number" ? glowLevel(o.glow) : o.bloom === true ? 0.5 : 0;
    return { glow, ground: o.ground !== false };
  } catch {
    return { glow: 0, ground: true };
  }
}

/** What's remembered about a show's 3D view on this computer. */
export interface ShowView3d {
  orbit: Orbit | null;
  /** How far behind the props (toward the house, -z) the photo stands. */
  photoDepth: number;
}

export const DEFAULT_PHOTO_DEPTH = 0.05;

const showKey = (key: string) => `pixelflow.view3d:${key}`;

export function loadShowView(key: string): ShowView3d {
  try {
    const saved = JSON.parse(read(showKey(key)) ?? "{}") as Record<string, unknown>;
    const depth = saved.photoDepth;
    return {
      orbit: parseOrbit(saved.orbit),
      photoDepth: typeof depth === "number" && Number.isFinite(depth) ? depth : DEFAULT_PHOTO_DEPTH,
    };
  } catch {
    return { orbit: null, photoDepth: DEFAULT_PHOTO_DEPTH };
  }
}

export function saveShowView(key: string, view: Partial<ShowView3d>) {
  write(showKey(key), JSON.stringify({ ...loadShowView(key), ...view }));
}

/** Copies what's remembered under `from` to `to`, unless `to` has settings of its own. */
function copyShowView(from: string, to: string) {
  const saved = read(showKey(from));
  if (saved !== null && read(showKey(to)) === null) write(showKey(to), saved);
}

export const useView3d = create<View3dState>((set, get) => ({
  mode: read(MODE_KEY) === "3d" ? "3d" : "2d",
  playMode: read(PLAY_MODE_KEY) === "3d" ? "3d" : "2d",
  sequenceMode: read(SEQUENCE_MODE_KEY) === "3d" ? "3d" : "2d",
  ...loadViewOptions(),
  command: null,
  showKey: null,
  photoDepth: DEFAULT_PHOTO_DEPTH,
  carried: null,
  loadedModel: null,

  setMode(mode) {
    write(MODE_KEY, mode);
    set({ mode });
  },
  setPlayMode(playMode) {
    write(PLAY_MODE_KEY, playMode);
    set({ playMode });
  },
  setSequenceMode(sequenceMode) {
    write(SEQUENCE_MODE_KEY, sequenceMode);
    set({ sequenceMode });
  },
  setGlow(level) {
    const glow = glowLevel(level);
    write(OPTIONS_KEY, JSON.stringify({ glow, ground: get().ground }));
    set({ glow });
  },
  setGround(ground) {
    write(OPTIONS_KEY, JSON.stringify({ glow: get().glow, ground }));
    set({ ground });
  },
  camera: (action) => set({ command: { seq: (get().command?.seq ?? 0) + 1, action } }),
  openShow(key) {
    if (get().showKey !== key) set({ showKey: key, photoDepth: loadShowView(key).photoDepth });
  },
  carryShow(from, to) {
    if (from === to) return;
    copyShowView(from, to);
    // The depth in use is the show's: keep it, and keep it under the new key.
    if (get().showKey === from) {
      saveShowView(to, { photoDepth: get().photoDepth });
      set({ showKey: to });
    }
    set({ carried: { from, to } });
  },
  setPhotoDepth(photoDepth) {
    const key = get().showKey;
    if (key) saveShowView(key, { photoDepth });
    set({ photoDepth });
  },
}));

/** The key a show's view settings are remembered under: its file, or its name until it's saved. */
export function showViewKey(path: string | null, name: string): string {
  return path ?? `unsaved:${name}`;
}
