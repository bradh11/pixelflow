import { AlertTriangle, ChevronDown, ChevronRight, GripVertical, Route, Settings2, Trash2, Unplug } from "lucide-react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import type { Controller, Port, PortSlot, Prop } from "../../api/types";
import { plural, thousands } from "../../lib/format";
import { startSession } from "../../lib/wireSession";
import {
  type Capacity,
  type PortRef,
  type SlotChannels,
  type SlotRef,
  capacityOptions,
  moveSlotByEdits,
  portCapacity,
  portChannels,
  portPixels,
  receiverName,
  removePortEdits,
  renumberPortEdits,
  resolveSlot,
  slotChannels,
  slotLabel,
  universeText,
  unwireEdits,
  updatePortEdits,
  updateSlotEdits,
} from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { portKey, samePort, sameSlot, useWiring } from "../../state/wiring";
import { NumberField } from "../layout/PropertiesPanel";
import { Button } from "../ui";
import { AddPicker } from "./AddPicker";
import type { WiringData } from "./ControllerCard";
import { OptionalNumberField } from "./fields";
import { useDragSource } from "./useWiringDrag";

const rowKey = (controller: string, at: number, index: number) => `${controller}:${at}:${index}`;

/** Up and down arrows move between the props' rows, across ports. */
function focusNeighbor(from: HTMLElement, key: string) {
  const rows = [...document.querySelectorAll<HTMLElement>("[data-wiring-chip]")];
  rows[rows.indexOf(from) + (key === "ArrowUp" ? -1 : 1)]?.focus();
}

/** Where focus goes once the slot is unwired: the next row, else the one before, else the port's Add button. */
export function focusAfterUnwire(port: Port, ref: SlotRef): SlotRef {
  const next = port.slots[ref.index + 1] ?? port.slots[ref.index - 1];
  const index = port.slots[ref.index + 1] ? ref.index : ref.index - 1;
  return next ? { ...ref, index, prop: next.prop, segment: next.segment } : { ...ref, index: port.slots.length, prop: "", segment: null };
}

interface RowProps {
  controller: Controller;
  port: Port;
  at: number;
  index: number;
  slot: PortSlot;
  prop: Prop | undefined;
  data: WiringData;
  channels: SlotChannels | null;
  receivers: boolean;
  /** A drop would land before this row ("before"), or after it, the last ("after"). */
  drop: "before" | "after" | null;
}

const DROP_LINE = { before: "inset 0 2px 0 0 var(--color-accent-500)", after: "inset 0 -2px 0 0 var(--color-accent-500)" };

