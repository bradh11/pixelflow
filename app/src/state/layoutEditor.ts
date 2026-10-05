import { create } from "zustand";
import type { Background } from "../api/types";
import type { View } from "../lib/layoutMath";
import type { PropKind } from "../lib/shows";

/** Select and move props, move the view, or draw a new prop of a kind. */
export type Tool = "select" | "pan" | PropKind;

interface LayoutEditorState {
  tool: Tool;
  /** Selected prop ids, in the order they were picked. */
  selected: string[];
  /** Snap moves and drawing to the grid. */
  snap: boolean;
  /** Grid spacing in layout units. */
  grid: number;
  /** When on, dragging moves and resizes the background photo instead of props. */
  editPhoto: boolean;
  /** The photo as it is being changed (dragged, or its strength slid), before it's saved. */
  photoDraft: Background | null;
  /** What the canvas shows; null fits everything in on the next draw. */
  view: View | null;

  setTool(tool: Tool): void;
  select(ids: string[]): void;
  /** Adds the prop to the selection, or takes it out if it's already there. */
  toggle(id: string): void;
  clear(): void;
  setSnap(snap: boolean): void;
  setEditPhoto(on: boolean): void;
  setPhotoDraft(draft: Background | null): void;
  setView(view: View | null): void;
}

export const useLayoutEditor = create<LayoutEditorState>((set, get) => ({
  tool: "select",
  selected: [],
  snap: false,
  grid: 0.5,
  editPhoto: false,
  photoDraft: null,
  view: null,

  setTool: (tool) => set({ tool, editPhoto: false }),
  select: (ids) => set({ selected: [...new Set(ids)] }),
  toggle: (id) => {
    const selected = get().selected;
    set({ selected: selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id] });
  },
  clear: () => set({ selected: [] }),
  setSnap: (snap) => set({ snap }),
  setEditPhoto: (editPhoto) => set({ editPhoto, tool: "select", selected: editPhoto ? [] : get().selected }),
  setPhotoDraft: (photoDraft) => set({ photoDraft }),
  setView: (view) => set({ view }),
}));
