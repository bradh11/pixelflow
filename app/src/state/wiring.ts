import { create } from "zustand";
import type { WireSession } from "../lib/wireSession";
import type { PlaceRef, PortRef, SlotRef } from "../lib/wiringMath";

/** What's being dragged: a prop from the props list, or a slot (a table row) from a port. */
export type DragItem = { kind: "prop"; prop: string } | { kind: "slot"; from: SlotRef; prop: string };

/** Where a drag would land: a place on a port, or the props list (unwire). */
export type DropTarget = ({ kind: "port" } & PlaceRef) | { kind: "props" };

/** A port by controller and place in its list, for remembering which ports are folded. */
export const portKey = (controller: string, at: number) => `${controller}:${at}`;

interface WiringState {
  /** The slot whose settings are open, by identity (its prop and pixels); the Wiring screen keeps
   * `index` up to date and closes the settings when the slot is gone. */
  selected: SlotRef | null;
  /** The port the pointer is over, shown in the preview. */
  hovered: PortRef | null;
  /** The prop whose table row the pointer is over: lit, with its stretch of wire, in the preview. */
  hoveredProp: string | null;
  /** Controllers folded shut (by id). */
  collapsed: string[];
  /** Ports folded to their one-line summary (by `portKey`). */
  folded: string[];
  query: string;
  /** `owner` is the drag source that started it, so it can let go if it disappears mid-drag. */
  drag: { item: DragItem; x: number; y: number; over: DropTarget | null; owner: object } | null;
  /** A row to focus once it's drawn (after a keyboard move, or closing its settings); found by
   * identity. With `index` past the port's end and no such slot, the port's Add button. */
  focus: SlotRef | null;
  /** The port whose "+ Add" picker is open. */
  adding: PortRef | null;
  /** Wiring a port by clicking props on the layout. */
  session: WireSession | null;
  select(ref: SlotRef | null): void;
  hover(ref: PortRef | null): void;
  toggleCollapsed(id: string): void;
  toggleFolded(key: string): void;
  setQuery(query: string): void;
}

const toggled = (list: string[], id: string) => (list.includes(id) ? list.filter((c) => c !== id) : [...list, id]);

export const useWiring = create<WiringState>((set, get) => ({
  selected: null,
  hovered: null,
  hoveredProp: null,
  collapsed: [],
  folded: [],
  query: "",
  drag: null,
  focus: null,
  adding: null,
  session: null,
  select: (selected) => set({ selected }),
  hover: (hovered) => set({ hovered }),
  toggleCollapsed: (id) => set({ collapsed: toggled(get().collapsed, id) }),
  toggleFolded: (key) => set({ folded: toggled(get().folded, key) }),
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
