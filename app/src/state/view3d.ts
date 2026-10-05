// How the layout is shown: in 2D or 3D, and the 3D view's settings. The 3D camera and the
// photo's depth are view state, not part of the show: they're remembered on this computer for
// each show file instead.

import { create } from "zustand";
import { type Orbit, type Preset, parseOrbit } from "../lib/layout3d";

export type LayoutMode = "2d" | "3d";

/** Something for the open 3D view to do with its camera. */
export type CameraAction = { kind: "fit" } | { kind: "preset"; preset: Preset } | { kind: "zoom"; factor: number };

interface View3dState {
  /** The Layout screen's view. */
  mode: LayoutMode;
  /** The Play screen's preview. */
  playMode: LayoutMode;
  /** Glow around lit pixels. */
  bloom: boolean;
  /** The ground and its grid. */
  ground: boolean;
  /** The latest camera request; `seq` tells a repeat from the one before. */
  command: { seq: number; action: CameraAction } | null;
  /** The show whose view settings these are (see `showViewKey`), and its photo's depth. */
  showKey: string | null;
  photoDepth: number;

  setMode(mode: LayoutMode): void;
  setPlayMode(mode: LayoutMode): void;
  setBloom(on: boolean): void;
  setGround(on: boolean): void;
  camera(action: CameraAction): void;
  /** Reads the remembered settings for the show `key`. */
  openShow(key: string): void;
  setPhotoDepth(depth: number): void;
}

const MODE_KEY = "pixelflow.layoutMode";
const PLAY_MODE_KEY = "pixelflow.playMode";
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

function storedOptions(): { bloom: boolean; ground: boolean } {
  try {
    const o = JSON.parse(read(OPTIONS_KEY) ?? "{}") as Record<string, unknown>;
    return { bloom: o.bloom !== false, ground: o.ground !== false };
  } catch {
    return { bloom: true, ground: true };
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

export const useView3d = create<View3dState>((set, get) => ({
  mode: read(MODE_KEY) === "3d" ? "3d" : "2d",
  playMode: read(PLAY_MODE_KEY) === "3d" ? "3d" : "2d",
  ...storedOptions(),
  command: null,
  showKey: null,
  photoDepth: DEFAULT_PHOTO_DEPTH,

  setMode(mode) {
    write(MODE_KEY, mode);
    set({ mode });
  },
  setPlayMode(playMode) {
    write(PLAY_MODE_KEY, playMode);
    set({ playMode });
  },
  setBloom(bloom) {
    write(OPTIONS_KEY, JSON.stringify({ bloom, ground: get().ground }));
    set({ bloom });
  },
  setGround(ground) {
    write(OPTIONS_KEY, JSON.stringify({ bloom: get().bloom, ground }));
    set({ ground });
  },
  camera: (action) => set({ command: { seq: (get().command?.seq ?? 0) + 1, action } }),
  openShow(key) {
    if (get().showKey !== key) set({ showKey: key, photoDepth: loadShowView(key).photoDepth });
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
