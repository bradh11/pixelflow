import { ChevronDown, ChevronRight, Pencil, Plus, Repeat, Settings2, Trash2 } from "lucide-react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import type { ChannelMap, Controller, Port, PortSlot, Prop, Show } from "../../api/types";
import { plural, thousands } from "../../lib/format";
import {
  type Capacity,
  type NodeCounts,
  type PortRef,
  type PropWiring,
  type SlotRef,
  addPortEdits,
  capacityOptions,
  moveSlotByEdits,
  portCapacities,
  portCapacity,
  portChannels,
  portPixels,
  receiverName,
  removePortEdits,
  renumberPortEdits,
  resolveSlot,
  slotLabel,
  unwireEdits,
  updatePortEdits,
} from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { samePort, sameSlot, useWiring } from "../../state/wiring";
import { NumberField } from "../layout/PropertiesPanel";
import { Button } from "../ui";
import { AddPicker } from "./AddPicker";
import { ControllerEditForm } from "./ControllerEditForm";
import { controllerEdits, controllerDraft } from "../../lib/controllerEdit";
import { OptionalNumberField } from "./fields";
import { useDragSource } from "./useWiringDrag";

/** What every port row needs to know about the show. */
export interface WiringData {
  show: Show;
  nodes: NodeCounts;
  /** Channels per pixel by prop. */
  cpp: ReadonlyMap<string, number>;
  propById: ReadonlyMap<string, Prop>;
  wiring: Map<string, PropWiring>;
  channelMap: ChannelMap;
  /** Unwired props, left to right in the layout. */
  unwired: string[];
}

const ADAPTERS: Record<Controller["adapter"], string> = { fpp: "FPP", falcon: "Falcon", wled: "WLED", generic: "" };

const chipKey = (controller: string, at: number, index: number) => `${controller}:${at}:${index}`;

/** Every chip on the screen in order, with where it is. */
function allChips(): { el: HTMLElement; port: string; index: number }[] {
  return [...document.querySelectorAll<HTMLElement>("[data-wiring-chip]")].map((el) => ({
    el,
    port: `${el.dataset.controller}:${el.dataset.portAt}`,
    index: Number(el.dataset.index),
  }));
}

/** Arrow keys between chips: left and right along the port, up and down to the next port with chips. */
function focusNeighbor(from: HTMLElement, key: string) {
  const chips = allChips();
  const here = chips.findIndex((c) => c.el === from);
  if (here < 0) return;
  if (key === "ArrowLeft" || key === "ArrowRight") {
    const next = chips[here + (key === "ArrowLeft" ? -1 : 1)];
    if (next && next.port === chips[here].port) next.el.focus();
    return;
  }
  const ports: (typeof chips)[] = [];
  for (const c of chips) {
    const last = ports[ports.length - 1];
    if (last && last[0].port === c.port) last.push(c);
    else ports.push([c]);
  }
  const p = ports.findIndex((group) => group[0].port === chips[here].port);
  const target = ports[p + (key === "ArrowUp" ? -1 : 1)];
  target?.[Math.min(chips[here].index, target.length - 1)].el.focus();
}

/** Where focus goes once the slot is unwired: the next chip, else the one before, else the port's Add button. */
export function focusAfterUnwire(port: Port, ref: SlotRef): SlotRef {
  const next = port.slots[ref.index + 1] ?? port.slots[ref.index - 1];
  const index = port.slots[ref.index + 1] ? ref.index : ref.index - 1;
  return next ? { ...ref, index, prop: next.prop, segment: next.segment } : { ...ref, index: port.slots.length, prop: "", segment: null };
}

