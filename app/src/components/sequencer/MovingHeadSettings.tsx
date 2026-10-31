import { ChevronDown, ChevronRight, House } from "lucide-react";
import { type KeyboardEvent, type PointerEvent, useId, useRef, useState } from "react";
import type { Effect, EffectSetting } from "../../api/sequence";
import type { DmxFixture } from "../../api/types";
import { curveValue } from "../../lib/curves";
import { isHead } from "../../lib/dmx";
import { headOffset, headSettings, servoSettings } from "../../lib/headEffects";
import { axes, limits } from "../../lib/motion";
import { useSequencer } from "../../state/sequencer";
import { More } from "../ui";
import { type CurveControl, SettingControl, useLiveValue } from "./effectControls";

// The Moving Head and Servo effects' settings, in groups: where it points, how it moves, how a
// group's heads (or servos) fan out, and its light. Every number keeps its curve button.

/** How far a pad reaches each way, in degrees from Home. */
export interface Reach {
  pan: [number, number];
  tilt: [number, number];
}

/** The pad's reach when the row has no head to take it from. */
const VIEW: Reach = { pan: [-180, 180], tilt: [-90, 90] };

/** How far the row's first head can point (its limits, within the settings' ranges), or a usual view. */
export function headReach(fixture: DmxFixture | undefined): Reach {
  const motors = fixture ? axes(fixture.channels) : [];
  const of = (role: "pan" | "tilt", range: [number, number]): [number, number] => {
    const axis = motors.find((a) => a.role === role);
    if (!axis) return VIEW[role];
    const [lo, hi] = limits(axis.motor);
    const out: [number, number] = [Math.max(range[0], Math.round(lo)), Math.min(range[1], Math.round(hi))];
    return out[1] - out[0] >= 10 ? out : VIEW[role];
  };
  return { pan: of("pan", [-360, 360]), tilt: of("tilt", [-180, 180]) };
}

