import { Plus, X } from "lucide-react";
import { useId, useRef, useState } from "react";
import type { Blend, Effect, EffectInfo, EffectSetting, Sequence, SequenceTarget, TimingTrack } from "../../api/sequence";
import type { Show } from "../../api/types";
import { memberProp } from "../../lib/shows";
import { facesOf, targetProp } from "../../lib/submodels";
import { newGesture } from "../../state/sequencer";

// The controls the effect settings panel is made of: setting fields from the catalog, colors,
// times, and the sections they sit in.
export const BLENDS: { value: Blend; label: string; help: string }[] = [
  { value: "normal", label: "Cover", help: "Covers the layers below where it's lit." },
  { value: "add", label: "Add light", help: "Adds its light to the layers below." },
  { value: "max", label: "Brighter of the two", help: "Keeps the brighter color, channel by channel." },
  { value: "multiply", label: "Tint", help: "Tints the layers below with its colors." },
];

export const MAX_COLORS = 32;

export const FIELD = "rounded-md border border-neutral-300 bg-white px-2 py-1 text-sm dark:border-neutral-700 dark:bg-neutral-950";

/** Shows numbers to the setting's step: 0.05 → 2 decimals. */
function decimals(step: number): number {
  const text = String(step);
  return text.includes(".") ? text.split(".")[1].length : 0;
}

/** `v` rounded to a whole millisecond and kept between `lo` and `hi`. */
export function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, Math.round(v)));
}

/** Builds a changed copy of the effect as it is when the change's turn comes; null changes nothing. */
export type Change = (next: (latest: Effect, doc: Sequence) => Effect | null, gesture?: string) => Promise<boolean>;

/** The faces a Faces effect on `target` can use: the faces of the props it lights, by name. */
export function faceNames(show: Show | undefined, target: SequenceTarget): string[] {
  const ids = "group" in target ? (show?.groups.find((g) => g.id === target.group)?.members ?? []).map(memberProp) : [targetProp(target)];
  const names = ids.flatMap((id) => {
    const prop = show?.props.find((p) => p.id === id);
    return prop ? facesOf(prop).map((r) => r.name) : [];
  });
  return [...new Set(names)];
}

export function Panel({ children }: { children: React.ReactNode }) {
  return (
    <aside aria-label="Effect settings" className="w-72 shrink-0 overflow-auto border-l border-neutral-200 p-3 dark:border-neutral-800">
      {children}
    </aside>
  );
}

export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section aria-label={title} className="mt-4 flex flex-col gap-2.5">
      <h3 className="text-xs font-semibold tracking-wide text-neutral-500 uppercase">{title}</h3>
      {children}
    </section>
  );
}

/**
 * Sends a value while a control is being pulled (a slider, a color picker) as one gesture: one
 * undo step for the whole pull. Only one send is on its way at a time; values that come in
 * meanwhile are folded into the next one. The control shows `live` (its own value) until the
 * engine has the last one, so it never jumps back.
 */
export function useLiveValue<T>(send: (value: T, gesture: string) => Promise<unknown>) {
  const [live, setLive] = useState<T | null>(null);
  const state = useRef<{ gesture: string | null; busy: boolean; queue: { value: T; gesture: string }[] }>({ gesture: null, busy: false, queue: [] });
  const sendRef = useRef(send);
  sendRef.current = send;

  const settle = () => {
    const s = state.current;
    if (!s.busy && s.gesture === null && s.queue.length === 0) setLive(null);
  };
  const run = () => {
    const s = state.current;
    const next = s.queue.shift();
    if (!next) {
      s.busy = false;
      settle();
      return;
    }
    s.busy = true;
    void sendRef.current(next.value, next.gesture).finally(run);
  };
  return {
    live,
    /** A new value from the control. */
    push(value: T) {
      const s = state.current;
      s.gesture ??= newGesture();
      setLive(value);
      const last = s.queue[s.queue.length - 1];
      if (last && last.gesture === s.gesture) last.value = value;
      else s.queue.push({ value, gesture: s.gesture });
      if (!s.busy) run();
    },
    /** The control was let go: the next value starts a new undo step. */
    end() {
      state.current.gesture = null;
      settle();
    },
  };
}

/**
 * A number typed into a box: nothing is sent while typing; Enter or leaving the box sends it (the
 * caller keeps it in range), and Escape puts back the current value.
 */
