import { GripVertical, Search } from "lucide-react";
import type { Prop } from "../../api/types";
import { plural, thousands } from "../../lib/format";
import type { PropWiring, WiringStatus } from "../../lib/wiringMath";
import { useWiring } from "../../state/wiring";
import { GoToScreen } from "../GoToScreen";
import { Input } from "../ui";
import { useDragSource } from "./useWiringDrag";

/** Unwired first, then problems, then partly wired, then wired. */
const ORDER: Record<WiringStatus, number> = { unwired: 0, twice: 1, partial: 2, wired: 3 };

function statusText(w: PropWiring): string {
  const where = w.places.map((p) => `${p.controllerName} · Port ${p.port}`);
  switch (w.status) {
    case "unwired":
      return "Not wired";
    case "twice":
      return `Wired twice: ${where.join(", ")}`;
    case "partial":
      return `${thousands(w.wiredPixels)} of ${thousands(w.nodes)} pixels wired: ${where.join(", ")}`;
    case "wired":
      return where.join(", ");
  }
}

const STATUS_CLASS: Record<WiringStatus, string> = {
  unwired: "text-amber-700 dark:text-amber-400",
  twice: "text-red-600 dark:text-red-400",
  partial: "text-amber-700 dark:text-amber-400",
  wired: "text-neutral-500 dark:text-neutral-400",
};

function PropItem({ prop, wiring }: { prop: Prop; wiring: PropWiring }) {
  const source = useDragSource({ kind: "prop", prop: prop.id });
  const dragging = useWiring((s) => s.drag?.item.kind === "prop" && s.drag.item.prop === prop.id);
  const status = statusText(wiring);
  return (
    <li>
      <button
        type="button"
        aria-label={`${prop.name}, ${thousands(wiring.nodes)} pixels. ${status}`}
        title="Drag onto a port to wire it"
        className={`flex w-full cursor-grab touch-none items-start gap-1.5 rounded-md px-1.5 py-1.5 text-left select-none hover:bg-neutral-100 dark:hover:bg-neutral-800 ${
          dragging ? "bg-accent-50 dark:bg-accent-600/15" : ""
        }`}
        onPointerDown={source.onPointerDown}
        onPointerMove={source.onPointerMove}
        onPointerUp={source.onPointerUp}
        onPointerCancel={source.onPointerCancel}
        onLostPointerCapture={source.onLostPointerCapture}
        onClick={() => {
          if (source.endedDrag()) return;
          // A wired prop: open its (first) slot's settings.
          const place = wiring.places[0];
          if (place) useWiring.getState().select({ controller: place.controller, port: place.port, at: place.at, index: place.index, prop: prop.id, segment: place.slot.segment });
        }}
      >
        <GripVertical size={14} className="mt-0.5 shrink-0 text-neutral-400" aria-hidden />
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span className="truncate text-sm font-medium">{prop.name}</span>
            <span className="ml-auto shrink-0 text-xs text-neutral-500 tabular-nums">{thousands(wiring.nodes)} px</span>
          </span>
          <span className={`block truncate text-xs ${STATUS_CLASS[wiring.status]}`}>{status}</span>
        </span>
      </button>
    </li>
  );
}

/** Every prop with where it's wired; unwired props first. Drag one onto a port to wire it, or
 * drop a chip here to unwire it. */
export function PropsPanel({ props, wiring }: { props: Prop[]; wiring: Map<string, PropWiring> }) {
  const query = useWiring((s) => s.query);
  const setQuery = useWiring((s) => s.setQuery);
  const over = useWiring((s) => s.drag?.item.kind === "slot" && s.drag.over?.kind === "props");
  const unwired = props.filter((p) => wiring.get(p.id)?.status === "unwired").length;
  const needle = query.trim().toLowerCase();
  const shown = props
    .filter((p) => !needle || p.name.toLowerCase().includes(needle))
    .map((p, i) => ({ prop: p, w: wiring.get(p.id)!, i }))
    .filter((x) => x.w)
    .sort((a, b) => ORDER[a.w.status] - ORDER[b.w.status] || a.i - b.i);
  return (
    <aside
      aria-label="Props"
      data-wiring-drop="props"
      className={`flex max-h-[28rem] flex-col rounded-lg border bg-white lg:sticky lg:top-0 lg:max-h-[calc(100vh-11rem)] dark:bg-neutral-900 ${
        over ? "border-accent-500 ring-2 ring-accent-500/40" : "border-neutral-200 dark:border-neutral-800"
      }`}
    >
      <div className="border-b border-neutral-200 p-3 dark:border-neutral-800">
        <h2 className="text-sm font-semibold">Props</h2>
        <p
          className={`mt-0.5 text-xs ${unwired > 0 ? "font-medium text-amber-700 dark:text-amber-400" : "text-neutral-500"}`}
          data-testid="unwired-count"
        >
          {props.length === 0 ? (
            <>
              No props yet. <GoToScreen screen="layout">Add props on Layout</GoToScreen>
            </>
          ) : unwired === 0 ? (
            "Every prop is wired."
          ) : (
            `${plural(unwired, "prop")} ${unwired === 1 ? "isn't" : "aren't"} wired yet`
          )}
        </p>
        <label className="relative mt-2 block">
          <span className="sr-only">Find a prop</span>
          <Search size={14} className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-neutral-400" aria-hidden />
          <Input className="w-full pl-7" placeholder="Find a prop" value={query} onChange={(e) => setQuery(e.target.value)} />
        </label>
      </div>
      <ul className="min-h-0 flex-1 overflow-auto p-1.5">
        {shown.map(({ prop, w }) => (
          <PropItem key={prop.id} prop={prop} wiring={w} />
        ))}
        {needle && shown.length === 0 && <li className="px-2 py-3 text-sm text-neutral-500">No prop matches “{query.trim()}”.</li>}
      </ul>
      <p className="border-t border-neutral-200 px-3 py-2 text-xs text-neutral-500 dark:border-neutral-800">
        {over
          ? "Let go to unwire it."
          : "Drag each prop onto the port it's plugged into, in the order the wire reaches them. Drop a wired prop back here to unwire it."}
      </p>
    </aside>
  );
}
