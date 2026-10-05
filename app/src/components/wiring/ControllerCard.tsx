import { ChevronDown, ChevronRight, Plus, Repeat, Settings2, Trash2 } from "lucide-react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import type { ChannelMap, Controller, Port, PortSlot, Prop, Show } from "../../api/types";
import { plural, thousands } from "../../lib/format";
import {
  type NodeCounts,
  type PropWiring,
  type SlotRef,
  addPortEdits,
  moveSlotEdits,
  portCapacity,
  portChannels,
  portPixels,
  removePortEdits,
  renumberPortEdits,
  slotLabel,
  unwireEdits,
  updatePortEdits,
  wirePropEdits,
  wireRemainingEdits,
} from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { samePort, sameSlot, useWiring } from "../../state/wiring";
import { NumberField } from "../layout/PropertiesPanel";
import { Button, Select } from "../ui";
import { OptionalNumberField } from "./fields";
import { useDragSource } from "./useWiringDrag";

/** What every port row needs to know about the show. */
export interface WiringData {
  show: Show;
  nodes: NodeCounts;
  wiring: Map<string, PropWiring>;
  channelMap: ChannelMap;
  /** Unwired props, left to right in the layout. */
  unwired: string[];
}

const ADAPTERS: Record<Controller["adapter"], string> = { fpp: "FPP", falcon: "Falcon", wled: "WLED", generic: "" };

const chipKey = (ref: SlotRef) => `${ref.controller}:${ref.port}:${ref.index}`;

/** Every chip on the screen in order, with where it is. */
function allChips(): { el: HTMLElement; controller: string; port: number; index: number }[] {
  return [...document.querySelectorAll<HTMLElement>("[data-wiring-chip]")].map((el) => ({
    el,
    controller: el.dataset.controller!,
    port: Number(el.dataset.port),
    index: Number(el.dataset.index),
  }));
}

/** Arrow keys between chips: left and right along the port, up and down to the next port with chips. */
function focusNeighbor(ref: SlotRef, key: string) {
  const chips = allChips();
  const here = chips.findIndex((c) => c.controller === ref.controller && c.port === ref.port && c.index === ref.index);
  if (here < 0) return;
  const samePortAs = (c: (typeof chips)[number], other: (typeof chips)[number]) => c.controller === other.controller && c.port === other.port;
  if (key === "ArrowLeft" || key === "ArrowRight") {
    const next = chips[here + (key === "ArrowLeft" ? -1 : 1)];
    if (next && samePortAs(next, chips[here])) next.el.focus();
    return;
  }
  const ports: (typeof chips)[] = [];
  for (const c of chips) {
    const last = ports[ports.length - 1];
    if (last && samePortAs(last[0], c)) last.push(c);
    else ports.push([c]);
  }
  const p = ports.findIndex((group) => samePortAs(group[0], chips[here]));
  const target = ports[p + (key === "ArrowUp" ? -1 : 1)];
  target?.[Math.min(ref.index, target.length - 1)].el.focus();
}