/** One prop on a port, in wiring order: drag its handle (or Alt+↑/↓ on its name) to reorder. */
function SlotRow({ controller, port, at, index, slot, prop, data, channels, receivers, drop }: RowProps) {
  const ref: SlotRef = { controller: controller.id, port: port.number, at, index, prop: slot.prop, segment: slot.segment };
  const source = useDragSource({ kind: "slot", from: ref, prop: slot.prop });
  const selected = useWiring((s) => sameSlot(s.selected, ref));
  const moving = useWiring((s) => s.drag?.item.kind === "slot" && sameSlot(s.drag.item.from, ref));
  const lit = useWiring((s) => s.hoveredProp === slot.prop && samePort(s.hovered, ref));
  const apply = useApp((s) => s.apply);
  const name = prop?.name ?? "Missing prop";
  const label = slotLabel(name, slot);
  const problem = !prop || data.wiring.get(slot.prop)?.status === "twice";
  const nodes = data.nodes.get(slot.prop) ?? 0;
  const pixels = slot.segment ? slot.segment.end - slot.segment.start : nodes;

  // Each press is built from the show as it is when its turn comes, and finds this prop by
  // identity: a held key keeps moving the same prop, and a second Delete does nothing.
  const move = async (step: -1 | 1) => {
    if (await apply((show) => moveSlotByEdits(show, ref, step))) useWiring.setState({ focus: ref });
  };
  const unwire = () => {
    const then = focusAfterUnwire(port, ref);
    void apply((show) => unwireEdits(show, ref)).then((ok) => ok && useWiring.setState({ focus: then }));
  };
  const toggle = () => useWiring.getState().select(selected ? null : ref);

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      if (e.altKey) void move(e.key === "ArrowUp" ? -1 : 1);
      else focusNeighbor(e.currentTarget, e.key);
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      unwire();
      return;
    }
    if (e.key === "Escape" && selected) {
      e.preventDefault();
      useWiring.getState().select(null);
    }
  };

  const cell = "px-1.5 py-1 tabular-nums";
  return (
    <tr
      data-wiring-row=""
      onPointerEnter={() => useWiring.setState({ hovered: ref, hoveredProp: slot.prop })}
      onPointerLeave={() => useWiring.setState({ hoveredProp: null })}
      style={drop ? { boxShadow: DROP_LINE[drop] } : undefined}
      className={`border-t border-neutral-100 dark:border-neutral-800/70 ${selected ? "bg-accent-50 dark:bg-accent-600/15" : lit ? "bg-neutral-50 dark:bg-neutral-800/50" : ""} ${moving ? "opacity-40" : ""}`}
    >
      <td className="w-12 py-1 pl-0.5">
        <span className="flex items-center">
          <button
            type="button"
            tabIndex={-1}
            aria-label={`Drag ${label} to reorder`}
            title="Drag to reorder (from the keyboard: Alt+↑/↓ on the prop's name)"
            className="cursor-grab touch-none rounded p-0.5 text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
            onPointerDown={source.onPointerDown}
            onPointerMove={source.onPointerMove}
            onPointerUp={source.onPointerUp}
            onPointerCancel={source.onPointerCancel}
            onLostPointerCapture={source.onLostPointerCapture}
            onClick={() => source.endedDrag()}
          >
            <GripVertical size={14} aria-hidden />
          </button>
          <span className="w-5 text-right text-xs text-neutral-400 tabular-nums">{index + 1}</span>
        </span>
      </td>
      <td className="max-w-0 py-1 pr-1.5">
        <button
          type="button"
          data-wiring-chip=""
          data-controller={controller.id}
          data-port-at={at}
          data-index={index}
          data-chip-key={rowKey(controller.id, at, index)}
          aria-label={`${label} on ${controller.name} port ${port.number}`}
          aria-describedby="wiring-chip-help"
          aria-pressed={selected}
          title="Open its settings"
          onClick={toggle}
          onKeyDown={onKeyDown}
          className={`flex max-w-full items-center gap-1 rounded px-1 text-left hover:underline ${problem ? "text-red-700 dark:text-red-300" : ""}`}
        >
          {problem && <AlertTriangle size={12} aria-hidden className="shrink-0" />}
          <span className="truncate">{label}</span>
        </button>
      </td>
      <td className={`${cell} text-right`}>{thousands(pixels)}</td>
      <td className={`${cell} hidden text-right text-neutral-500 @min-[560px]:table-cell`}>{channels ? thousands(channels.first) : "—"}</td>
      <td className={`${cell} hidden text-neutral-500 @min-[560px]:table-cell`}>{universeText(channels?.universes ?? null) || "—"}</td>
      {receivers && <td className={`${cell} text-center`}>{slot.smartReceiver !== null ? receiverName(slot.smartReceiver) : "—"}</td>}
      <td className={`${cell} text-center`}>
        <input
          type="checkbox"
          aria-label={`${label} starts at the other end`}
          title="Starts at the other end (the wire plugs into its far end)"
          checked={slot.reverse}
          onChange={(e) => {
            const reverse = e.target.checked;
            void apply((show) => updateSlotEdits(show, ref, (s) => ({ ...s, reverse })));
          }}
        />
      </td>
      <td className={`${cell} text-right text-neutral-500`}>{slot.nullPixels || "—"}</td>
      <td className="py-1 pr-1">
        <span className="flex justify-end">
          <button
            type="button"
            aria-label={`Settings for ${label}`}
            title="Settings: pixels, receiver, brightness"
            aria-pressed={selected}
            onClick={toggle}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-100 hover:text-neutral-800 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
          >
            <Settings2 size={14} aria-hidden />
          </button>
          <button
            type="button"
            aria-label={`Unwire ${label}`}
            title="Unwire (back to the props list)"
            onClick={unwire}
            className="rounded p-1 text-neutral-500 hover:bg-red-50 hover:text-red-700 dark:hover:bg-red-950/40 dark:hover:text-red-300"
          >
            <Unplug size={14} aria-hidden />
          </button>
        </span>
      </td>
    </tr>
  );
}

const BAR: Record<string, string> = { ok: "bg-emerald-500", near: "bg-amber-500", slow: "bg-amber-500", over: "bg-red-500" };

/** "A 50 · B 512": each smart receiver's share of the port's pixels. */
export function receiverBreakdown(c: Capacity): string {
  return c.receivers.map((r) => `${r.receiver === null ? "Port" : receiverName(r.receiver)} ${thousands(r.used)}`).join(" · ");
}

