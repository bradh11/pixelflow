import { useRef } from "react";
import type { Curve, EffectSetting } from "../../api/sequence";
import { MAX_CYCLES, MIN_CYCLES, SHAPE_CHOICES, type ShapeChoice, curveLevel, followsSomething, shapeChoice, tidyPoints, withShape } from "../../lib/curves";
import { FIELD, NumberDraft, useLiveValue } from "./effectControls";

type NumberSpec = Extract<EffectSetting, { type: "number" | "int" }>;

/**
 * A setting's curve over its effect: the shape, the two values it goes between, and a small graph
 * of it. On a custom curve the points can be dragged; clicking the graph adds one, and
 * double-clicking a point (or Delete on it) removes it. A drag is one undo step.
 */
export function CurveEditor({
  id,
  setting,
  curve,
  onChange,
}: {
  /** For the shape list, so the setting's label names it. */
  id: string;
  setting: NumberSpec;
  curve: Curve;
  onChange: (curve: Curve, gesture?: string) => Promise<boolean>;
}) {
  const drag = useLiveValue<Curve>((c, gesture) => onChange(c, gesture));
  const shown = drag.live ?? curve;
  const fit = (v: number) => Math.min(setting.max, Math.max(setting.min, setting.type === "int" ? Math.round(v) : v));
  const repeats = shown.shape === "sine" || shown.shape === "square" || shown.shape === "saw";
  const value = (v: number) => (setting.type === "int" ? Math.round(v) : Number(v.toFixed(3)));
  // What a music or timing curve needs besides its two values.
  const extras: { key: "gain" | "trigger" | "fade"; label: string; min: number; max: number }[] =
    shown.shape === "music" || shown.shape === "invertedMusic"
      ? [{ key: "gain", label: "Gain %", min: -100, max: 100 }]
      : shown.shape === "musicTrigger"
        ? [
            { key: "trigger", label: "Trigger %", min: 0, max: 100 },
            { key: "fade", label: "Fade frames", min: 0, max: 1000 },
          ]
        : shown.shape === "timingFade"
          ? [{ key: "fade", label: "Fade frames", min: 0, max: 1000 }]
          : shown.shape === "timingFadeSpan"
            ? [{ key: "fade", label: "Fade %", min: 0, max: 100 }]
            : [];
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-1.5">
        <select id={id} className={`${FIELD} min-w-0 flex-1`} value={shapeChoice(shown)} onChange={(e) => void onChange(withShape(shown, e.target.value as ShapeChoice))}>
          {SHAPE_CHOICES.filter((c) => !c.onlyWhenSet || c.value === shown.shape).map((c) => (
            <option key={c.value} value={c.value}>
              {c.label}
            </option>
          ))}
        </select>
        {repeats && (
          <label className="flex items-center gap-1 text-xs text-neutral-500" title="Times it repeats over the effect">
            ×
            <NumberDraft
              aria-label={`${setting.label} repeats`}
              className={`${FIELD} w-14 tabular-nums`}
              min={MIN_CYCLES}
              max={MAX_CYCLES}
              step={0.5}
              value={shown.cycles ?? 1}
              onCommit={(v) => onChange({ ...shown, cycles: Math.min(MAX_CYCLES, Math.max(MIN_CYCLES, v)) })}
            />
          </label>
        )}
      </div>
      {followsSomething(shown) ? (
        extras.length > 0 && (
          <div className="grid grid-cols-2 gap-2">
            {extras.map((x) => (
              <label key={x.key} className="flex min-w-0 items-center gap-1 text-xs whitespace-nowrap text-neutral-500">
                {x.label}
                <NumberDraft
                  aria-label={`${setting.label} ${x.label.toLowerCase()}`}
                  className={`${FIELD} w-full min-w-0 tabular-nums`}
                  min={x.min}
                  max={x.max}
                  step={1}
                  value={shown[x.key] ?? 0}
                  onCommit={(v) => onChange({ ...shown, [x.key]: Math.min(x.max, Math.max(x.min, v)) })}
                />
              </label>
            ))}
          </div>
        )
      ) : (
        <CurveGraph label={setting.label} curve={shown} onDrag={drag.push} onDragEnd={drag.end} onEdit={(c) => void onChange(c)} />
      )}
      <div className="grid grid-cols-2 gap-2">
        {(["from", "to"] as const).map((end) => (
          <label key={end} className="flex min-w-0 items-center gap-1 text-xs text-neutral-500">
            {end === "from" ? "From" : "To"}
            <NumberDraft
              aria-label={`${setting.label} ${end === "from" ? "from" : "to"}`}
              className={`${FIELD} w-full min-w-0 tabular-nums`}
              min={setting.min}
              max={setting.max}
              step={setting.step}
              value={value(shown[end])}
              onCommit={(v) => onChange({ ...shown, [end]: fit(v) })}
            />
          </label>
        ))}
      </div>
    </div>
  );
}

