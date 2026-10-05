import { Search } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import type { Controller, Port } from "../../api/types";
import { plural, thousands } from "../../lib/format";
import { type PortRef, firstGap, slotRange, wirePropEdits, wireRemainingEdits } from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { samePort, useWiring } from "../../state/wiring";
import { Button } from "../ui";
import type { WiringData } from "./ControllerCard";

/** The end of the port, whatever its length when the edit's turn comes. */
const END = Number.MAX_SAFE_INTEGER;

interface Choice {
  id: string;
  label: string;
  detail?: string;
}

/**
 * The non-drag way to put props on a port: a searchable list of unwired props first, then the
 * rest of partly wired props, then props wired on one other port (moved here). There is one at a
 * time, built only while it's open, so a show with hundreds of props and ports stays light.
 */
export function AddPicker({ controller, port, portRef, data, onDone }: { controller: Controller; port: Port; portRef: PortRef; data: WiringData; onDone: () => void }) {
  const apply = useApp((s) => s.apply);
  const [query, setQuery] = useState("");
  const [confirmRest, setConfirmRest] = useState(false);
  const boxRef = useRef<HTMLDivElement>(null);
  const close = (refocus = true) => {
    useWiring.setState({ adding: null });
    if (refocus) onDone();
  };

  // A press anywhere else closes it.
  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (!boxRef.current?.contains(e.target as Node) && !(e.target as HTMLElement).closest?.("[aria-haspopup='dialog']")) useWiring.setState({ adding: null });
    };
    document.addEventListener("pointerdown", onDown, true);
    return () => document.removeEventListener("pointerdown", onDown, true);
  }, []);

  const sections = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const name = (id: string) => data.propById.get(id)?.name ?? "";
    const match = (id: string) => !needle || name(id).toLowerCase().includes(needle);
    const unwired: Choice[] = data.unwired.filter(match).map((id) => ({ id, label: name(id) }));
    const rest: Choice[] = [];
    const move: Choice[] = [];
    for (const prop of data.show.props) {
      const w = data.wiring.get(prop.id);
      if (!w || !match(prop.id)) continue;
      if (w.status === "partial") {
        const gap = firstGap(w.places.map((p) => slotRange(p.slot, data.nodes)).filter((r) => r !== null), w.nodes);
        if (gap) rest.push({ id: prop.id, label: prop.name, detail: `pixels ${thousands(gap.start + 1)}–${thousands(gap.end)}` });
      } else if (w.status === "wired" && w.places.length === 1 && !samePort({ ...w.places[0] }, portRef)) {
        const place = w.places[0];
        move.push({ id: prop.id, label: prop.name, detail: `from ${place.controllerName} · Port ${place.port}` });
      }
    }
    return { unwired, rest, move };
  }, [data, query, portRef]);

  const pick = (id: string) => {
    void apply((show) => wirePropEdits(show, id, { ...portRef, index: END }, data.nodes));
    close();
  };

  // Up and down move through the list; Escape closes.
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const items = [...(boxRef.current?.querySelectorAll<HTMLElement>("[data-pick], input") ?? [])];
    const here = items.indexOf(document.activeElement as HTMLElement);
    const next = items[Math.max(0, Math.min(items.length - 1, here + (e.key === "ArrowDown" ? 1 : -1)))];
    if (next) {
      e.preventDefault();
      next.focus();
    }
  };

  const remaining = data.unwired;
  const total = sections.unwired.length + sections.rest.length + sections.move.length;
  const section = (title: string, choices: Choice[]): ReactNode =>
    choices.length > 0 && (
      <li>
        <p className="px-2 pt-2 pb-0.5 text-[11px] font-medium tracking-wide text-neutral-500 uppercase">{title}</p>
        <ul>
          {choices.map((c) => (
            <li key={c.id}>
              <button
                type="button"
                data-pick=""
                onClick={() => pick(c.id)}
                className="flex w-full items-baseline gap-2 rounded px-2 py-1 text-left text-sm hover:bg-neutral-100 focus:bg-neutral-100 focus:outline-none dark:hover:bg-neutral-800 dark:focus:bg-neutral-800"
              >
                <span className="min-w-0 flex-1 truncate">{c.label}</span>
                {c.detail && <span className="shrink-0 text-xs text-neutral-500">{c.detail}</span>}
              </button>
            </li>
          ))}
        </ul>
      </li>
    );

  return (
    <div
      ref={boxRef}
      role="dialog"
      aria-label={`Add a prop to port ${port.number} of ${controller.name}`}
      onKeyDown={onKeyDown}
      className="absolute top-full left-0 z-30 mt-1 w-72 max-w-[80vw] rounded-lg border border-neutral-200 bg-white p-1.5 shadow-lg dark:border-neutral-700 dark:bg-neutral-900"
    >
      {confirmRest ? (
        <div className="flex flex-col gap-2 p-1.5 text-sm">
          <p>
            Wire {plural(remaining.length, "prop")} onto the end of port {port.number}, left to right as they sit in the layout:{" "}
            {remaining.map((id) => data.propById.get(id)?.name ?? "").join(", ")}?
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setConfirmRest(false)}>
              Cancel
            </Button>
            <Button
              variant="primary"
              autoFocus
              onClick={() => {
                const ids = remaining;
                // Props wired in the meantime are skipped when the edit's turn comes.
                void apply((show) => wireRemainingEdits(show, portRef, ids, data.nodes));
                close();
              }}
            >
              Wire them
            </Button>
          </div>
        </div>
      ) : (
        <>
          <label className="relative block">
            <span className="sr-only">Find a prop to add</span>
            <Search size={14} className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-neutral-400" aria-hidden />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Find a prop"
              className="w-full rounded-md border border-neutral-300 bg-white py-1 pr-2 pl-7 text-sm dark:border-neutral-700 dark:bg-neutral-950"
            />
          </label>
          <ul className="mt-1 max-h-72 overflow-auto">
            {!query.trim() && remaining.length >= 2 && (
              <li>
                <button
                  type="button"
                  data-pick=""
                  onClick={() => setConfirmRest(true)}
                  className="w-full rounded px-2 py-1 text-left text-sm font-medium text-accent-700 hover:bg-accent-50 focus:bg-accent-50 focus:outline-none dark:text-accent-300 dark:hover:bg-accent-600/10 dark:focus:bg-accent-600/10"
                >
                  All {remaining.length} unwired props, left to right…
                </button>
              </li>
            )}
            {section("Not wired", sections.unwired)}
            {section("Add the rest of", sections.rest)}
            {section("Move here", sections.move)}
            {total === 0 && <li className="px-2 py-2 text-sm text-neutral-500">{query.trim() ? `No prop matches “${query.trim()}”.` : "Every prop is wired here or in pieces."}</li>}
          </ul>
        </>
      )}
    </div>
  );
}