function Chip({ controller, port, at, index, slot, prop, data }: { controller: Controller; port: Port; at: number; index: number; slot: PortSlot; prop: Prop | undefined; data: WiringData }) {
  const ref: SlotRef = { controller: controller.id, port: port.number, at, index, prop: slot.prop, segment: slot.segment };
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

  // Each press is built from the show as it is when its turn comes, and finds this prop by
  // identity: a held key keeps moving the same prop, and a second Delete does nothing.
  const move = async (step: -1 | 1) => {
    if (await apply((show) => moveSlotByEdits(show, ref, step))) useWiring.setState({ focus: ref });
  };

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    const earlier = e.key === "ArrowUp" || e.key === "ArrowLeft";
    const later = e.key === "ArrowDown" || e.key === "ArrowRight";
    if (e.altKey && (earlier || later)) {
      e.preventDefault();
      void move(earlier ? -1 : 1);
      return;
    }
    if (earlier || later) {
      e.preventDefault();
      focusNeighbor(e.currentTarget, e.key);
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      const then = focusAfterUnwire(port, ref);
      void apply((show) => unwireEdits(show, ref)).then((ok) => ok && useWiring.setState({ focus: then }));
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
      data-port-at={at}
      data-index={index}
      data-chip-key={chipKey(controller.id, at, index)}
      aria-label={`${label} on ${controller.name} port ${port.number}`}
      aria-describedby="wiring-chip-help"
      aria-pressed={selected}
      title={`${label}: ${thousands(pixels)} pixels${slot.reverse ? ", starts at the other end" : ""}${slot.nullPixels ? `, ${slot.nullPixels} empty before it` : ""}${slot.smartReceiver !== null ? `, smart receiver ${receiverName(slot.smartReceiver)}` : ""}`}
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
      onLostPointerCapture={source.onLostPointerCapture}
      onClick={() => {
        if (source.endedDrag()) return;
        useWiring.getState().select(selected ? null : ref);
      }}
      onKeyDown={onKeyDown}
    >
      <span className="text-[10px] font-semibold text-neutral-400 tabular-nums" aria-hidden>
        {index + 1}
      </span>
      {slot.smartReceiver !== null && (
        <span className="rounded bg-neutral-200 px-1 text-[10px] font-semibold text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300" aria-hidden>
          {receiverName(slot.smartReceiver)}
        </span>
      )}
      <span className="truncate">{label}</span>
      {slot.reverse && <Repeat size={12} aria-hidden className="shrink-0 text-accent-600 dark:text-accent-400" />}
      <span className="shrink-0 text-xs text-neutral-500 tabular-nums">{thousands(pixels)}</span>
    </button>
  );
}

const BAR: Record<string, string> = { ok: "bg-emerald-500", near: "bg-amber-500", slow: "bg-amber-500", over: "bg-red-500" };

