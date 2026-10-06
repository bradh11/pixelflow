import { create } from "zustand";
import type { Background, Phoneme, Prop } from "../api/types";
import type { Gesture, Size, View } from "../lib/layoutMath";
import type { PropKind } from "../lib/shows";

/**
 * A submodel or face picked in the properties panel: its pixels are drawn bright on the canvas,
 * or, for a face with a `phoneme`, the face shows that mouth shape (with open eyes and outline).
 */
export interface Highlight {
  prop: string;
  region: string;
  phoneme: Phoneme | null;
}

/** Select and move props, move the view, or draw a new prop of a kind. */
export type Tool = "select" | PropKind;

/**
 * A finished gesture on its way to the engine. The canvas keeps drawing it until the engine's
 * pixel positions include it: `revision` is the show revision that holds it, once known.
 */
export interface PendingGesture {
  key: number;
  ids: string[];
  gesture: Gesture;
  revision: number | null;
}

interface LayoutEditorState {
  tool: Tool;
  /** Selected prop ids, in the order they were picked. */
  selected: string[];
  /** Snap moves and drawing to the grid. */
  snap: boolean;
  /** Grid spacing in layout units. */
  grid: number;
  /** Snap to other props' edges, centers, gaps, and sizes while moving, resizing, and drawing. */
  smartGuides: boolean;
  /** When on, dragging moves and resizes the background photo instead of props. */
  editPhoto: boolean;
  /** The photo as it is being changed (dragged, or its strength slid), before it's saved. */
  photoDraft: Background | null;
  /** What the canvas shows; null fits everything in on the next draw. */
  view: View | null;
  /** Gestures sent but not yet in the engine's pixel positions, oldest first. */
  pending: PendingGesture[];
  /** Arrow-key moves while a key is held, sent as one move when it's let go. */
  nudge: { ids: string[]; dx: number; dy: number } | null;
  /**
   * Props copied (⌘C) or cut (⌘X), and how many steps along the next paste lands: cut props
   * paste back where they were, copies a little to the side, and each paste a little further.
   */
  clipboard: { props: Prop[]; nextOffset: number } | null;
  /** The submodel or face shown on the canvas; cleared when its prop is no longer selected. */
  highlight: Highlight | null;
  /** The point picked on the selected poly line (Delete removes it); cleared when the selection changes. */
  polyPoint: { prop: string; index: number } | null;

  setTool(tool: Tool): void;
  select(ids: string[]): void;
  /** Adds the prop to the selection, or takes it out if it's already there. */
  toggle(id: string): void;
  clear(): void;
  setSnap(snap: boolean): void;
  setSmartGuides(on: boolean): void;
  setEditPhoto(on: boolean): void;
  setPhotoDraft(draft: Background | null): void;
  setView(view: View | null): void;
  setHighlight(highlight: Highlight | null): void;
  setPolyPoint(point: { prop: string; index: number } | null): void;
  /** The props and groups list beside the canvas: shown or folded away, which tab, and the group open in it. */
  sidePanel: { open: boolean; tab: "props" | "groups"; group: string | null };
  setSidePanel(change: Partial<LayoutEditorState["sidePanel"]>): void;
}

const SMART_GUIDES_KEY = "pixelflow.smartGuides";
const SIDE_PANEL_KEY = "pixelflow.layoutSidePanel";

/** The props list as last left on this computer: shown unless folded away. */
function storedSidePanelOpen(): boolean {
  try {
    return localStorage.getItem(SIDE_PANEL_KEY) !== "closed";
  } catch {
    return true;
  }
}

/** Smart guides as last set on this computer: on unless turned off. */
function storedSmartGuides(): boolean {
  try {
    return localStorage.getItem(SMART_GUIDES_KEY) !== "false";
  } catch {
    return true;
  }
}

/** The highlight, if its prop is still selected. */
const keep = (highlight: Highlight | null, selected: string[]) => (highlight && selected.includes(highlight.prop) ? highlight : null);

/** The picked poly line point, if its prop is still the only one selected. */
const keepPoint = (point: LayoutEditorState["polyPoint"], selected: string[]) =>
  point && selected.length === 1 && selected[0] === point.prop ? point : null;

export const useLayoutEditor = create<LayoutEditorState>((set, get) => ({
  tool: "select",
  selected: [],
  snap: false,
  grid: 0.5,
  smartGuides: storedSmartGuides(),
  editPhoto: false,
  photoDraft: null,
  view: null,
  pending: [],
  nudge: null,
  clipboard: null,
  highlight: null,
  polyPoint: null,

  setTool: (tool) => set({ tool, editPhoto: false }),
  select: (ids) => {
    const selected = [...new Set(ids)];
    set({ selected, highlight: keep(get().highlight, selected), polyPoint: keepPoint(get().polyPoint, selected) });
  },
  toggle: (id) => {
    const before = get().selected;
    const selected = before.includes(id) ? before.filter((s) => s !== id) : [...before, id];
    set({ selected, highlight: keep(get().highlight, selected), polyPoint: keepPoint(get().polyPoint, selected) });
  },
  clear: () => set({ selected: [], highlight: null, polyPoint: null }),
  setSnap: (snap) => set({ snap }),
  setSmartGuides: (smartGuides) => {
    try {
      localStorage.setItem(SMART_GUIDES_KEY, String(smartGuides));
    } catch {
      // Storage unavailable: the setting still applies for this session.
    }
    set({ smartGuides });
  },
  setEditPhoto: (editPhoto) =>
    set({ editPhoto, tool: "select", selected: editPhoto ? [] : get().selected, highlight: editPhoto ? null : get().highlight }),
  setPhotoDraft: (photoDraft) => set({ photoDraft }),
  setView: (view) => set({ view }),
  setHighlight: (highlight) => set({ highlight }),
  setPolyPoint: (polyPoint) => set({ polyPoint }),
  sidePanel: { open: storedSidePanelOpen(), tab: "props", group: null },
  setSidePanel: (change) => {
    const sidePanel = { ...get().sidePanel, ...change };
    if (change.open !== undefined) {
      try {
        localStorage.setItem(SIDE_PANEL_KEY, sidePanel.open ? "open" : "closed");
      } catch {
        // Storage unavailable: it still applies for this session.
      }
    }
    set({ sidePanel });
  },
}));

/** Measures the 2D layout canvas on screen; set by the canvas while it's there. */
let measureCanvas: (() => Size) | null = null;

/** Lets the canvas on screen be measured; the returned function stops it. */
export function registerCanvas(measure: () => Size): () => void {
  measureCanvas = measure;
  return () => {
    if (measureCanvas === measure) measureCanvas = null;
  };
}

/** The 2D layout canvas's size now, or null when there isn't one on screen. */
export function canvasSize(): Size | null {
  const size = measureCanvas?.();
  return size && size.width > 0 && size.height > 0 ? size : null;
}
