import { Search, Trash2 } from "lucide-react";
import { type KeyboardEvent, type MouseEvent, memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Prop } from "../../api/types";
import { thousands } from "../../lib/format";
import { removeEdits, updateEdits } from "../../lib/layoutEdits";
import { PROP_SORTS, type PropSort, listedProps, rangeSelect, wiringStatuses } from "../../lib/propList";
import type { WiringStatus } from "../../lib/wiringMath";
import { shapeLabel } from "../../lib/shows";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { confirmAction } from "../../state/confirm";
import { useSequencer } from "../../state/sequencer";
import { toastWithUndo } from "../../state/undoToast";
import { deleteUseWarning } from "../../lib/sequenceUse";
import { Input, Select } from "../ui";
import { useVirtualRows } from "./useVirtualRows";

const ROW_PX = 30;
const SORT_KEY = "pixelflow.propsSort";

function storedSort(): PropSort {
  try {
    const saved = localStorage.getItem(SORT_KEY);
    return PROP_SORTS.some((s) => s.sort === saved) ? (saved as PropSort) : "layout";
  } catch {
    return "layout";
  }
}

const rowId = (id: string) => `props-list-${id}`;

/** Deletes a prop, with a toast that can take it back. */
export async function deleteProps(ids: string[], names: string[]) {
  // Rows in the open sequence that light these props would be left showing nothing: ask first.
  const subject = names.length === 1 ? names[0] : `These ${names.length} props`;
  const warning = deleteUseWarning(useSequencer.getState().doc, subject, { props: ids }, names.length > 1);
  const title = names.length === 1 ? `Delete ${names[0]}?` : `Delete ${names.length} props?`;
  if (warning && !(await confirmAction({ title, message: warning, confirm: "Delete anyway" }))) return;
  const revision = await useApp.getState().edit(removeEdits(ids));
  if (revision === null) return;
  useLayoutEditor.getState().select(useLayoutEditor.getState().selected.filter((id) => !ids.includes(id)));
  toastWithUndo(names.length === 1 ? `Deleted ${names[0]}` : `Deleted ${names.length} props`, revision);
}

/** Types over a prop's name in the list: Enter or leaving saves, Escape doesn't. */
function RenameField({ prop, onDone }: { prop: Prop; onDone: () => void }) {
  const apply = useApp((s) => s.apply);
  const [name, setName] = useState(prop.name);
  const commit = () => {
    const trimmed = name.trim();
    if (trimmed && trimmed !== prop.name) void apply(updateEdits(prop.id, (p) => ({ ...p, name: trimmed })));
    onDone();
  };
  return (
    <input
      autoFocus
      aria-label={`Name of ${prop.name}`}
      value={name}
      onChange={(e) => setName(e.target.value)}
      onBlur={commit}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") onDone();
      }}
      className="min-w-0 flex-1 rounded border border-accent-500 bg-white px-1 text-sm dark:bg-neutral-950"
    />
  );
}

interface RowProps {
  prop: Prop;
  top: number;
  pixels: number;
  wiring: WiringStatus;
  selected: boolean;
  active: boolean;
  renaming: boolean;
  onPick: (id: string, e: MouseEvent) => void;
  onRename: (id: string | null) => void;
}

/** How each wiring state looks and reads in the list. */
const WIRING: Record<WiringStatus, { dot: string; label: string }> = {
  wired: { dot: "bg-emerald-500/70", label: "Wired" },
  partial: { dot: "border-2 border-amber-500 bg-transparent", label: "Partly wired" },
  unwired: { dot: "bg-amber-500", label: "Not wired yet" },
  twice: { dot: "bg-red-500", label: "Wired twice" },
};