/** One bar per port: every smart receiver on it counts against the port's one limit (as in xLights). */
export function CapacityBar({ port, controller, c }: { port: Port; controller: Controller; c: Capacity }) {
  const breakdown = c.receivers.length > 0 ? receiverBreakdown(c) : "";
  if (c.limit === null) {
    return (
      <span className="text-xs text-neutral-500 tabular-nums" title={breakdown ? `Smart receivers: ${breakdown} px` : undefined}>
        {thousands(c.used)} px
      </span>
    );
  }
  const share = c.limit > 0 ? Math.min(1, c.used / c.limit) : 1;
  const notes = [
    breakdown && `Smart receivers share the port's limit: ${breakdown} px.`,
    c.refresh !== null && `About ${thousands(c.refresh)} pixels refresh in time at the show's frame rate (xLights' figure for Falcon V4/V5 boards).`,
    controller.adapter === "falcon" && "The board also counts any null pixels it is set to skip itself; those aren't counted here.",
  ].filter(Boolean);
  // Each receiver's share drawn as a segment of the one bar, so the split shows at a glance.
  let at = 0;
  const segments = c.receivers.map((r) => {
    const left = at;
    at += c.limit! > 0 ? r.used / c.limit! : 1;
    return { key: r.receiver ?? "port", left: Math.min(1, left) };
  });
  return (
    <div className="flex min-w-0 items-center gap-2" data-capacity={c.level} title={notes.join(" ") || undefined}>
      {breakdown && (
        <span className="text-[10px] text-neutral-500 tabular-nums" data-testid={`receivers-${port.number}`}>
          {breakdown}
        </span>
      )}
      <div
        role="meter"
        aria-label={`Port ${port.number} pixels used`}
        aria-valuemin={0}
        aria-valuemax={c.limit}
        aria-valuenow={c.used}
        aria-valuetext={`${thousands(c.used)} of ${thousands(c.limit)} pixels${breakdown ? ` (${breakdown})` : ""}`}
        className="relative h-1.5 w-20 shrink-0 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800"
      >
        <div className={`h-full rounded-full ${BAR[c.level]}`} style={{ width: `${share * 100}%` }} />
        {segments.slice(1).map((s) => (
          <span key={s.key} aria-hidden className="absolute top-0 bottom-0 w-px bg-white dark:bg-neutral-900" style={{ left: `${s.left * 100}%` }} />
        ))}
        {c.refresh !== null && <span aria-hidden className="absolute top-0 bottom-0 w-px bg-neutral-500" style={{ left: `${(c.refresh / c.limit) * 100}%` }} />}
      </div>
      <span
        className={`text-xs whitespace-nowrap tabular-nums ${c.level === "over" ? "font-medium text-red-600 dark:text-red-400" : c.level === "near" || c.level === "slow" ? "text-amber-700 dark:text-amber-400" : "text-neutral-500"}`}
      >
        {thousands(c.used)} / {thousands(c.limit)} px
      </span>
    </div>
  );
}

function PortSettings({ controller, port, at, onClose }: { controller: Controller; port: Port; at: number; onClose: () => void }) {
  const apply = useApp((s) => s.apply);
  const ref: PortRef = { controller: controller.id, port: port.number, at };
  const taken = controller.ports.filter((_, i) => i !== at).map((p) => p.number);
  const [clash, setClash] = useState<number | null>(null);
  return (
    <div className="mt-2 mb-1 grid grid-cols-2 items-end gap-3 rounded-md bg-neutral-50 p-3 sm:grid-cols-[8rem_10rem_1fr] dark:bg-neutral-950/60">
      <NumberField
        label="Port number"
        hint="The number printed on the controller next to this port."
        value={port.number}
        min={1}
        max={65535}
        integer
        onCommit={(n) => {
          if (taken.includes(n)) return setClash(n);
          setClash(null);
          void apply((show) => renumberPortEdits(show, ref, n));
          onClose();
        }}
      />
      <OptionalNumberField
        label="Pixel limit"
        hint="Most pixels this port can drive, null pixels included (RGBW pixels count as 1⅓). Smart receivers on the port share this limit. Leave empty if you don't know."
        value={port.maxPixels}
        min={1}
        max={1_000_000}
        integer
        placeholder="Not known"
        onCommit={(maxPixels) => void apply((show) => updatePortEdits(show, ref, (p) => ({ ...p, maxPixels })))}
      />
      <div className="col-span-2 flex items-center gap-2 sm:col-span-1 sm:justify-end">
        <Button
          variant="danger"
          onClick={() => {
            onClose();
            void apply((show) => removePortEdits(show, ref));
          }}
        >
          <Trash2 size={14} /> Remove port
          {port.slots.length > 0 && <span className="font-normal">(unwires {plural(port.slots.length, "prop")})</span>}
        </Button>
        <Button variant="ghost" onClick={onClose}>
          Done
        </Button>
      </div>
      {clash !== null && <p className="col-span-full text-xs text-red-600 dark:text-red-400">Another port is already number {clash}.</p>}
    </div>
  );
}

