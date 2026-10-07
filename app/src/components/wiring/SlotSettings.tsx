import { Unplug, X } from "lucide-react";
import { type KeyboardEvent, useEffect, useRef } from "react";
import type { PortSlot } from "../../api/types";
import { thousands } from "../../lib/format";
import { type SlotRef, findPort, moveSlotEdits, unwireEdits, updateSlotEdits } from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { useWiring } from "../../state/wiring";
import { NumberField } from "../layout/PropertiesPanel";
import { Button, Select } from "../ui";
import { type WiringData, focusAfterUnwire } from "./ControllerCard";
import { OptionalNumberField } from "./fields";

/** Most null pixels one slot may have (the engine's limit). */
const MAX_NULL_PIXELS = 1000;

/**
 * Settings for the selected chip: which pixels it carries, which end the data enters, and
 * overrides. `selected` is the slot by identity, found where it is now (the Wiring screen closes
 * this when it's gone); every change finds it again when its turn comes.
 */
export function SlotSettings({ selected, data }: { selected: SlotRef; data: WiringData }) {
  const apply = useApp((s) => s.apply);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const controller = data.show.controllers.find((c) => c.id === selected.controller);
  const port = findPort(data.show, selected);
  const slot = port?.slots[selected.index];
  const opened = `${selected.controller}:${selected.port}:${selected.at}:${selected.prop}`;
  // Opening (or switching to another chip) moves focus here, so the keyboard follows.
  useEffect(() => {
    headingRef.current?.focus();
  }, [opened]);
  if (!controller || !port || !slot) return null;
  const prop = data.propById.get(slot.prop);
  const nodes = data.nodes.get(slot.prop) ?? 0;
  const name = prop?.name ?? "Missing prop";
  const change = (patch: Partial<PortSlot>) =>
    void apply((show) => updateSlotEdits(show, selected, (s) => ({ ...s, ...patch }))).then((ok) => {
      // A new pixel range is part of what identifies the slot: keep following it.
      const now = useWiring.getState().selected;
      if (ok && patch.segment !== undefined && now?.prop === selected.prop) useWiring.getState().select({ ...now, segment: patch.segment });
    });
  /** Closes, giving focus back to the chip. */
  const close = () => useWiring.setState({ selected: null, focus: selected });
  const segment = slot.segment;
  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    close();
  };

  return (
    <section aria-label={`${name} settings`} onKeyDown={onKeyDown} className="scroll-mt-4 rounded-lg border border-accent-500/50 bg-white p-3 dark:bg-neutral-900">
      <div className="mb-3 flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <h2 ref={headingRef} tabIndex={-1} className="truncate text-sm font-semibold focus:outline-none">
            {name}
          </h2>
          <p className="text-xs text-neutral-500">
            {controller.name} · Port {port.number} · {ordinal(selected.index + 1)} on the port
          </p>
        </div>
        <button type="button" aria-label="Close settings" data-tip="Close settings" onClick={close} className="rounded p-1 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800">
          <X size={14} />
        </button>
      </div>

      <div className="flex flex-col gap-3">
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-neutral-500 dark:text-neutral-400">Port</span>
          <Select
            value={`${controller.id}:${controller.ports.indexOf(port)}`}
            onChange={(e) => {
              const [id, at] = e.target.value.split(":");
              const target = data.show.controllers.find((c) => c.id === id)?.ports[Number(at)];
              if (!target) return;
              const to = { controller: id, port: target.number, at: Number(at), index: target.slots.length };
              // The panel follows the slot to its new port.
              void apply((show) => moveSlotEdits(show, selected, to)).then((ok) => ok && useWiring.getState().select({ ...selected, ...to }));
            }}
          >
            {data.show.controllers.map((c) => (
              <optgroup key={c.id} label={c.name}>
                {c.ports.map((p, i) => (
                  <option key={i} value={`${c.id}:${i}`}>
                    {c.name} · Port {p.number}
                  </option>
                ))}
              </optgroup>
            ))}
          </Select>
        </label>

        <label className="flex items-start gap-2 text-sm">
          <input type="checkbox" className="mt-0.5" checked={slot.reverse} onChange={(e) => change({ reverse: e.target.checked })} />
          <span>
            Starts at the other end
            <span className="block text-xs text-neutral-500">Tick this when the wire plugs into the prop's far end. The preview's green dot shows where the data enters.</span>
          </span>
        </label>

        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-xs text-neutral-500 dark:text-neutral-400">Pixels on this port</legend>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={segment === null} onChange={(e) => change({ segment: e.target.checked ? null : { start: 0, end: nodes } })} />
            The whole prop ({thousands(nodes)} pixels)
          </label>
          {segment && (
            <div className="grid grid-cols-2 gap-2">
              <NumberField
                label="From pixel"
                value={segment.start + 1}
                min={1}
                max={segment.end}
                integer
                onCommit={(n) => change({ segment: { start: n - 1, end: segment.end } })}
              />
              <NumberField
                label="To pixel"
                value={segment.end}
                min={segment.start + 1}
                max={nodes}
                integer
                onCommit={(n) => change({ segment: { start: segment.start, end: n } })}
              />
            </div>
          )}
        </fieldset>

        <div className="grid grid-cols-2 gap-2">
          <NumberField
            label="Empty pixels before it"
            hint="Pixels on the string before this prop that stay dark, such as a lead to reach it."
            value={slot.nullPixels}
            min={0}
            max={MAX_NULL_PIXELS}
            integer
            onCommit={(nullPixels) => change({ nullPixels })}
          />
          <OptionalNumberField
            label="Smart receiver"
            hint="Only for ports that feed smart receivers: which receiver this prop hangs off (1 is A, 2 is B, …). Each receiver gets the port's whole pixel limit."
            value={slot.smartReceiver}
            min={0}
            max={255}
            integer
            placeholder="None"
            onCommit={(smartReceiver) => change({ smartReceiver })}
          />
          <OptionalNumberField
            label="Brightness (%)"
            value={slot.brightness}
            min={0}
            max={100}
            integer
            placeholder={`Port's (${port.brightness}%)`}
            onCommit={(brightness) => change({ brightness })}
          />
          <OptionalNumberField
            label="Gamma"
            hint="2.2 suits most pixels; 1 leaves colors as they are."
            value={slot.gamma}
            min={0.1}
            max={5}
            placeholder={`Port's (${port.gamma})`}
            onCommit={(gamma) => change({ gamma })}
          />
        </div>

        <div className="flex justify-between gap-2">
          <Button
            variant="danger"
            onClick={() => {
              const then = focusAfterUnwire(port, selected);
              useWiring.setState({ selected: null });
              void apply((show) => unwireEdits(show, selected)).then((ok) => ok && useWiring.setState({ focus: then }));
            }}
          >
            <Unplug size={14} /> Unwire
          </Button>
          <Button onClick={close}>Done</Button>
        </div>
      </div>
    </section>
  );
}

function ordinal(n: number): string {
  const suffix = n % 100 >= 11 && n % 100 <= 13 ? "th" : ({ 1: "st", 2: "nd", 3: "rd" } as Record<number, string>)[n % 10] ?? "th";
  return `${n}${suffix}`;
}
