import { Plus, X } from "lucide-react";
import type { DmxChannel, DmxFixture, DmxMotor, DmxShutter, DmxShutterRange, DmxSlot, Prop } from "../../api/types";
import { DEFAULT_BEAM, DEFAULT_ZOOM, USUAL_SHUTTER, isHead } from "../../lib/dmx";
import { type Axis, axes, axisValues, limits, travel } from "../../lib/motion";
import { Input, More, Select } from "../ui";
import { NumberField } from "./PropertiesPanel";

const tidy = (v: number) => Math.round(v * 100) / 100;

/** A fixture's channels with one motor changed; an axis resting at Home before stays at Home. */
function withMotor(channels: DmxChannel[], axis: Axis, motor: DmxMotor): DmxChannel[] {
  const out = channels.map((c) => ({ ...c }));
  const [coarse, fine] = axisValues(axis, 0);
  const atHome = (channels[axis.coarse].value ?? 0) === coarse && (axis.fine === null || (channels[axis.fine].value ?? 0) === fine);
  out[axis.coarse].motor = motor;
  if (atHome) {
    const [c, f] = axisValues({ ...axis, motor }, 0);
    out[axis.coarse].value = c;
    if (axis.fine !== null && f !== null) out[axis.fine].value = f;
  }
  return out;
}

/** One motor's settings: how far it turns, where Home is, its limits either side, and whether it's reversed. */
function MotorFields({ name, axis, low, high, onChange }: { name: string; axis: Axis; low: string; high: string; onChange: (motor: DmxMotor) => void }) {
  const m = axis.motor;
  const [lo, hi] = limits(m);
  const [travelLo, travelHi] = travel(m);
  const set = (patch: Partial<DmxMotor>) => {
    const next: DmxMotor = { ...m, ...patch };
    // Limits that reach the end of the travel are no limit at all.
    const [tLo, tHi] = travel(next);
    if (next.minDeg !== undefined && next.minDeg <= tLo) delete next.minDeg;
    if (next.maxDeg !== undefined && next.maxDeg >= tHi) delete next.maxDeg;
    if (!next.reverse) delete next.reverse;
    onChange(next);
  };
  return (
    <fieldset className="mb-2 grid grid-cols-2 gap-2">
      <legend className="mb-1 text-xs font-medium text-neutral-700 dark:text-neutral-300">
        {name} <span className="font-normal text-neutral-500">· {axis.fine === null ? "8-bit" : "16-bit"}</span>
      </legend>
      <NumberField label="Turns (°)" hint="How many degrees it turns from value 0 to its highest (the fixture's manual says)" value={m.rangeDeg} min={1} max={1080} onCommit={(rangeDeg) => set({ rangeDeg, homeDeg: Math.min(m.homeDeg, rangeDeg) })} />
      <NumberField label="Home at (°)" hint="Where Home is (facing forward, pointing up), in degrees from value 0" value={m.homeDeg} min={0} max={m.rangeDeg} onCommit={(homeDeg) => set({ homeDeg })} />
      <NumberField label={`${low} limit (°)`} hint={`How far it may turn ${low.toLowerCase()} from Home`} value={tidy(lo)} min={Math.min(travelLo, 0)} max={0} onCommit={(minDeg) => set({ minDeg })} />
      <NumberField label={`${high} limit (°)`} hint={`How far it may turn ${high.toLowerCase()} from Home`} value={tidy(hi)} min={0} max={Math.max(travelHi, 0)} onCommit={(maxDeg) => set({ maxDeg })} />
      <label className="col-span-2 flex items-center gap-2 text-xs" title="It turns the other way as its value rises">
        <input type="checkbox" checked={m.reverse ?? false} onChange={(e) => set({ reverse: e.target.checked })} />
        Reversed
      </label>
    </fieldset>
  );
}

/**
 * A moving head's or servos' movement: each motor's settings; for a head, whether it hangs upside
 * down and its beam as the previews draw it.
 */