/**
 * One port: a one-line header (its number, what's on it, how full it is, and "Wire on the
 * layout"), then its props as a table in wiring order. Folded, the header says it all.
 */
export function PortRow({ controller, port, at, data }: { controller: Controller; port: Port; at: number; data: WiringData }) {
  const ref: PortRef = { controller: controller.id, port: port.number, at };
  const over = useWiring((s) => (s.drag?.over?.kind === "port" && samePort(s.drag.over, ref) ? s.drag.over.index : null));
  const highlighted = useWiring((s) => samePort(s.hovered, ref) || samePort(s.selected, ref));
  const focus = useWiring((s) => (samePort(s.focus, ref) ? s.focus : null));
  const adding = useWiring((s) => samePort(s.adding, ref));
  const folded = useWiring((s) => s.folded.includes(portKey(controller.id, at)));
  const [settings, setSettings] = useState(false);
  const rowRef = useRef<HTMLLIElement>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const capacity = portCapacity(port, data.nodes, capacityOptions(data.show, controller, data.cpp));
  const channels = portChannels(data.channelMap, controller.id, port.number);
  const perSlot = folded ? [] : slotChannels(data.channelMap, controller.id, port, data.nodes);
  const receivers = port.slots.some((s) => s.smartReceiver !== null);
  const pixels = portPixels(port, data.nodes);
  const names = port.slots.map((s) => slotLabel(data.propById.get(s.prop)?.name ?? "Missing prop", s));
  const universes = universeText(channels?.universes ?? null);
  const summary =
    port.slots.length === 0 ? "Nothing wired" : [folded ? names.join(" → ") : plural(names.length, "prop"), `${thousands(pixels)} px`, universes].filter(Boolean).join(" · ");

  // After a keyboard move, an unwire, or closing a row's settings: focus that row where it is
  // now (or the Add button when it's gone and the port is empty).
  useEffect(() => {
    if (!focus) return;
    const index = focus.prop ? resolveSlot(port, focus) : null;
    const row = index === null ? null : rowRef.current?.querySelector<HTMLElement>(`[data-chip-key="${rowKey(controller.id, at, index)}"]`);
    (row ?? (port.slots.length === 0 || !focus.prop ? addRef.current : null))?.focus();
    useWiring.setState({ focus: null });
  }, [focus, port, controller.id, at]);

  const th = "px-1.5 py-1 font-medium";
  return (
    <li
      ref={rowRef}
      data-wiring-drop="port"
      data-controller={controller.id}
      data-port={port.number}
      data-port-at={at}
      data-folded={folded ? "" : undefined}
      onPointerEnter={() => useWiring.getState().hover(ref)}
      onPointerLeave={() => useWiring.setState({ hovered: null, hoveredProp: null })}
      className={`@container border-t border-neutral-200 px-2 py-1.5 dark:border-neutral-800 ${
        over !== null ? "bg-accent-50 dark:bg-accent-600/10" : highlighted ? "bg-neutral-50/70 dark:bg-neutral-800/30" : ""
      } ${capacity.level === "over" ? "border-l-2 border-l-red-500" : ""}`}
    >
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <button
          type="button"
          aria-label={`${folded ? "Show" : "Hide"} the props on port ${port.number}`}
          title={folded ? "Show the table" : "Fold to one line"}
          aria-expanded={!folded}
          onClick={() => useWiring.getState().toggleFolded(portKey(controller.id, at))}
          className="rounded p-0.5 text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-100"
        >
          {folded ? <ChevronRight size={14} aria-hidden /> : <ChevronDown size={14} aria-hidden />}
        </button>
        <button
          type="button"
          aria-label={`Port ${port.number} settings`}
          aria-expanded={settings}
          onClick={() => setSettings(!settings)}
          title="Port settings: number, pixel limit, remove"
          className="flex shrink-0 items-center gap-1 rounded text-sm font-medium hover:text-accent-600 dark:hover:text-accent-400"
        >
          Port {port.number}
          <Settings2 size={12} className="text-neutral-400" aria-hidden />
        </button>
        <span className="min-w-0 flex-1 truncate text-xs text-neutral-500" data-testid={`summary-${port.number}`} title={names.join(" → ") || undefined}>
          {summary}
        </span>
        {channels && (
          <span className="hidden text-[11px] text-neutral-500 tabular-nums @min-[520px]:inline" data-testid={`channels-${port.number}`}>
            Ch {thousands(channels.first)}–{thousands(channels.last)}
          </span>
        )}
        <CapacityBar port={port} controller={controller} c={capacity} />
        <button
          type="button"
          data-wire-button={portKey(controller.id, at)}
          aria-label={`Wire port ${port.number} of ${controller.name} on the layout`}
          title="Wire on the layout: click props on the house picture in the order the wire runs"
          onClick={() => useWiring.setState({ session: startSession(data.show, { ...ref, at }), selected: null, adding: null })}
          className="flex items-center gap-1 rounded-md border border-neutral-300 px-1.5 py-0.5 text-xs text-neutral-700 hover:border-accent-500 hover:text-accent-700 dark:border-neutral-700 dark:text-neutral-300 dark:hover:text-accent-300"
        >
          <Route size={12} aria-hidden /> Wire
        </button>
      </div>
      {capacity.message && (
        <p
          role={capacity.level === "over" ? "alert" : undefined}
          className={`mt-0.5 ml-6 text-xs ${capacity.level === "over" ? "text-red-600 dark:text-red-400" : "text-amber-700 dark:text-amber-400"}`}
        >
          {capacity.message}
        </p>
      )}
      {settings && <PortSettings controller={controller} port={port} at={at} onClose={() => setSettings(false)} />}
      {!folded && (
        <div className="mt-1 ml-5 @container">
          {port.slots.length > 0 ? (
            <table className="w-full table-fixed text-sm" aria-label={`Props on port ${port.number} of ${controller.name}, in wiring order`}>
              <thead className="text-left text-[11px] text-neutral-500">
                <tr>
                  <th className={`${th} w-12 pl-1`}>#</th>
                  <th className={th}>Prop</th>
                  <th className={`${th} w-16 text-right`}>Pixels</th>
                  <th className={`${th} hidden w-20 text-right @min-[560px]:table-cell`}>Start ch</th>
                  <th className={`${th} hidden w-20 @min-[560px]:table-cell`}>Universe</th>
                  {receivers && (
                    <th className={`${th} w-10 text-center`} title="Smart receiver">
                      Rx
                    </th>
                  )}
                  <th className={`${th} w-10 text-center`} title="Starts at the other end">
                    Rev
                  </th>
                  <th className={`${th} w-11 text-right`} title="Empty pixels before it">
                    Null
                  </th>
                  <th className={`${th} w-16`}>
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {port.slots.map((slot, i) => (
                  <SlotRow
                    key={i}
                    controller={controller}
                    port={port}
                    at={at}
                    index={i}
                    slot={slot}
                    prop={data.propById.get(slot.prop)}
                    data={data}
                    channels={perSlot[i] ?? null}
                    receivers={receivers}
                    drop={over === i ? "before" : over !== null && over >= port.slots.length && i === port.slots.length - 1 ? "after" : null}
                  />
                ))}
              </tbody>
            </table>
          ) : null}
          {/* The Add button on the left, so its picker opens over the table. */}
          <div className="mt-1 flex flex-row-reverse items-center justify-end gap-2">
            {port.slots.length === 0 && (
              <p
                className={`min-w-0 flex-1 rounded-md border border-dashed px-3 py-1 text-xs ${
                  over !== null ? "border-accent-500 text-accent-700 dark:text-accent-300" : "border-neutral-300 text-neutral-500 dark:border-neutral-700"
                }`}
              >
                {over !== null ? "Let go to wire it here" : "Drop a prop here, or use Wire to click props on the layout."}
              </p>
            )}
            <span className="relative shrink-0">
            <button
              ref={addRef}
              type="button"
              aria-label={`Add a prop to port ${port.number} of ${controller.name}`}
              aria-haspopup="dialog"
              aria-expanded={adding}
              onClick={() => useWiring.setState({ adding: adding ? null : ref })}
              title="Pick a prop to wire here (or drag one from the list)"
              className="rounded px-1.5 py-0.5 text-xs text-neutral-500 hover:bg-neutral-100 hover:text-accent-700 dark:hover:bg-neutral-800 dark:hover:text-accent-300"
            >
              + Add…
            </button>
            {/* One picker at a time, built only while it's open. */}
            {adding && <AddPicker controller={controller} port={port} portRef={ref} data={data} onDone={() => addRef.current?.focus()} />}
            </span>
          </div>
        </div>
      )}
    </li>
  );
}