const Row = memo(function Row({ prop, top, pixels, wiring, selected, active, renaming, onPick, onRename }: RowProps) {
  const look = WIRING[wiring];
  return (
    <div
      role="option"
      id={rowId(prop.id)}
      aria-selected={selected}
      title={`${prop.name}: ${shapeLabel(prop.shape)}, ${thousands(pixels)} pixels${wiring === "wired" ? "" : `, ${look.label.toLowerCase()}`}. Double-click to rename.`}
      onClick={(e) => onPick(prop.id, e)}
      onDoubleClick={() => onRename(prop.id)}
      style={{ top, height: ROW_PX }}
      className={`group absolute right-0 left-0 flex cursor-default items-center gap-2 px-2 text-sm select-none ${
        selected ? "bg-accent-100 text-accent-900 dark:bg-accent-600/25 dark:text-accent-100" : "hover:bg-neutral-100 dark:hover:bg-neutral-800/70"
      } ${active ? "outline-2 -outline-offset-2 outline-accent-500" : ""}`}
    >
      <span
        aria-hidden
        className={`h-2 w-2 shrink-0 rounded-full ${look.dot}`}
        title={look.label}
      />
      {renaming ? <RenameField prop={prop} onDone={() => onRename(null)} /> : <span className="min-w-0 flex-1 truncate">{prop.name}</span>}
      <span className="hidden shrink-0 text-xs text-neutral-500 @min-[15rem]:inline">{shapeLabel(prop.shape)}</span>
      <span className="w-12 shrink-0 text-right text-xs text-neutral-500 tabular-nums">{thousands(pixels)}</span>
      <span className="sr-only">{wiring === "wired" ? "" : `, ${look.label.toLowerCase()}`}</span>
      <button
        type="button"
        tabIndex={-1}
        aria-label={`Delete ${prop.name}`}
        title={`Delete ${prop.name} (Undo brings it back)`}
        onClick={(e) => {
          e.stopPropagation();
          void deleteProps([prop.id], [prop.name]);
        }}
        className="shrink-0 rounded p-0.5 text-neutral-400 opacity-0 group-hover:opacity-100 hover:text-red-600 focus:opacity-100 group-aria-selected:opacity-100 dark:hover:text-red-400"
      >
        <Trash2 size={13} />
      </button>
    </div>
  );
});

/**
 * Every prop, searchable and sortable, with pixel counts and wiring. Picking props here picks them
 * on the canvas and the other way round (⌘/Ctrl-click adds one, Shift-click a run). Only the rows
 * in view are drawn, so shows with thousands of props stay quick.
 */