/** A group of settings under a heading that folds it away. */
function Group({ title, open: initially = true, folded, children }: { title: string; open?: boolean; folded?: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(initially);
  const region = useId();
  return (
    <section aria-label={title} className="mt-4 flex flex-col gap-2.5">
      <h3 className="text-xs font-semibold tracking-wide text-neutral-500 uppercase">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={region}
          className="flex w-full items-center gap-1 rounded text-left uppercase hover:text-neutral-800 dark:hover:text-neutral-200"
          onClick={() => setOpen(!open)}
        >
          {open ? <ChevronDown size={13} aria-hidden /> : <ChevronRight size={13} aria-hidden />}
          {title}
          {!open && folded && (
            <>
              {" "}
              <span className="ml-auto font-normal tracking-normal normal-case">{folded}</span>
            </>
          )}
        </button>
      </h3>
      <div id={region} hidden={!open} className="flex flex-col gap-2.5">
        {open && children}
      </div>
    </section>
  );
}

const clamp = (v: number, [lo, hi]: [number, number]) => Math.min(hi, Math.max(lo, v));

/**
 * A square to point a head with: across is pan, up is tilt, over how far the head can point. The
 * dot is where it points (the middle of its movement, drawn faintly round it), the cross is
 * Home. A drag sets both at once, as one undo step; the arrow keys nudge it (Shift: 10°), and
 * Home goes Home. When pan or tilt changes over the effect, it shows where the curves have it at
 * the playhead and leaves the pointing to them.
 */
export function PanTiltPad({
  pan,
  tilt,
  reach,
  following,
  path = [],
  onPoint,
}: {
  pan: number;
  tilt: number;
  reach: Reach;
  /** Pan or tilt changes over the effect: the pad only shows where they are. */
  following: boolean;
  /** The movement round the dot, as offsets in degrees, drawn faintly. */
  path?: [number, number][];
  onPoint: (pan: number, tilt: number, gesture: string) => Promise<unknown>;
}) {
  const live = useLiveValue<[number, number]>(([p, t], gesture) => onPoint(p, t, gesture));
  const [p, t] = live.live ?? [pan, tilt];
  const dragging = useRef(false);
  const x = (v: number) => ((clamp(v, reach.pan) - reach.pan[0]) / (reach.pan[1] - reach.pan[0])) * 100;
  const y = (v: number) => ((reach.tilt[1] - clamp(v, reach.tilt)) / (reach.tilt[1] - reach.tilt[0])) * 100;
  const at = (e: PointerEvent<HTMLDivElement>): [number, number] => {
    const r = e.currentTarget.getBoundingClientRect();
    const fx = r.width > 0 ? Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) : 0.5;
    const fy = r.height > 0 ? Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)) : 0.5;
    return [Math.round(reach.pan[0] + fx * (reach.pan[1] - reach.pan[0])), Math.round(reach.tilt[1] - fy * (reach.tilt[1] - reach.tilt[0]))];
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 10 : 1;
    const moves: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
    if (following || e.metaKey || e.ctrlKey || !(e.key in moves || e.key === "Home")) return;
    e.preventDefault();
    if (e.key === "Home") live.push([clamp(0, reach.pan), clamp(0, reach.tilt)]);
    else live.push([clamp(Math.round(p) + moves[e.key][0], reach.pan), clamp(Math.round(t) + moves[e.key][1], reach.tilt)]);
  };
  const line = path.map(([dp, dt]) => `${x(p + dp).toFixed(2)},${y(t + dt).toFixed(2)}`).join(" ");
  return (
    <div
      role="slider"
      tabIndex={0}
      aria-label="Pan and tilt"
      aria-valuemin={reach.pan[0]}
      aria-valuemax={reach.pan[1]}
      aria-valuenow={Math.round(p)}
      aria-valuetext={`Pan ${Math.round(p)}°, tilt ${Math.round(t)}°`}
      aria-disabled={following || undefined}
      title={following ? "Pan or tilt changes over the effect" : "Drag to point it: across is pan, up is tilt. Arrow keys nudge it (Shift: 10°)."}
      className={`relative size-32 shrink-0 touch-none overflow-hidden rounded-md border border-neutral-300 bg-neutral-50 outline-none select-none focus-visible:ring-2 focus-visible:ring-violet-500 dark:border-neutral-700 dark:bg-neutral-900 ${
        following ? "cursor-not-allowed opacity-60" : "cursor-crosshair"
      }`}
      onPointerDown={(e) => {
        if (following || e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.focus();
        e.currentTarget.setPointerCapture?.(e.pointerId);
        dragging.current = true;
        live.push(at(e));
      }}
      onPointerMove={(e) => {
        if (dragging.current) live.push(at(e));
      }}
      onPointerUp={() => {
        dragging.current = false;
        live.end();
      }}
      onPointerCancel={() => {
        dragging.current = false;
        live.end();
      }}
      onKeyDown={onKeyDown}
      onKeyUp={live.end}
      onBlur={live.end}
    >
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="absolute inset-0 h-full w-full" aria-hidden>
        <line x1={x(0)} y1="0" x2={x(0)} y2="100" className="stroke-neutral-300 dark:stroke-neutral-700" vectorEffect="non-scaling-stroke" />
        <line x1="0" y1={y(0)} x2="100" y2={y(0)} className="stroke-neutral-300 dark:stroke-neutral-700" vectorEffect="non-scaling-stroke" />
        {line && <polyline points={line} fill="none" className="stroke-violet-500/80" strokeWidth="2" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />}
      </svg>
      <span aria-hidden className="pointer-events-none absolute top-0.5 left-1 text-[10px] text-neutral-400">
        Tilt
      </span>
      <span aria-hidden className="pointer-events-none absolute right-1 bottom-0.5 text-[10px] text-neutral-400">
        Pan
      </span>
      <span
        aria-hidden
        className="pointer-events-none absolute size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white bg-violet-600 shadow dark:border-neutral-950"
        style={{ left: `${x(p)}%`, top: `${y(t)}%` }}
      />
    </div>
  );
}

/** What a Moving Head or Servo panel is given by the effect settings. */
interface PanelProps {
  effect: Effect;
  /** The kind's settings, from the catalog. */
  settings: EffectSetting[];
  /** The fixtures the row plays on. */
  fixtures: DmxFixture[];
  onParam: (key: string, value: unknown, gesture?: string) => Promise<boolean>;
  animate: (key: string) => CurveControl;
}

/** The catalog's control for setting `key` of `effect` (none when the catalog has no such setting). */
function controls({ effect, settings, onParam, animate }: PanelProps) {
  const params = effect.params as Record<string, unknown>;
  return (key: string) => {
    const setting = settings.find((s) => s.key === key);
    if (!setting) return null;
    return (
      <SettingControl
        key={`${effect.id}:${key}`}
        setting={setting}
        value={params[key]}
        onChange={(value, gesture) => onParam(key, value, gesture)}
        faces={[]}
        tracks={[]}
        animate={setting.type === "number" || setting.type === "int" ? animate(key) : undefined}
      />
    );
  };
}

/** Whether any of `keys` is set away from its default, or changes over the effect. */
function anySet(effect: Effect, keys: string[], defaults: Record<string, unknown>): boolean {
  const params = effect.params as Record<string, unknown>;
  return keys.some((k) => effect.curves?.[k] !== undefined || (params[k] !== undefined && params[k] !== defaults[k]));
}

/**
 * A Moving Head effect's settings: where it points (with a pad to point it by), how it moves, how
 * a group's heads fan out, and its light.
 */