export function MotionSection({ prop, fixture, update }: { prop: Prop; fixture: DmxFixture; update: (change: (p: Prop) => Prop) => void }) {
  const found = axes(fixture.channels);
  if (found.length === 0) return null;
  const head = isHead(fixture);
  const setChannels = (channels: DmxChannel[]) => update((p) => (p.dmx ? { ...p, dmx: { ...p.dmx, channels } } : p));
  const hung = Math.abs(((prop.transform.rotationDeg.z % 360) + 360) % 360 - 180) < 0.5;
  const beam = fixture.beam ?? DEFAULT_BEAM;
  const servos = found.filter((a) => a.role === "servo");
  const zoomAt = fixture.channels.findIndex((c) => c.role === "zoom");
  const zoom = zoomAt < 0 ? null : (fixture.channels[zoomAt].zoom ?? DEFAULT_ZOOM);
  const setZoom = (patch: Partial<typeof DEFAULT_ZOOM>) =>
    update((p) => (p.dmx ? { ...p, dmx: { ...p.dmx, channels: p.dmx.channels.map((c, k) => (k === zoomAt ? { ...c, zoom: { ...(zoom ?? DEFAULT_ZOOM), ...patch } } : c)) } } : p));
  return (
    <More id={`dmx-motion-${prop.id}`} label={head ? "Pan and tilt" : `Servos (${servos.length})`}>
      {found.map((axis) => (
        <MotorFields
          key={axis.coarse}
          name={axis.role === "pan" ? "Pan" : axis.role === "tilt" ? "Tilt" : fixture.channels[axis.coarse].name || "Servo"}
          axis={axis}
          low={axis.role === "pan" ? "Left" : axis.role === "tilt" ? "Back" : "Lower"}
          high={axis.role === "pan" ? "Right" : axis.role === "tilt" ? "Forward" : "Upper"}
          onChange={(motor) => setChannels(withMotor(fixture.channels, axis, motor))}
        />
      ))}
      {head && (
        <>
          <label className="mb-2 flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={hung}
              onChange={(e) => {
                // Read now: the edit is made when its turn comes, after the box has been put back.
                const z = e.target.checked ? 180 : 0;
                update((p) => ({ ...p, transform: { ...p.transform, rotationDeg: { ...p.transform.rotationDeg, z } } }));
              }}
            />
            Hung upside down
          </label>
          <div className="grid grid-cols-2 gap-2">
            {zoom ? (
              <>
                <NumberField
                  label="Zoom at 0 (°)"
                  hint="How wide its beam spreads with its zoom channel at 0 (the fixture's manual says)"
                  value={zoom.fromDeg}
                  min={1}
                  max={120}
                  onCommit={(fromDeg) => setZoom({ fromDeg })}
                />
                <NumberField
                  label="Zoom at 255 (°)"
                  hint="How wide its beam spreads with its zoom channel at 255"
                  value={zoom.toDeg}
                  min={1}
                  max={120}
                  onCommit={(toDeg) => setZoom({ toDeg })}
                />
              </>
            ) : (
              <NumberField
                label="Beam width (°)"
                hint="How wide its beam spreads (the fixture's beam angle)"
                value={beam.angleDeg}
                min={1}
                max={120}
                onCommit={(angleDeg) => update((p) => (p.dmx ? { ...p, dmx: { ...p.dmx, beam: { ...beam, angleDeg } } } : p))}
              />
            )}
            <NumberField
              label="Beam drawn"
              hint="How far the previews draw its beam, in layout units"
              value={beam.length}
              min={0.5}
              max={100}
              onCommit={(length) => update((p) => (p.dmx ? { ...p, dmx: { ...p.dmx, beam: { ...beam, length } } } : p))}
            />
          </div>
        </>
      )}
      <p className="mt-1 text-xs text-neutral-500">
        Positions are in degrees from Home. Where it rests is what its {head ? "pan and tilt" : "servo"} channels hold; a DMX effect moves it.
      </p>
    </More>
  );
}

const HEX = /^#[0-9a-f]{6}$/i;

const SHUTTER_LABELS: Record<DmxShutter, string> = { closed: "Closed", open: "Open", strobe: "Strobe, slow to fast" };

/**
 * What a strobe channel's values do: ranges from a value up to the next, each closed, open or a
 * strobe from slow to fast, so the previews show the light as the fixture would. Without its own,
 * the usual: open low, then a strobe.
 */