function Chip({ controller, port, index, slot, prop, data }: { controller: Controller; port: Port; index: number; slot: PortSlot; prop: Prop | undefined; data: WiringData }) {
  const ref: SlotRef = { controller: controller.id, port: port.number, index };
  const source = useDragSource({ kind: "slot", from: ref, prop: slot.prop });
  const selected = useWiring((s) => sameSlot(s.selected, ref));
  const moving = useWiring((s) => s.drag?.item.kind === "slot" && sameSlot(s.drag.item.from, ref));
  const apply = useApp((s) => s.apply);
  const name = prop?.name ?? "Missing prop";
  const label = slotLabel(name, slot);
  const status = data.wiring.get(slot.prop)?.status;
  const problem = !prop || status === "twice";
  const nodes = data.nodes.get(slot.prop) ?? 0;
  const pixels = slot.segment ? slot.segment.end - slot.segment.start : nodes;

  const move = async (to: number, focusAt: number) => {
    if (await apply((show) => moveSlotEdits(show, ref, { ...ref, index: to }))) {
      useWiring.setState({ focus: { ...ref, index: focusAt }, selected: null });
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    const earlier = e.key === "ArrowUp" || e.key === "ArrowLeft";
    const later = e.key === "ArrowDown" || e.key === "ArrowRight";
    if (e.altKey && (earlier || later)) {
      e.preventDefault();
      if (earlier && index > 0) void move(index - 1, index - 1);
      if (later && index < port.slots.length - 1) void move(index + 2, index + 1);
      return;
    }
    if (earlier || later) {
      e.preventDefault();
      focusNeighbor(ref, e.key);
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      void apply((show) => unwireEdits(show, ref)).then((ok) => {
        if (!ok) return;
        const left = port.slots.length - 1;
        useWiring.setState({ focus: { ...ref, index: Math.min(index, left - 1 < 0 ? 0 : left - 1) }, selected: null });
      });
      return;
    }
    if (e.key === "Escape" && selected) {
      e.preventDefault();
      useWiring.getState().select(null);
    }
  };

  return (
    <button
      type="button"
      data-wiring-chip=""
      data-controller={controller.id}
      data-port={port.number}
      data-index={index}
      data-chip-key={chipKey(ref)}
      aria-label={`${label} on ${controller.name} port ${port.number}`}
      aria-describedby="wiring-chip-help"
      aria-pressed={selected}
      title={`${label}: ${thousands(pixels)} pixels${slot.reverse ? ", starts at the other end" : ""}${slot.nullPixels ? `, ${slot.nullPixels} empty before it` : ""}`}
      className={`relative flex max-w-full cursor-grab touch-none items-center gap-1.5 rounded-full border py-0.5 pr-2.5 pl-2 text-sm select-none ${
        selected
          ? "border-accent-500 bg-accent-50 ring-2 ring-accent-500/40 dark:bg-accent-600/20"
          : problem
            ? "border-red-400 bg-red-50 text-red-800 dark:border-red-700 dark:bg-red-950/50 dark:text-red-200"
            : "border-neutral-300 bg-neutral-50 hover:border-neutral-400 dark:border-neutral-700 dark:bg-neutral-800 dark:hover:border-neutral-500"
      } ${moving ? "opacity-40" : ""}`}
      onPointerDown={source.onPointerDown}
      onPointerMove={source.onPointerMove}
      onPointerUp={source.onPointerUp}
      onPointerCancel={source.onPointerCancel}
      onClick={() => {
        if (source.endedDrag()) return;
        useWiring.getState().select(selected ? null : ref);
      }}
      onKeyDown={onKeyDown}
    >
      <span className="text-[10px] font-semibold text-neutral-400 tabular-nums" aria-hidden>
        {index + 1}
      </span>
      <span className="truncate">{label}</span>
      {slot.reverse && <Repeat size={12} aria-hidden className="shrink-0 text-accent-600 dark:text-accent-400" />}
      <span className="shrink-0 text-xs text-neutral-500 tabular-nums">{thousands(pixels)}</span>
    </button>
  );
}

const BAR: Record<string, string> = { ok: "bg-emerald-500", near: "bg-amber-500", over: "bg-red-500" };

function CapacityBar({ port, data }: { port: Port; data: WiringData }) {
  const c = portCapacity(port, data.nodes);
  if (c.limit === null) {
    return <span className="text-xs text-neutral-500 tabular-nums">{thousands(c.used)} px</span>;
  }
  const share = c.limit > 0 ? Math.min(1, c.used / c.limit) : 1;
  return (
    <div className="flex min-w-0 items-center gap-2" data-capacity={c.level}>
      <div
        role="meter"
        aria-label={`Port ${port.number} pixels used`}
        aria-valuemin={0}
        aria-valuemax={c.limit}
        aria-valuenow={c.used}
        aria-valuetext={`${thousands(c.used)} of ${thousands(c.limit)} pixels`}
        className="h-1.5 w-28 shrink-0 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800"
      >
        <div className={`h-full rounded-full ${BAR[c.level]}`} style={{ width: `${share * 100}%` }} />
      </div>
      <span
        className={`text-xs tabular-nums ${c.level === "over" ? "font-medium text-red-600 dark:text-red-400" : c.level === "near" ? "text-amber-700 dark:text-amber-400" : "text-neutral-500"}`}
      >
        {thousands(c.used)} / {thousands(c.limit)} px
      </span>
    </div>
  );
}

function PortSettings({ controller, port, onClose }: { controller: Controller; port: Port; onClose: () => void }) {
  const apply = useApp((s) => s.apply);
  const ref = { controller: controller.id, port: port.number };
  const taken = controller.ports.filter((p) => p.number !== port.number).map((p) => p.number);
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
        hint="Most pixels this port can drive, null pixels included. Leave empty if you don't know."
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

function PortRow({ controller, port, data }: { controller: Controller; port: Port; data: WiringData }) {
  const apply = useApp((s) => s.apply);
  const ref = { controller: controller.id, port: port.number };
  const over = useWiring((s) => (s.drag?.over?.kind === "port" && samePort(s.drag.over, ref) ? s.drag.over.index : null));
  const highlighted = useWiring((s) => samePort(s.hovered, ref) || samePort(s.selected, ref));
  const focus = useWiring((s) => (samePort(s.focus, ref) ? s.focus : null));
  const [settings, setSettings] = useState(false);
  const [confirmRest, setConfirmRest] = useState(false);
  const rowRef = useRef<HTMLLIElement>(null);
  const addRef = useRef<HTMLSelectElement>(null);
  const capacity = portCapacity(port, data.nodes);
  const channels = portChannels(data.channelMap, controller.id, port.number);
  const propById = new Map(data.show.props.map((p) => [p.id, p]));
  const restNames = data.unwired.map((id) => propById.get(id)?.name ?? "");

  // After a keyboard move or unwire: focus the chip now at that place (or the Add box if none is left).
  useEffect(() => {
    if (!focus) return;
    const chip = rowRef.current?.querySelector<HTMLElement>(`[data-chip-key="${chipKey(focus)}"]`);
    (chip ?? (port.slots.length === 0 ? addRef.current : null))?.focus();
    useWiring.setState({ focus: null });
  }, [focus, port.slots.length]);

  const indicator = <span aria-hidden className="pointer-events-none absolute -left-1.5 top-0 bottom-0 w-0.5 rounded bg-accent-500" />;

  return (
    <li
      ref={rowRef}
      data-wiring-drop="port"
      data-controller={controller.id}
      data-port={port.number}
      onPointerEnter={() => useWiring.getState().hover(ref)}
      onPointerLeave={() => useWiring.getState().hover(null)}
      className={`border-t border-neutral-200 px-2 py-2 dark:border-neutral-800 ${
        over !== null ? "bg-accent-50 dark:bg-accent-600/10" : highlighted ? "bg-neutral-50 dark:bg-neutral-800/40" : ""
      } ${capacity.level === "over" ? "border-l-2 border-l-red-500" : ""}`}
    >
      <div className="flex flex-wrap items-start gap-x-3 gap-y-1.5">
        <button
          type="button"
          aria-label={`Port ${port.number} settings`}
          aria-expanded={settings}
          onClick={() => setSettings(!settings)}
          className="flex w-16 shrink-0 items-center gap-1 rounded py-0.5 text-sm font-medium hover:text-accent-600 dark:hover:text-accent-400"
        >
          Port {port.number}
          <Settings2 size={12} className="text-neutral-400" aria-hidden />
        </button>
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5" aria-label={`Props on port ${port.number}, in wiring order`} role="group">
          {port.slots.map((slot, i) => (
            <span key={i} className="relative max-w-full">
              {over === i && indicator}
              <Chip controller={controller} port={port} index={i} slot={slot} prop={propById.get(slot.prop)} data={data} />
            </span>
          ))}
          {port.slots.length === 0 ? (
            <span
              className={`rounded-full border border-dashed px-3 py-0.5 text-xs ${
                over !== null ? "border-accent-500 text-accent-700 dark:text-accent-300" : "border-neutral-300 text-neutral-400 dark:border-neutral-700"
              }`}
            >
              {over !== null ? "Let go to wire it here" : "Drop a prop here"}
            </span>
          ) : (
            over === port.slots.length && (
              <span className="relative h-6 w-1">
                <span aria-hidden className="pointer-events-none absolute left-0 top-0 bottom-0 w-0.5 rounded bg-accent-500" />
              </span>
            )
          )}
          <Select
            ref={addRef}
            aria-label={`Add a prop to port ${port.number} of ${controller.name}`}
            className="!py-0.5 text-xs"
            value=""
            onChange={(e) => {
              const id = e.target.value;
              if (!id) return;
              if (id === "__rest") return setConfirmRest(true);
              const to = { ...ref, index: port.slots.length };
              void apply((show) => wirePropEdits(show, id, to, data.nodes));
            }}
          >
            <option value="">+ Add…</option>
            {data.unwired.length >= 2 && <option value="__rest">All {data.unwired.length} unwired props, left to right…</option>}
            {data.unwired.length > 0 && (
              <optgroup label="Not wired">
                {data.unwired.map((id) => (
                  <option key={id} value={id}>
                    {propById.get(id)?.name}
                  </option>
                ))}
              </optgroup>
            )}
            <optgroup label="Move here from another port">
              {data.show.props
                .filter((p) => {
                  const w = data.wiring.get(p.id);
                  return w && w.status !== "unwired" && !(w.places.length === 1 && samePort(w.places[0], ref));
                })
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
            </optgroup>
          </Select>
        </div>
        <div className="flex w-full flex-col items-end gap-0.5 sm:w-auto">
          <CapacityBar port={port} data={data} />
          {channels && (
            <span className="text-[11px] text-neutral-500 tabular-nums" data-testid={`channels-${port.number}`}>
              {channels.universes
                ? `Universe ${channels.universes[0] === channels.universes[1] ? channels.universes[0] : `${channels.universes[0]}–${channels.universes[1]}`} · `
                : ""}
              Ch {thousands(channels.first)}–{thousands(channels.last)}
            </span>
          )}
        </div>
      </div>
      {capacity.message && (
        <p role={capacity.level === "over" ? "alert" : undefined} className={`mt-1 ml-[4.75rem] text-xs ${capacity.level === "over" ? "text-red-600 dark:text-red-400" : "text-amber-700 dark:text-amber-400"}`}>
          {capacity.message}
        </p>
      )}
      {confirmRest && (
        <div className="mt-2 ml-[4.75rem] flex flex-wrap items-center gap-2 rounded-md bg-accent-50 p-2 text-sm dark:bg-accent-600/10">
          <span className="min-w-0 flex-1">
            Wire {plural(data.unwired.length, "prop")} onto the end of port {port.number}, left to right as they sit in the layout:{" "}
            {restNames.join(", ")}?
          </span>
          <Button
            variant="primary"
            onClick={() => {
              setConfirmRest(false);
              const ids = data.unwired;
              void apply((show) => wireRemainingEdits(show, ref, ids));
            }}
          >
            Wire them
          </Button>
          <Button variant="ghost" onClick={() => setConfirmRest(false)}>
            Cancel
          </Button>
        </div>
      )}
      {settings && <PortSettings controller={controller} port={port} onClose={() => setSettings(false)} />}
    </li>
  );
}

/** One controller: its ports as rows, each with its chain of props in wiring order. */
export function ControllerCard({ controller, data }: { controller: Controller; data: WiringData }) {
  const apply = useApp((s) => s.apply);
  const collapsed = useWiring((s) => s.collapsed.includes(controller.id));
  const pixels = controller.ports.reduce((sum, p) => sum + portPixels(p, data.nodes), 0);
  const over = controller.ports.filter((p) => portCapacity(p, data.nodes).level === "over").length;
  const kind = ADAPTERS[controller.adapter];
  return (
    <section className="rounded-lg border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900" aria-label={controller.name}>
      <header className="flex flex-wrap items-center gap-x-3 gap-y-1 p-3">
        <h2 className="min-w-0 font-semibold">
          <button
            type="button"
            aria-expanded={!collapsed}
            onClick={() => useWiring.getState().toggleCollapsed(controller.id)}
            className="flex min-w-0 items-center gap-1 hover:text-accent-600 dark:hover:text-accent-400"
          >
            {collapsed ? <ChevronRight size={16} aria-hidden /> : <ChevronDown size={16} aria-hidden />}
            <span className="truncate">{controller.name}</span>
          </button>
        </h2>
        <span className="text-sm text-neutral-500">{controller.address}</span>
        <span className="rounded bg-neutral-100 px-1.5 py-0.5 text-xs dark:bg-neutral-800">
          {kind && `${kind} · `}
          {controller.protocol.type === "ddp" ? "DDP" : "sACN"}
        </span>
        <span className="text-xs text-neutral-500 tabular-nums">
          {plural(controller.ports.length, "port")} · {thousands(pixels)} px
          {over > 0 && <span className="ml-1 font-medium text-red-600 dark:text-red-400">· {plural(over, "port")} over the limit</span>}
        </span>
        <div className="ml-auto flex gap-1">
          <Button variant="ghost" aria-label={`Add a port to ${controller.name}`} onClick={() => void apply((show) => addPortEdits(show, controller.id))}>
            <Plus size={14} /> Port
          </Button>
          <Button variant="danger" aria-label={`Delete ${controller.name}`} onClick={() => void apply([{ type: "removeController", id: controller.id }])}>
            <Trash2 size={16} />
          </Button>
        </div>
      </header>
      {!collapsed &&
        (controller.ports.length === 0 ? (
          <p className="border-t border-neutral-200 px-3 py-3 text-sm text-neutral-500 dark:border-neutral-800">
            No ports yet. Add one with + Port.
          </p>
        ) : (
          <ul>
            {controller.ports.map((port, i) => (
              <PortRow key={`${port.number}-${i}`} controller={controller} port={port} data={data} />
            ))}
          </ul>
        ))}
    </section>
  );
}