export function MovingHeadSettings(props: PanelProps & { onPoint: (pan: number, tilt: number, gesture?: string) => Promise<boolean> }) {
  const { effect, fixtures, onPoint } = props;
  const control = controls(props);
  const s = headSettings(effect);
  const heads = fixtures.filter(isHead);
  const curves = effect.curves ?? {};
  const following = curves.pan !== undefined || curves.tilt !== undefined;
  // Where the curves have it at the playhead, while it follows them.
  const playheadMs = useSequencer((st) => (following ? st.playheadMs : 0));
  const progress = Math.min(1, Math.max(0, (playheadMs - effect.startMs) / Math.max(1, effect.endMs - effect.startMs)));
  const pan = curves.pan ? curveValue(curves.pan, progress) : s.pan;
  const tilt = curves.tilt ? curveValue(curves.tilt, progress) : s.tilt;
  const path: [number, number][] =
    s.pattern === "still"
      ? []
      : Array.from({ length: 97 }, (_, k) => headOffset(s.pattern, (k / 96) * (s.pattern === "wander" ? 4 : 1), s.width, s.height, s.seed, 0));
  const fanKeys = ["fanPan", "fanTilt", "delay", "fans", "mirror"];
  const fanned = heads.length > 1 || anySet(effect, fanKeys, { fanPan: 0, fanTilt: 0, delay: 0, fans: 1, mirror: false });
  return (
    <>
      <Group title="Position">
        <div className="flex items-start gap-3">
          <PanTiltPad pan={pan} tilt={tilt} reach={headReach(heads[0])} following={following} path={path} onPoint={onPoint} />
          <div className="flex min-w-0 flex-col gap-2 text-xs text-neutral-500">
            <p>{following ? "Pan or tilt changes over the effect: the dot shows where, at the playhead." : "Drag to point it, or nudge it with the arrow keys."}</p>
            <button
              type="button"
              disabled={following || (s.pan === 0 && s.tilt === 0)}
              title="Point it at Home: facing the street, straight up"
              className="flex w-fit items-center gap-1 rounded-md border border-neutral-300 px-2 py-1 text-xs text-neutral-700 hover:bg-neutral-100 disabled:opacity-40 disabled:hover:bg-transparent dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-800"
              onClick={() => void onPoint(0, 0)}
            >
              <House size={12} aria-hidden /> Home
            </button>
          </div>
        </div>
        {control("pan")}
        {control("tilt")}
      </Group>
      <Group title="Movement">
        {control("pattern")}
        {s.pattern !== "still" && (
          <>
            {control("width")}
            {control("height")}
            {control("speed")}
            {s.pattern === "wander" && control("seed")}
          </>
        )}
      </Group>
      <Group title="Fan" open={fanned} folded="for a group of heads">
        {control("fanPan")}
        {control("fanTilt")}
        {control("delay")}
        <More key={`${effect.id}:fans`} id="head-fans" label="More: fans and mirror" forceOpen={anySet(effect, ["fans", "mirror"], { fans: 1, mirror: false })}>
          <div className="flex flex-col gap-2.5">
            {control("fans")}
            {control("mirror")}
          </div>
        </More>
      </Group>
      <Group title="Light">
        {control("color")}
        {s.color === "drawn" ? (
          <p className="text-xs text-neutral-500">The layers below light the head: this effect only moves it.</p>
        ) : (
          <>
            {control("dimmer")}
            {control("strobe")}
          </>
        )}
      </Group>
    </>
  );
}

/** A Servo effect's settings: where it turns to, how it moves, and how several fan out. */
export function ServoSettings(props: PanelProps) {
  const { effect, fixtures } = props;
  const control = controls(props);
  const s = servoSettings(effect);
  const servos = fixtures.reduce((n, f) => n + axes(f.channels).filter((a) => a.role === "servo").length, 0);
  const fanned = servos > 1 || anySet(effect, ["fan", "delay", "servo"], { fan: 0, delay: 0, servo: 0 });
  return (
    <>
      <Group title="Position">{control("position")}</Group>
      <Group title="Movement">
        {control("pattern")}
        {s.pattern !== "still" && (
          <>
            {control("size")}
            {control("speed")}
            {s.pattern === "steps" && control("steps")}
          </>
        )}
      </Group>
      <Group title="Fan" open={fanned} folded="for several servos">
        {control("fan")}
        {control("delay")}
        <More key={`${effect.id}:servo`} id="servo-which" label="More: which servo" forceOpen={anySet(effect, ["servo"], { servo: 0 })}>
          <div className="flex flex-col gap-1">
            {control("servo")}
            <p className="text-xs text-neutral-500">0 turns every servo of each fixture.</p>
          </div>
        </More>
      </Group>
    </>
  );
}