function CapacityBar({ port, controller, c }: { port: Port; controller: Controller; c: Capacity }) {
  const receiver = c.receiver === null ? "" : ` receiver ${receiverName(c.receiver)}`;
  if (c.limit === null) {
    return (
      <span className="text-xs text-neutral-500 tabular-nums">
        {receiver && `${receiverName(c.receiver!)} · `}
        {thousands(c.used)} px
      </span>
    );
  }
  const share = c.limit > 0 ? Math.min(1, c.used / c.limit) : 1;
  const notes = [
    c.refresh !== null && `About ${thousands(c.refresh)} pixels refresh in time at the show's frame rate (xLights' figure for Falcon V4/V5 boards).`,
    controller.adapter === "falcon" && "The board also counts any null pixels it is set to skip itself; those aren't counted here.",
  ].filter(Boolean);
  return (
    <div className="flex min-w-0 items-center gap-2" data-capacity={c.level} title={notes.join(" ") || undefined}>
      {receiver && <span className="text-[10px] font-semibold text-neutral-500">{receiverName(c.receiver!)}</span>}
      <div
        role="meter"
        aria-label={`Port ${port.number}${receiver} pixels used`}
        aria-valuemin={0}
        aria-valuemax={c.limit}
        aria-valuenow={c.used}
        aria-valuetext={`${thousands(c.used)} of ${thousands(c.limit)} pixels`}
        className="relative h-1.5 w-28 shrink-0 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800"
      >
        <div className={`h-full rounded-full ${BAR[c.level]}`} style={{ width: `${share * 100}%` }} />
        {c.refresh !== null && <span aria-hidden className="absolute top-0 bottom-0 w-px bg-neutral-500" style={{ left: `${(c.refresh / c.limit) * 100}%` }} />}
      </div>
      <span
        className={`text-xs tabular-nums ${c.level === "over" ? "font-medium text-red-600 dark:text-red-400" : c.level === "near" || c.level === "slow" ? "text-amber-700 dark:text-amber-400" : "text-neutral-500"}`}
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
        hint="Most pixels this port can drive, null pixels included (RGBW pixels count as 1⅓). With smart receivers, each receiver gets this many. Leave empty if you don't know."
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

function PortRow({ controller, port, at, data }: { controller: Controller; port: Port; at: number; data: WiringData }) {
  const ref: PortRef = { controller: controller.id, port: port.number, at };
  const over = useWiring((s) => (s.drag?.over?.kind === "port" && samePort(s.drag.over, ref) ? s.drag.over.index : null));
  const highlighted = useWiring((s) => samePort(s.hovered, ref) || samePort(s.selected, ref));
  const focus = useWiring((s) => (samePort(s.focus, ref) ? s.focus : null));
  const adding = useWiring((s) => samePort(s.adding, ref));
  const [settings, setSettings] = useState(false);
  const rowRef = useRef<HTMLLIElement>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const capacities = portCapacities(port, data.nodes, capacityOptions(data.show, controller, data.cpp));
  const worst = portCapacity(port, data.nodes, capacityOptions(data.show, controller, data.cpp));
  const channels = portChannels(data.channelMap, controller.id, port.number);
  const messages = capacities.filter((c) => c.message);

  // After a keyboard move, an unwire, or closing a chip's settings: focus that chip where it is
  // now (or the Add button when it's gone and the port is empty).
  useEffect(() => {
    if (!focus) return;
    const index = focus.prop ? resolveSlot(port, focus) : null;
    const chip = index === null ? null : rowRef.current?.querySelector<HTMLElement>(`[data-chip-key="${chipKey(controller.id, at, index)}"]`);
    (chip ?? (port.slots.length === 0 || !focus.prop ? addRef.current : null))?.focus();
    useWiring.setState({ focus: null });
  }, [focus, port, controller.id, at]);

  const indicator = <span aria-hidden className="pointer-events-none absolute -left-1.5 top-0 bottom-0 w-0.5 rounded bg-accent-500" />;

  return (
    <li
      ref={rowRef}
      data-wiring-drop="port"
      data-controller={controller.id}
      data-port={port.number}
      data-port-at={at}
      onPointerEnter={() => useWiring.getState().hover(ref)}
      onPointerLeave={() => useWiring.getState().hover(null)}
      className={`border-t border-neutral-200 px-2 py-2 dark:border-neutral-800 ${
        over !== null ? "bg-accent-50 dark:bg-accent-600/10" : highlighted ? "bg-neutral-50 dark:bg-neutral-800/40" : ""
      } ${worst.level === "over" ? "border-l-2 border-l-red-500" : ""}`}
    >
      <div className="flex flex-wrap items-start gap-x-3 gap-y-1.5">
        <button
          type="button"
          aria-label={`Port ${port.number} settings`}
          aria-expanded={settings}
          onClick={() => setSettings(!settings)}
          title="Port settings: number, pixel limit, remove"
          className="flex min-h-7 w-16 shrink-0 items-center gap-1 rounded py-0.5 text-sm font-medium hover:text-accent-600 dark:hover:text-accent-400"
        >
          Port {port.number}
          <Settings2 size={12} className="text-neutral-400" aria-hidden />
        </button>
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5" aria-label={`Props on port ${port.number}, in wiring order`} role="group">
          {port.slots.map((slot, i) => (
            <span key={i} className="relative max-w-full">
              {over === i && indicator}
              <Chip controller={controller} port={port} at={at} index={i} slot={slot} prop={data.propById.get(slot.prop)} data={data} />
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
          <span className="relative">
            <button
              ref={addRef}
              type="button"
              aria-label={`Add a prop to port ${port.number} of ${controller.name}`}
              aria-haspopup="dialog"
              aria-expanded={adding}
              onClick={() => useWiring.setState({ adding: adding ? null : ref })}
              title="Pick a prop to wire here (or drag one from the list)"
              className="min-h-7 rounded-full border border-neutral-300 px-2.5 py-0.5 text-xs text-neutral-600 hover:border-accent-500 hover:text-accent-700 dark:border-neutral-700 dark:text-neutral-300 dark:hover:text-accent-300"
            >
              + Add…
            </button>
            {/* One picker at a time, built only while it's open. */}
            {adding && <AddPicker controller={controller} port={port} portRef={ref} data={data} onDone={() => addRef.current?.focus()} />}
          </span>
        </div>
        <div className="flex w-full flex-col items-end gap-0.5 sm:w-auto">
          {capacities.map((c) => (
            <CapacityBar key={c.receiver ?? "port"} port={port} controller={controller} c={c} />
          ))}
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
      {messages.map((c) => (
        <p
          key={c.receiver ?? "port"}
          role={c.level === "over" ? "alert" : undefined}
          className={`mt-1 ml-[4.75rem] text-xs ${c.level === "over" ? "text-red-600 dark:text-red-400" : "text-amber-700 dark:text-amber-400"}`}
        >
          {c.message}
        </p>
      ))}
      {settings && <PortSettings controller={controller} port={port} at={at} onClose={() => setSettings(false)} />}
    </li>
  );
}

/** The controller's name, typed over in place (double-click the name): Enter saves, Escape doesn't. */
function RenameField({ controller, onDone }: { controller: Controller; onDone: () => void }) {
  const apply = useApp((s) => s.apply);
  const [name, setName] = useState(controller.name);
  const taken = useApp((s) => s.snapshot?.show.controllers.some((c) => c.id !== controller.id && c.name.trim().toLowerCase() === name.trim().toLowerCase()) ?? false);
  const problem = !name.trim() ? "Give the controller a name." : taken ? `Another controller is already called ${name.trim()}.` : null;
  const commit = () => {
    if (!problem) void apply(controllerEdits(controller.id, { ...controllerDraft(controller), name }));
    onDone();
  };
  return (
    <span className="flex min-w-0 flex-col">
      <input
        autoFocus
        aria-label={`Name of ${controller.name}`}
        aria-invalid={!!problem}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            e.stopPropagation();
            onDone();
          }
        }}
        className="rounded-md border border-neutral-300 bg-white px-2 py-0.5 font-semibold dark:border-neutral-700 dark:bg-neutral-950"
      />
      {problem && <span className="text-xs font-normal text-red-600 dark:text-red-400">{problem}</span>}
    </span>
  );
}

/** One controller: its ports as rows, each with its chain of props in wiring order. */
export function ControllerCard({ controller, data }: { controller: Controller; data: WiringData }) {
  const apply = useApp((s) => s.apply);
  const collapsed = useWiring((s) => s.collapsed.includes(controller.id));
  const [editing, setEditing] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const pixels = controller.ports.reduce((sum, p) => sum + portPixels(p, data.nodes), 0);
  const options = capacityOptions(data.show, controller, data.cpp);
  const over = controller.ports.filter((p) => portCapacity(p, data.nodes, options).level === "over").length;
  const kind = ADAPTERS[controller.adapter];
  const wired = controller.ports.reduce((sum, p) => sum + p.slots.length, 0);
  const sacn = controller.protocol.type === "sacn" ? controller.protocol : null;
  return (
    <section className="rounded-lg border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900" aria-label={controller.name}>
      <header className="flex flex-wrap items-center gap-x-3 gap-y-1 p-3">
        <h2 className="min-w-0 font-semibold">
          {renaming ? (
            <RenameField controller={controller} onDone={() => setRenaming(false)} />
          ) : (
            <button
              type="button"
              aria-expanded={!collapsed}
              title="Click to fold away, double-click to rename"
              onClick={() => useWiring.getState().toggleCollapsed(controller.id)}
              onDoubleClick={() => setRenaming(true)}
              className="flex min-w-0 items-center gap-1 hover:text-accent-600 dark:hover:text-accent-400"
            >
              {collapsed ? <ChevronRight size={16} aria-hidden /> : <ChevronDown size={16} aria-hidden />}
              <span className="truncate">{controller.name}</span>
            </button>
          )}
        </h2>
        <span className="text-sm text-neutral-500">{controller.address}</span>
        <span className="rounded bg-neutral-100 px-1.5 py-0.5 text-xs dark:bg-neutral-800">
          {kind && `${kind} · `}
          {sacn ? `sACN${sacn.startUniverse !== null ? ` · from universe ${sacn.startUniverse}` : ""}` : "DDP"}
        </span>
        <span className="text-xs text-neutral-500 tabular-nums">
          {plural(controller.ports.length, "port")} · {thousands(pixels)} px
          {over > 0 && <span className="ml-1 font-medium text-red-600 dark:text-red-400">· {plural(over, "port")} over the limit</span>}
        </span>
        <div className="ml-auto flex gap-1">
          <Button
            variant="ghost"
            aria-label={`Edit ${controller.name}`}
            title="Change the name, address, or protocol"
            aria-expanded={editing}
            onClick={() => setEditing(!editing)}
          >
            <Pencil size={14} /> Edit
          </Button>
          <Button variant="ghost" aria-label={`Add a port to ${controller.name}`} title="Add a port" onClick={() => void apply((show) => addPortEdits(show, controller.id))}>
            <Plus size={14} /> Port
          </Button>
          <Button
            variant="danger"
            aria-label={`Delete ${controller.name}`}
            title={wired > 0 ? `Delete this controller (unwires ${plural(wired, "prop")}; Undo brings it back)` : "Delete this controller"}
            onClick={() => void apply([{ type: "removeController", id: controller.id }])}
          >
            <Trash2 size={16} />
          </Button>
        </div>
      </header>
      {editing && <ControllerEditForm controller={controller} onDone={() => setEditing(false)} />}
      {!collapsed &&
        (controller.ports.length === 0 ? (
          <p className="border-t border-neutral-200 px-3 py-3 text-sm text-neutral-500 dark:border-neutral-800">
            No ports yet. Add one with + Port.
          </p>
        ) : (
          <ul>
            {controller.ports.map((port, i) => (
              <PortRow key={`${port.number}-${i}`} controller={controller} port={port} at={i} data={data} />
            ))}
          </ul>
        ))}
    </section>
  );
}