export function PropsList() {
  const show = useApp((s) => s.snapshot!.show);
  const channelMap = useApp((s) => s.snapshot!.channelMap);
  const selected = useLayoutEditor((s) => s.selected);
  const [query, setQuery] = useState("");
  const [sort, setSortState] = useState<PropSort>(storedSort);
  const [unwiredOnly, setUnwiredOnly] = useState(false);
  const [active, setActive] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const anchor = useRef<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  const pixels = useMemo(() => new Map(channelMap.props.map((p) => [p.prop, p.nodes])), [channelMap]);
  const wiring = useMemo(() => wiringStatuses(show, pixels), [show, pixels]);
  const listed = useMemo(() => listedProps(show.props, { sort, query, unwiredOnly }, pixels, wiring), [show.props, sort, query, unwiredOnly, pixels, wiring]);
  const order = useMemo(() => listed.map((p) => p.id), [listed]);
  const picked = useMemo(() => new Set(selected), [selected]);
  const { first, last, total, reveal } = useVirtualRows(scroller, listed.length, ROW_PX);

  // Picked on the canvas: bring the last one picked into view here.
  const lastPicked = selected[selected.length - 1];
  useEffect(() => {
    if (lastPicked) reveal(order.indexOf(lastPicked));
  }, [lastPicked]); // eslint-disable-line react-hooks/exhaustive-deps

  const setSort = (next: PropSort) => {
    setSortState(next);
    try {
      localStorage.setItem(SORT_KEY, next);
    } catch {
      // Storage unavailable: the order still applies until the screen closes.
    }
  };

  const pick = (id: string, e: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }) => {
    const editor = useLayoutEditor.getState();
    if (e.shiftKey && anchor.current) {
      editor.select(rangeSelect(order, anchor.current, id));
    } else if (e.metaKey || e.ctrlKey) {
      editor.toggle(id);
      anchor.current = id;
    } else {
      editor.select([id]);
      anchor.current = id;
    }
    setActive(id);
  };

  // The rows keep one handler for good, so only the rows that change are drawn again.
  const pickNow = useRef(pick);
  pickNow.current = pick;
  const onPick = useCallback((id: string, e: MouseEvent) => pickNow.current(id, e), []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (order.length === 0) return;
    const at = active ? order.indexOf(active) : -1;
    const move = (to: number) => {
      const index = Math.max(0, Math.min(order.length - 1, to));
      const id = order[index];
      if (e.shiftKey && anchor.current) useLayoutEditor.getState().select(rangeSelect(order, anchor.current, id));
      else {
        useLayoutEditor.getState().select([id]);
        anchor.current = id;
      }
      setActive(id);
      reveal(index);
    };
    const page = Math.max(1, Math.floor((scroller.current?.clientHeight || 600) / ROW_PX) - 1);
    const keys: Record<string, () => void> = {
      ArrowDown: () => move(at + 1),
      ArrowUp: () => move(at < 0 ? 0 : at - 1),
      Home: () => move(0),
      End: () => move(order.length - 1),
      PageDown: () => move(at + page),
      PageUp: () => move(at - page),
      " ": () => {
        if (active) pick(active, { shiftKey: false, metaKey: true, ctrlKey: false });
      },
      Enter: () => active && setRenaming(active),
      F2: () => active && setRenaming(active),
    };
    const run = keys[e.key];
    if (run && !e.metaKey && !e.ctrlKey && !e.altKey) {
      // Up and down move through the list here (left and right still nudge the picked props).
      e.preventDefault();
      run();
    }
  };

  const filtered = query.trim() !== "" || unwiredOnly;
  const activeShown = active && order.indexOf(active) >= first && order.indexOf(active) < last ? rowId(active) : undefined;
  return (
    <div className="@container flex min-h-0 flex-1 flex-col">
      <div className="flex flex-col gap-1.5 border-b border-neutral-200 p-2 dark:border-neutral-800">
        <label className="relative block">
          <span className="sr-only">Find props</span>
          <Search size={14} aria-hidden className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-neutral-400" />
          <Input type="search" value={query} placeholder="Find props" onChange={(e) => setQuery(e.target.value)} className="w-full pl-7" />
        </label>
        <div className="flex items-center gap-2">
          <Select aria-label="Sort props" value={sort} onChange={(e) => setSort(e.target.value as PropSort)} className="min-w-0 flex-1 py-1 text-xs">
            {PROP_SORTS.map((s) => (
              <option key={s.sort} value={s.sort}>
                {s.label}
              </option>
            ))}
          </Select>
          <label className="flex shrink-0 items-center gap-1 text-xs text-neutral-600 dark:text-neutral-400" title="Only the props not wired to a controller yet, or only partly wired">
            <input type="checkbox" checked={unwiredOnly} onChange={(e) => setUnwiredOnly(e.target.checked)} className="accent-accent-500" />
            Not wired (or partly)
          </label>
        </div>
        <p className="text-xs text-neutral-500" aria-live="polite">
          {filtered ? `${thousands(listed.length)} of ${thousands(show.props.length)} props` : `${thousands(show.props.length)} ${show.props.length === 1 ? "prop" : "props"}`}
          {selected.length > 0 && ` · ${thousands(selected.length)} selected`}
        </p>
      </div>
      {show.props.length === 0 ? (
        <div className="p-4 text-center text-sm text-neutral-500">
          <p className="font-medium text-neutral-700 dark:text-neutral-300">No props yet</p>
          <p className="mt-1 text-xs">Use Add prop, or pick a tool on the toolbar and drag on the canvas.</p>
        </div>
      ) : listed.length === 0 ? (
        <p className="p-4 text-center text-sm text-neutral-500">No props match.</p>
      ) : (
        <div
          ref={scroller}
          role="listbox"
          aria-label="Props"
          aria-multiselectable
          aria-activedescendant={activeShown}
          tabIndex={0}
          onKeyDown={onKeyDown}
          onFocus={() => {
            if (!active) setActive(selected.find((id) => order.includes(id)) ?? order[0]);
          }}
          className="relative min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-2 focus-visible:ring-accent-500/50"
        >
          <div style={{ height: total }} className="relative">
            {listed.slice(first, last).map((prop, i) => (
              <Row
                key={prop.id}
                prop={prop}
                top={(first + i) * ROW_PX}
                pixels={pixels.get(prop.id) ?? 0}
                wiring={wiring.get(prop.id) ?? "unwired"}
                selected={picked.has(prop.id)}
                active={active === prop.id}
                renaming={renaming === prop.id}
                onPick={onPick}
                onRename={setRenaming}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
