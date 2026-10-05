import { create } from "zustand";
import type { PlaceRef, PortRef, SlotRef } from "../lib/wiringMath";

/** What's being dragged: a prop from the props list, or a chip (slot) from a port. */
export type DragItem = { kind: "prop"; prop: string } | { kind: "slot"; from: SlotRef; prop: string };

/** Where a drag would land: a place on a port, or the props list (unwire). */
export type DropTarget = ({ kind: "port" } & PlaceRef) | { kind: "props" };

interface WiringState {
  /** The chip whose settings are open, by identity (its prop and pixels); the Wiring screen keeps
   * `index` up to date and closes the settings when the slot is gone. */
  selected: SlotRef | null;
  /** The port the pointer is over, shown in the preview. */
  hovered: PortRef | null;
  /** Controllers folded shut (by id). */
  collapsed: string[];
  query: string;
  /** `owner` is the drag source that started it, so it can let go if it disappears mid-drag. */
  drag: { item: DragItem; x: number; y: number; over: DropTarget | null; owner: object } | null;
  /** A chip to focus once it's drawn (after a keyboard move, or closing its settings); found by
   * identity. With `index` past the port's end and no such slot, the port's Add button. */
  focus: SlotRef | null;
  /** The port whose "+ Add" picker is open. */
  adding: PortRef | null;
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
  adding: null,
  select: (selected) => set({ selected }),
  hover: (hovered) => set({ hovered }),
  toggleCollapsed: (id) => {
    const collapsed = get().collapsed;
    set({ collapsed: collapsed.includes(id) ? collapsed.filter((c) => c !== id) : [...collapsed, id] });
  },
  setQuery: (query) => set({ query }),
}));

/** The same port: two refs to a port that shares its number with another match only when both
 * say which (`at`) and agree, or when either doesn't say. */
export const samePort = (a: PortRef | null, b: PortRef | null) =>
  !!a && !!b && a.controller === b.controller && a.port === b.port && (a.at === undefined || b.at === undefined || a.at === b.at);

/** The same slot: same port and place, carrying the same prop. */
export const sameSlot = (a: SlotRef | null, b: SlotRef | null) => !!a && !!b && samePort(a, b) && a.index === b.index && a.prop === b.prop;

/** The same drop target (so a pointer move that lands where it was changes nothing). */
export const sameTarget = (a: DropTarget | null, b: DropTarget | null) =>
  a === b ||
  (!!a && !!b && (a.kind === "props" ? b.kind === "props" : b.kind === "port" && samePort(a, b) && a.at === b.at && a.index === b.index));