export function NumberDraft({
  value,
  onCommit,
  className,
  ...props
}: {
  value: number;
  onCommit: (value: number) => Promise<unknown>;
  className: string;
  "aria-label"?: string;
  id?: string;
  step?: number;
  min?: number;
  max?: number;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft === null) return;
    const n = Number(draft);
    if (draft.trim() === "" || !Number.isFinite(n) || n === value) {
      setDraft(null);
      return;
    }
    // Keep showing what was typed until the engine answers.
    void onCommit(n).finally(() => setDraft((d) => (d === draft ? null : d)));
  };
  return (
    <input
      {...props}
      type="number"
      className={className}
      value={draft ?? String(value)}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") {
          e.stopPropagation();
          setDraft(null);
        }
      }}
    />
  );
}

/** One setting from the catalog: a slider with a number box, a checkbox, or a list. */
export function SettingControl({
  setting,
  value,
  onChange,
  faces,
  tracks,
}: {
  setting: EffectSetting;
  value: unknown;
  onChange: (value: unknown, gesture?: string) => Promise<boolean>;
  /** The faces of the row's props, for a face setting. */
  faces: string[];
  /** The sequence's timing tracks, for a timing track setting. */
  tracks: TimingTrack[];
}) {
  if (setting.type === "face") {
    const current = typeof value === "string" ? value : setting.default;
    const known = current === "" || faces.some((f) => f.toLowerCase() === current.trim().toLowerCase());
    return (
      <label className="flex flex-col gap-1 text-sm" title={setting.description}>
        <span className="text-neutral-600 dark:text-neutral-400">{setting.label}</span>
        <select className={FIELD} value={current} onChange={(e) => void onChange(e.target.value)}>
          <option value="">{faces.length > 0 ? `The first face (${faces[0]})` : "The first face"}</option>
          {faces.map((f) => (
            <option key={f} value={f}>
              {f}
            </option>
          ))}
          {!known && <option value={current}>{current} (not on this prop)</option>}
        </select>
        {faces.length === 0 && <span className="text-xs text-amber-700 dark:text-amber-400">This row's prop has no face. Import one from xLights, or pick another row.</span>}
      </label>
    );
  }
  if (setting.type === "timingTrack") {
    const current = typeof value === "string" ? value : "";
    const lyrics = tracks.filter((t) => t.kind === "phonemes" || t.kind === "words" || t.kind === "lyrics");
    const others = tracks.filter((t) => !lyrics.includes(t));
    return (
      <label className="flex flex-col gap-1 text-sm" title={setting.description}>
        <span className="text-neutral-600 dark:text-neutral-400">{setting.label}</span>
        <select className={FIELD} value={current} onChange={(e) => void onChange(e.target.value === "" ? null : e.target.value)}>
          <option value="">None (mouth at rest)</option>
          {[...lyrics, ...others].map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
        {(() => {
          const kind = tracks.find((t) => t.id === current)?.kind;
          if (current === "" || kind === undefined || kind === "phonemes") return null;
          if (kind === "words" || kind === "lyrics")
            return <span className="text-xs text-neutral-500">Words are turned into mouth shapes letter by letter, so lips move roughly; a phonemes track from xLights is exact.</span>;
          return <span className="text-xs text-neutral-500">This track has no words, so the mouth stays at rest. Pick a lyrics track to sing.</span>;
        })()}
      </label>
    );
  }
  if (setting.type === "bool") {
    return (
      <label className="flex items-center gap-2 text-sm" title={setting.description}>
        <input type="checkbox" checked={typeof value === "boolean" ? value : setting.default} onChange={(e) => void onChange(e.target.checked)} />
        {setting.label}
      </label>
    );
  }
  if (setting.type === "choice") {
    return (
      <label className="flex flex-col gap-1 text-sm" title={setting.description}>
        <span className="text-neutral-600 dark:text-neutral-400">{setting.label}</span>
        <select className={FIELD} value={typeof value === "string" ? value : setting.default} onChange={(e) => void onChange(e.target.value)}>
          {setting.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </label>
    );
  }
  return <NumberSetting setting={setting} value={typeof value === "number" ? value : setting.default} onChange={onChange} />;
}

/** A number setting: a slider (one undo step per pull) and a box to type an exact value. */
export function NumberSetting({
  setting,
  value,
  onChange,
}: {
  setting: Extract<EffectSetting, { type: "number" | "int" }>;
  value: number;
  onChange: (value: unknown, gesture?: string) => Promise<boolean>;
}) {
  const slider = useLiveValue<number>((v, gesture) => onChange(v, gesture));
  const current = slider.live ?? value;
  const places = setting.type === "int" ? 0 : decimals(setting.step);
  const fit = (v: number) => Math.min(setting.max, Math.max(setting.min, setting.type === "int" ? Math.round(v) : v));
  const id = `setting-${setting.key}`;
  return (
    <div className="flex flex-col gap-1 text-sm" title={setting.description}>
      <label htmlFor={id} className="flex justify-between text-neutral-600 dark:text-neutral-400">
        <span>{setting.label}</span>
        {setting.unit && <span className="text-xs text-neutral-500">{setting.unit}</span>}
      </label>
      <div className="flex items-center gap-2">
        <input
          id={id}
          type="range"
          className="min-w-0 flex-1 accent-violet-600"
          min={setting.min}
          max={setting.max}
          step={setting.step}
          value={current}
          aria-valuetext={`${current.toFixed(places)}${setting.unit ? ` ${setting.unit}` : ""}`}
          onChange={(e) => slider.push(fit(Number(e.target.value)))}
          onPointerUp={slider.end}
          onKeyUp={slider.end}
          onBlur={slider.end}
        />
        <NumberDraft
          aria-label={`${setting.label} value`}
          className={`${FIELD} w-20 tabular-nums`}
          min={setting.min}
          max={setting.max}
          step={setting.step}
          value={Number(current.toFixed(places))}
          onCommit={(v) => onChange(fit(v))}
        />
      </div>
    </div>
  );
}

/** A time in milliseconds, typed in and sent on Enter or when the box is left. */
export function MsField({ label, value, hint, onCommit }: { label: string; value: number; hint?: string; onCommit: (v: number) => Promise<unknown> }) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1 text-sm">
      <label htmlFor={id} className="text-neutral-600 dark:text-neutral-400" title={hint}>
        {label}
      </label>
      <NumberDraft id={id} className={`${FIELD} tabular-nums`} min={0} step={25} value={value} onCommit={onCommit} />
    </div>
  );
}

/** The effect's colors: change one with the color picker, add, or remove. */
export function ColorList({ effect, info, change }: { effect: Effect; info: EffectInfo | undefined; change: Change }) {
  const colors = effect.palette.colors;
  const usesOne = info && ["fade"].includes(info.kind);
  const setColors = (next: (colors: string[]) => string[], gesture?: string) =>
    change((e) => ({ ...e, palette: { colors: next(e.palette.colors) } }), gesture);
  return (
    <div className="flex flex-col gap-2">
      {usesOne && <p className="text-xs text-neutral-500">This effect uses the first color.</p>}
      <ul className="flex flex-wrap gap-2" aria-label="Colors">
        {colors.map((color, i) => (
          <li key={i} className="group relative">
            <ColorPicker index={i} color={color} onPick={(value, gesture) => setColors((cs) => cs.map((c, k) => (k === i ? value : c)), gesture)} />
            {colors.length > 1 && (
              <button
                type="button"
                aria-label={`Remove color ${i + 1}`}
                className="absolute -top-1.5 -right-1.5 hidden rounded-full bg-neutral-700 p-0.5 text-white group-focus-within:block group-hover:block"
                onClick={() => void setColors((cs) => (cs.length > 1 ? cs.filter((_, k) => k !== i) : cs))}
              >
                <X size={10} />
              </button>
            )}
          </li>
        ))}
        {colors.length < MAX_COLORS && (
          <li>
            <button
              type="button"
              aria-label="Add a color"
              title="Add a color"
              className="flex h-8 w-8 items-center justify-center rounded border border-dashed border-neutral-400 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
              onClick={() => void setColors((cs) => (cs.length < MAX_COLORS ? [...cs, cs[cs.length - 1] ?? "#ffffff"] : cs))}
            >
              <Plus size={14} />
            </button>
          </li>
        )}
      </ul>
    </div>
  );
}

/** One color: picking in the color picker is one undo step until the picker is left. */
export function ColorPicker({ index, color, onPick }: { index: number; color: string; onPick: (value: string, gesture: string) => Promise<boolean> }) {
  const picking = useLiveValue<string>(onPick);
  return (
    <input
      type="color"
      aria-label={`Color ${index + 1}`}
      className="h-8 w-8 cursor-pointer rounded border border-neutral-300 bg-transparent p-0.5 dark:border-neutral-700"
      value={picking.live ?? color}
      onChange={(e) => picking.push(e.target.value)}
      onBlur={picking.end}
    />
  );
}