/** The curve drawn across the effect (left to right), low values at the bottom. */
function CurveGraph({
  label,
  curve,
  onDrag,
  onDragEnd,
  onEdit,
}: {
  label: string;
  curve: Curve;
  onDrag: (curve: Curve) => void;
  onDragEnd: () => void;
  onEdit: (curve: Curve) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const dragging = useRef<number | null>(null);
  const custom = curve.shape === "custom";
  const points = curve.points ?? [];
  // Levels run bottom (from) to top (to); when `to` is the lower value, flip so up is higher.
  const flip = curve.to < curve.from;
  const y = (level: number) => (flip ? level : 1 - level) * 100;
  const path = custom
    ? [[0, points[0]?.[1] ?? 0], ...points, [1, points[points.length - 1]?.[1] ?? 0]].map(([t, l], i) => `${i ? "L" : "M"}${t * 100},${y(l)}`).join("")
    : Array.from({ length: 201 }, (_, i) => `${i ? "L" : "M"}${i / 2},${y(curveLevel(curve, i / 200))}`).join("");

  const at = (e: { clientX: number; clientY: number }): [number, number] => {
    const r = box.current?.getBoundingClientRect();
    if (!r || r.width === 0 || r.height === 0) return [0, 0];
    const t = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const up = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
    return [Math.round(t * 1000) / 1000, Math.round((flip ? 1 - up : up) * 1000) / 1000];
  };
  /** Point `i` moved to `[t, level]`, kept between its neighbours in time. */
  const moved = (i: number, [t, level]: [number, number]): Curve => {
    const lo = points[i - 1]?.[0] ?? 0;
    const hi = points[i + 1]?.[0] ?? 1;
    const next = points.map((p, k) => (k === i ? ([Math.min(hi, Math.max(lo, t)), level] as [number, number]) : p));
    return { ...curve, points: next };
  };
  const remove = (i: number) => {
    if (points.length > 2) onEdit({ ...curve, points: points.filter((_, k) => k !== i) });
  };

  return (
    <div
      ref={box}
      role={custom ? "group" : "img"}
      aria-label={custom ? `${label} curve points` : `${label} curve`}
      title={custom ? "Drag a point; click to add one, double-click one to remove it." : "Pick Custom to shape it by hand."}
      className={`relative h-12 rounded border border-neutral-200 bg-neutral-50 dark:border-neutral-800 dark:bg-neutral-900 ${custom ? "cursor-crosshair" : ""}`}
      onPointerDown={(e) => {
        if (!custom || e.target !== e.currentTarget) return;
        onEdit({ ...curve, points: tidyPoints([...points, at(e)]) });
      }}
    >
      <svg className="pointer-events-none absolute inset-0 h-full w-full" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden>
        <path d={path} fill="none" className="stroke-violet-600 dark:stroke-violet-400" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
      </svg>
      {custom &&
        points.map(([t, level], i) => (
          <button
            key={i}
            type="button"
            aria-label={`Point ${i + 1}: ${Math.round(t * 100)}% through, ${Math.round(level * 100)}% of the way from the first value to the second`}
            className="absolute h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 cursor-grab rounded-full border border-white bg-violet-600 shadow focus:ring-2 focus:ring-violet-400 focus:outline-none dark:border-neutral-900"
            style={{ left: `${t * 100}%`, top: `${y(level)}%` }}
            onPointerDown={(e) => {
              e.stopPropagation();
              e.currentTarget.setPointerCapture?.(e.pointerId);
              dragging.current = i;
            }}
            onPointerMove={(e) => {
              if (dragging.current === i) onDrag(moved(i, at(e)));
            }}
            onPointerUp={() => {
              dragging.current = null;
              onDragEnd();
            }}
            onDoubleClick={() => remove(i)}
            onKeyDown={(e) => {
              const step = e.shiftKey ? 0.1 : 0.02;
              const nudge: Record<string, [number, number]> = { ArrowUp: [0, step], ArrowDown: [0, -step], ArrowLeft: [-step, 0], ArrowRight: [step, 0] };
              if (e.key === "Delete" || e.key === "Backspace") {
                e.preventDefault();
                remove(i);
              } else if (nudge[e.key]) {
                e.preventDefault();
                const [dt, dl] = nudge[e.key];
                const up = flip ? -dl : dl;
                onEdit(moved(i, [t + dt, Math.min(1, Math.max(0, Math.round((level + up) * 1000) / 1000))]));
              }
            }}
          />
        ))}
    </div>
  );
}
