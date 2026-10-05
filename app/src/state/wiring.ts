import { create } from "zustand";
import type { PortRef, SlotRef } from "../lib/wiringMath";

/** What's being dragged: a prop from the props list, or a chip (slot) from a port. */
export type DragItem = { kind: "prop"; prop: string } | { kind: "slot"; from: SlotRef; prop: string };

/** Where a drag would land: a place on a port, or the props list (unwire). */
export type DropTarget = ({ kind: "port" } & SlotRef) | { kind: "props" };

interface WiringState {
  /** The chip whose settings are open. */
  selected: SlotRef | null;
  /** The port the pointer is over, shown in the preview. */
  hovered: PortRef | null;
  /** Controllers folded shut (by id). */
  collapsed: string[];
  query: string;
  drag: { item: DragItem; x: number; y: number; over: DropTarget | null } | null;
  /** A chip to focus once it's drawn (after a keyboard move). */
  focus: SlotRef | null;
  select(ref: SlotRef | null): void;
  hover(ref: PortRef | null): void;
  toggleCollapsed(id: string): void;
  setQuery(query: string): void;
}

export const useWiring = create<WiringState>((set, get) => ({
  selected: null,
  hovered: null,
  collapsed: [],
  query: "",
  drag: null,
  focus: null,
  select: (selected) => set({ selected }),
  hover: (hovered) => set({ hovered }),
  toggleCollapsed: (id) => {
    const collapsed = get().collapsed;
    set({ collapsed: collapsed.includes(id) ? collapsed.filter((c) => c !== id) : [...collapsed, id] });
  },
  setQuery: (query) => set({ query }),
}));

export const sameSlot = (a: SlotRef | null, b: SlotRef | null) => !!a && !!b && a.controller === b.controller && a.port === b.port && a.index === b.index;
export const samePort = (a: PortRef | null, b: PortRef | null) => !!a && !!b && a.controller === b.controller && a.port === b.port;