export function ShutterRanges({ prop, index, channel, onChange }: { prop: Prop; index: number; channel: DmxChannel; onChange: (ranges: DmxShutterRange[]) => void }) {
  const own = (channel.shutter?.length ?? 0) > 0;
  const ranges = own ? channel.shutter! : USUAL_SHUTTER;
  const set = (i: number, patch: Partial<DmxShutterRange>) => {
    const next = ranges.map((r, k) => (k === i ? { ...r, ...patch } : r));
    onChange([...next].sort((a, b) => a.from - b.from));
  };
  const name = channel.name || "Strobe";
  return (
    <More id={`dmx-shutter-${prop.id}-${index}`} label={`${name}: what its values do`}>
      <ol className="flex flex-col gap-1" aria-label={`${name} ranges`}>
        {ranges.map((range, i) => (
          <li key={`${i}-${range.from}`} className="flex items-center gap-1 text-xs">
            <span className="text-neutral-500">From</span>
            <Input
              aria-label={`Range ${i + 1} from`}
              title="The first value of this range (0-255); it runs up to where the next starts"
              className="w-12 tabular-nums"
              defaultValue={String(range.from)}
              onBlur={(e) => {
                const n = Number(e.target.value);
                if (Number.isInteger(n) && n >= 0 && n <= 255 && n !== range.from && !ranges.some((r) => r.from === n)) set(i, { from: n });
              }}
            />
            <Select aria-label={`Range ${i + 1} does`} className="min-w-0 flex-1" value={range.does} onChange={(e) => set(i, { does: e.target.value as DmxShutter })}>
              {(Object.keys(SHUTTER_LABELS) as DmxShutter[]).map((d) => (
                <option key={d} value={d}>
                  {SHUTTER_LABELS[d]}
                </option>
              ))}
            </Select>
            <button
              type="button"
              aria-label={`Remove range ${i + 1}`}
              className="rounded p-0.5 text-neutral-500 hover:bg-neutral-100 disabled:opacity-40 dark:hover:bg-neutral-800"
              disabled={ranges.length <= 1}
              onClick={() => onChange(ranges.filter((_, k) => k !== i))}
            >
              <X size={12} />
            </button>
          </li>
        ))}
      </ol>
      <div className="mt-1 flex gap-3">
        {ranges.length < 32 && ranges[ranges.length - 1].from < 255 && (
          <button
            type="button"
            className="flex items-center gap-1 text-xs text-violet-700 dark:text-violet-300"
            onClick={() => {
              const last = ranges[ranges.length - 1].from;
              onChange([...ranges, { from: Math.min(255, last + Math.max(1, Math.floor((256 - last) / 2))), does: "open" }]);
            }}
          >
            <Plus size={12} aria-hidden /> Add a range
          </button>
        )}
        {own && (
          <button type="button" className="text-xs text-violet-700 dark:text-violet-300" onClick={() => onChange([])}>
            Use the usual
          </button>
        )}
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        {own ? "From the fixture's manual: each range runs up to where the next starts." : "The usual: open from 0 to 9, then a strobe from slow (10) to fast (255). Change a range to set the fixture's own from its manual."} The previews show a closed shutter dark and a strobe flashing.
      </p>
    </More>
  );
}

/** A colour wheel's colours, each with the value that selects it, so colours drawn on the head turn the wheel. */
export function WheelSlots({ prop, index, channel, onChange }: { prop: Prop; index: number; channel: DmxChannel; onChange: (slots: DmxSlot[]) => void }) {
  const slots = channel.slots ?? [];
  const set = (i: number, patch: Partial<DmxSlot>) => onChange(slots.map((s, k) => (k === i ? { ...s, ...patch } : s)));
  return (
    <More id={`dmx-wheel-${prop.id}-${index}`} label={`${channel.name || "Color wheel"}: its colors (${slots.length})`}>
      <ol className="flex flex-col gap-1" aria-label={`${channel.name || "Color wheel"} colors`}>
        {slots.map((slot, i) => (
          <li key={i} className="flex items-center gap-1 text-xs">
            <Input
              aria-label={`Color ${i + 1} value`}
              title="The value that turns the wheel to this color (0-255)"
              className="w-12 tabular-nums"
              defaultValue={String(slot.value)}
              onBlur={(e) => {
                const n = Number(e.target.value);
                if (Number.isInteger(n) && n >= 0 && n <= 255 && n !== slot.value) set(i, { value: n });
              }}
            />
            <input
              type="color"
              aria-label={`Color ${i + 1}`}
              className="h-6 w-8 cursor-pointer rounded border border-neutral-300 bg-transparent dark:border-neutral-700"
              value={slot.color && HEX.test(slot.color) ? slot.color.toLowerCase() : "#ffffff"}
              onChange={(e) => set(i, { color: e.target.value })}
            />
            <Input aria-label={`Color ${i + 1} name`} className="min-w-0 flex-1" placeholder="Name" defaultValue={slot.name ?? ""} onBlur={(e) => e.target.value !== (slot.name ?? "") && set(i, { name: e.target.value || undefined })} />
            <button type="button" aria-label={`Remove color ${i + 1}`} className="rounded p-0.5 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800" onClick={() => onChange(slots.filter((_, k) => k !== i))}>
              <X size={12} />
            </button>
          </li>
        ))}
      </ol>
      {slots.length < 64 && (
        <button
          type="button"
          className="mt-1 flex items-center gap-1 text-xs text-violet-700 dark:text-violet-300"
          onClick={() => onChange([...slots, { value: Math.min(255, (slots[slots.length - 1]?.value ?? -16) + 16), color: "#ffffff" }])}
        >
          <Plus size={12} aria-hidden /> Add a color
        </button>
      )}
      <p className="mt-1 text-xs text-neutral-500">With its colors listed, the wheel turns to the one nearest the color drawn on the head, and the dimmer carries the brightness.</p>
    </More>
  );
}
