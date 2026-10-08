import { Plus, Spline, X } from "lucide-react";
import { createContext, useContext, useEffect, useId, useRef, useState } from "react";
import type { Blend, Curve, Effect, EffectSetting, Sequence, SequenceTarget, TimingTrack } from "../../api/sequence";
import type { Show } from "../../api/types";
import { startCurve } from "../../lib/curves";
import { memberProp } from "../../lib/shows";
import { facesOf, targetProp } from "../../lib/submodels";
import { newGesture } from "../../state/sequencer";
import { More } from "../ui";
import { CurveEditor } from "./CurveEditor";

// The controls the effect settings panel is made of: setting fields from the catalog, colors,
// times, and the sections they sit in.
/** How an effect can mix with the layers below: the everyday ones first, then the rest of
 * xLights' layer methods (each help names the xLights one: "1" is this effect, "2" the layers below). */
export const BLENDS: { value: Blend; label: string; help: string; more?: true }[] = [
  { value: "normal", label: "Cover", help: "Covers the layers below where it's lit." },
  { value: "add", label: "Add light", help: "Adds its light to the layers below (xLights: Additive)." },
  { value: "max", label: "Brighter of the two", help: "Keeps the brighter color, channel by channel." },
  { value: "multiply", label: "Tint", help: "Tints the layers below with its colors." },
  { value: "behind", label: "Behind", help: "Shows only where the layers below are dark (xLights: 2 reveals 1, Layered)." },
  { value: "over", label: "Over where lit", help: "Shows where it's lit; the layers below show elsewhere (xLights: 1 reveals 2).", more: true },
  { value: "subtract", label: "Take light away", help: "Takes its light away from the layers below (xLights: Subtractive).", more: true },
  { value: "min", label: "Darker of the two", help: "Keeps the darker color, channel by channel; black where it's unlit.", more: true },
  { value: "average", label: "Average", help: "Averages its color with the layers below where both are lit.", more: true },
  { value: "mask", label: "Mask", help: "Blacks out the layers below where it's lit (xLights: 1 is Mask).", more: true },
  { value: "reveal", label: "Window", help: "Shows the layers below only where it's lit, black elsewhere (xLights: 1 is True Unmask).", more: true },
  { value: "revealBrightness", label: "Window, its brightness", help: "The layers below at its brightness where it's lit, black elsewhere (xLights: 1 is Unmask).", more: true },
  { value: "cutOut", label: "Cut out by below", help: "Shows where the layers below are dark, black where they're lit (xLights: 2 is Mask).", more: true },
  { value: "clip", label: "Inside below", help: "Shows only where the layers below are lit, black elsewhere (xLights: 2 is True Unmask).", more: true },
  { value: "clipBrightness", label: "Inside below, its brightness", help: "Its colors at the brightness of the layers below, black where they're dark (xLights: 2 is Unmask).", more: true },
  { value: "highlight", label: "Highlight", help: "Shows only where both it and the layers below are lit.", more: true },
  { value: "highlightAdd", label: "Highlight, add light", help: "Adds its light only where the layers below are lit (xLights: Highlight Vibrant).", more: true },
  { value: "shadow", label: "Shadow on below", help: "Shifts the hue of the layers below where it's lit (xLights: Shadow 1 on 2).", more: true },
  { value: "shadowBelow", label: "Shadow from below", help: "Its colors, hue shifted by the layers below (xLights: Shadow 2 on 1).", more: true },
  { value: "bottomHalf", label: "Bottom half only", help: "Shows on the bottom half; the layers below on the top half (xLights: Bottom-Top).", more: true },
  { value: "leftHalf", label: "Left half only", help: "Shows on the left half; the layers below on the right half (xLights: Left-Right).", more: true },
];

/** The blends as options for a list, the less common ones grouped apart. */
export function BlendOptions() {
  const option = (b: (typeof BLENDS)[number]) => (
    <option key={b.value} value={b.value} title={b.help}>
      {b.label}
    </option>
  );
  return (
    <>
      {BLENDS.filter((b) => !b.more).map(option)}
      <optgroup label="More, as in xLights">{BLENDS.filter((b) => b.more).map(option)}</optgroup>
    </>
  );
}

/** Sparkles and blur: settings every effect has, edited like the catalog's number settings. */
export const SPARKLES: Extract<EffectSetting, { type: "int" }> = {
  key: "sparkles",
  label: "Sparkles",
  description: "Lit pixels flash the sparkle color now and then: 0 is none, 200 the most.",
  type: "int",
  min: 0,
  max: 200,
  step: 1,
  default: 0,
};
export const BLUR: Extract<EffectSetting, { type: "int" }> = {
  key: "blur",
  label: "Blur",
  description: "Softens the effect before it mixes with the layers below: 0 is none, 14 the most.",
  type: "int",
  min: 0,
  max: 14,
  step: 1,
  default: 0,
};

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

/**
 * Where the settings sit: a column of their own beside the timeline ("docked"), over the
 * timeline's right edge in a narrow window ("floating"), or under the preview in its column
 * ("stacked").
 */
export type SettingsPlacement = "docked" | "floating" | "stacked";
export const SettingsPlacementContext = createContext<SettingsPlacement>("docked");

export function Panel({ children }: { children: React.ReactNode }) {
  const placement = useContext(SettingsPlacementContext);
  const floating = placement === "floating";
  return (
    <aside
      aria-label="Effect settings"
      data-floating={floating || undefined}
      className={`overflow-auto border-neutral-200 p-3 dark:border-neutral-800 ${
        placement === "stacked" ? "min-h-0 w-full flex-1 border-t" : "w-72 shrink-0 border-l"
      } ${floating ? "absolute inset-y-0 right-0 z-20 bg-white shadow-2xl dark:bg-neutral-950" : ""}`}
    >
      {children}
    </aside>
  );
}

/** An effect kind's settings: the everyday ones, then the ones most people leave alone under "More". */
export function KindSettings({ settings, control }: { settings: EffectSetting[]; control: (setting: EffectSetting) => React.ReactNode }) {
  const more = settings.filter((s) => s.more);
  return (
    <>
      {settings.filter((s) => !s.more).map(control)}
      {more.length > 0 && (
        <More id="effect-kind-settings" label="More settings">
          <div className="flex flex-col gap-2.5">{more.map(control)}</div>
        </More>
      )}
    </>
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

/** What a field shows when the selected effects have different values in it. */
export const MIXED = "Mixed";
/** A list's value while the selected effects have different values in it (never a real value). */
export const MIXED_OPTION = "\u0000mixed";

/**
 * A number typed into a box: nothing is sent while typing; Enter or leaving the box sends it (the
 * caller keeps it in range), and Escape puts back the current value. A `blank` box shows no value
 * (the selected effects differ, or it takes an amount to apply), and any number typed is sent.
 */
export function NumberDraft({
  value,
  onCommit,
  className,
  blank = false,
  placeholder,
  ...props
}: {
  value: number;
  onCommit: (value: number) => Promise<unknown>;
  className: string;
  blank?: boolean;
  placeholder?: string;
  "aria-label"?: string;
  id?: string;
  step?: number;
  min?: number;
  max?: number;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  /** The draft on its way to the engine: Enter and then leaving the box send it once, not twice
   * (twice would move effects twice in a box that takes an amount). */
  const sending = useRef<string | null>(null);
  const commit = () => {
    if (draft === null || draft === sending.current) return;
    const n = Number(draft);
    if (draft.trim() === "" || !Number.isFinite(n) || (!blank && n === value)) {
      setDraft(null);
      return;
    }
    sending.current = draft;
    // Keep showing what was typed until the engine answers.
    void onCommit(n).finally(() => {
      if (sending.current === draft) sending.current = null;
      setDraft((d) => (d === draft ? null : d));
    });
  };
  return (
    <input
      {...props}
      type="number"
      className={className}
      placeholder={placeholder}
      value={draft ?? (blank ? "" : String(value))}
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

/** A checkbox that can also show "mixed" (some of the selected effects have it on, some off). */
function MixedCheckbox({ checked, mixed, onChange }: { checked: boolean; mixed: boolean; onChange: (checked: boolean) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  // Every render: a click clears the browser's mixed state, and a refused change leaves `mixed` as it was.
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = mixed;
  });
  return (
    <input
      ref={ref}
      type="checkbox"
      checked={mixed ? false : checked}
      onChange={(e) => {
        onChange(e.target.checked);
        // Mixed until the change has landed (a refused one changes nothing on screen).
        e.target.indeterminate = mixed;
      }}
    />
  );
}

/** "Mixed" at the top of a list whose effects differ (it can't be picked). */
export function MixedOption({ mixed }: { mixed: boolean }) {
  return mixed ? (
    <option value={MIXED_OPTION} disabled>
      {MIXED}
    </option>
  ) : null;
}

/** A number setting's change over its effect, for the controls that offer one. */
export interface CurveControl {
  /** The setting's curve; null when it holds one value. */
  curve: Curve | null;
  /** The selected effects change this setting differently. */
  mixed?: boolean;
  /** A new curve, or null to hold one value again. */
  onChange: (curve: Curve | null, gesture?: string) => Promise<boolean>;
}

/**
 * One setting from the catalog: a slider with a number box, a checkbox, or a list. With `mixed`,
 * the selected effects have different values in it: the field says so, and only a change to it
 * is sent. A number setting given `animate` can also change over the effect.
 */
export function SettingControl({
  setting,
  value,
  onChange,
  faces,
  tracks,
  mixed = false,
  animate,
}: {
  setting: EffectSetting;
  value: unknown;
  onChange: (value: unknown, gesture?: string) => Promise<boolean>;
  /** The faces of the row's props, for a face setting. */
  faces: string[];
  /** The sequence's timing tracks, for a timing track setting. */
  tracks: TimingTrack[];
  mixed?: boolean;
  animate?: CurveControl;
}) {
  if (setting.type === "face") {
    const current = typeof value === "string" ? value : setting.default;
    const known = mixed || current === "" || faces.some((f) => f.toLowerCase() === current.trim().toLowerCase());
    return (
      <label className="flex flex-col gap-1 text-sm" title={setting.description}>
        <span className="text-neutral-600 dark:text-neutral-400">{setting.label}</span>
        <select className={FIELD} value={mixed ? MIXED_OPTION : current} onChange={(e) => void onChange(e.target.value)}>
          <MixedOption mixed={mixed} />
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
        <select className={FIELD} value={mixed ? MIXED_OPTION : current} onChange={(e) => void onChange(e.target.value === "" ? null : e.target.value)}>
          <MixedOption mixed={mixed} />
          <option value="">None (mouth at rest)</option>
          {[...lyrics, ...others].map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
        {(() => {
          const kind = tracks.find((t) => t.id === current)?.kind;
          if (mixed || current === "" || kind === undefined || kind === "phonemes") return null;
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
        <MixedCheckbox checked={typeof value === "boolean" ? value : setting.default} mixed={mixed} onChange={(checked) => void onChange(checked)} />
        {setting.label}
      </label>
    );
  }
  if (setting.type === "choice") {
    return (
      <label className="flex flex-col gap-1 text-sm" title={setting.description}>
        <span className="text-neutral-600 dark:text-neutral-400">{setting.label}</span>
        <select className={FIELD} value={mixed ? MIXED_OPTION : typeof value === "string" ? value : setting.default} onChange={(e) => void onChange(e.target.value)}>
          <MixedOption mixed={mixed} />
          {setting.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </label>
    );
  }
  return <NumberSetting setting={setting} value={typeof value === "number" ? value : setting.default} onChange={onChange} mixed={mixed} animate={animate} />;
}

/**
 * A number setting: a slider (one undo step per pull) and a box to type an exact value. Given
 * `animate`, a toggle beside its name makes it change over the effect instead, with a compact
 * curve editor in place of the slider.
 */
export function NumberSetting({
  setting,
  value,
  onChange,
  mixed = false,
  animate,
}: {
  setting: Extract<EffectSetting, { type: "number" | "int" }>;
  value: number;
  onChange: (value: unknown, gesture?: string) => Promise<boolean>;
  mixed?: boolean;
  animate?: CurveControl;
}) {
  const slider = useLiveValue<number>((v, gesture) => onChange(v, gesture));
  const current = slider.live ?? value;
  // Once the slider is pulled, every effect has its value.
  const differs = mixed && slider.live === null;
  const places = setting.type === "int" ? 0 : decimals(setting.step);
  const fit = (v: number) => Math.min(setting.max, Math.max(setting.min, setting.type === "int" ? Math.round(v) : v));
  const id = `setting-${setting.key}`;
  const curve = animate?.curve ?? null;
  const on = curve !== null || animate?.mixed === true;
  return (
    <div className="flex flex-col gap-1 text-sm" title={setting.description}>
      <div className="flex items-center justify-between gap-2 text-neutral-600 dark:text-neutral-400">
        <label htmlFor={id}>{setting.label}</label>
        <span className="flex items-center gap-1.5">
          {setting.unit && <span className="text-xs text-neutral-500">{setting.unit}</span>}
          {animate && (
            <button
              type="button"
              aria-pressed={on}
              aria-label={`Change ${setting.label} over the effect`}
              title={on ? "Changes over the effect. Click to hold one value." : "Change over the effect"}
              className={`rounded p-0.5 ${on ? "bg-violet-100 text-violet-700 dark:bg-violet-950 dark:text-violet-300" : "text-neutral-400 hover:bg-neutral-100 hover:text-neutral-700 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"}`}
              onClick={() => void animate.onChange(on ? null : startCurve(differs ? setting.default : current, setting.min, setting.max))}
            >
              <Spline size={13} aria-hidden />
            </button>
          )}
        </span>
      </div>
      {animate?.mixed ? (
        <p className="text-xs text-neutral-500">
          Changes differently in each effect.{" "}
          <button type="button" className="text-violet-700 underline dark:text-violet-300" onClick={() => void animate.onChange(null)}>
            Hold one value
          </button>
        </p>
      ) : curve && animate ? (
        <CurveEditor id={id} setting={setting} curve={curve} onChange={animate.onChange} />
      ) : (
        <div className="flex items-center gap-2">
          <input
            id={id}
            type="range"
            className="min-w-0 flex-1 accent-violet-600"
            min={setting.min}
            max={setting.max}
            step={setting.step}
            value={current}
            aria-valuetext={differs ? MIXED : `${current.toFixed(places)}${setting.unit ? ` ${setting.unit}` : ""}`}
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
            blank={differs}
            placeholder={differs ? MIXED : undefined}
            onCommit={(v) => onChange(fit(v))}
          />
        </div>
      )}
    </div>
  );
}

/** A time in milliseconds, typed in and sent on Enter or when the box is left. */
export function MsField({
  label,
  value,
  hint,
  onCommit,
  mixed = false,
  blank = false,
  placeholder,
  min = 0,
}: {
  label: string;
  value: number;
  hint?: string;
  onCommit: (v: number) => Promise<unknown>;
  /** The selected effects have different values here. */
  mixed?: boolean;
  /** The box takes an amount to apply, and shows none. */
  blank?: boolean;
  placeholder?: string;
  min?: number;
}) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1 text-sm">
      <label htmlFor={id} className="text-neutral-600 dark:text-neutral-400" title={hint}>
        {label}
      </label>
      <NumberDraft id={id} className={`${FIELD} tabular-nums`} min={min} step={25} value={value} blank={mixed || blank} placeholder={mixed ? MIXED : placeholder} onCommit={onCommit} />
    </div>
  );
}

/**
 * Colors: change one with the color picker, add, or remove. `onChange` gets how the list changes
 * (applied to the colors as they are when its turn comes). With `mixed`, the selected effects
 * have different colors; these are the first one's, and a change gives all of them the result.
 */
export function ColorList({
  colors,
  usesOne,
  mixed = false,
  onChange,
}: {
  colors: string[];
  usesOne: boolean;
  mixed?: boolean;
  onChange: (next: (colors: string[]) => string[], gesture?: string) => Promise<boolean>;
}) {
  return (
    <div className="flex flex-col gap-2">
      {mixed && <p className="text-xs text-neutral-500">These effects have different colors. Changing them gives every one of them these colors.</p>}
      {usesOne && <p className="text-xs text-neutral-500">This effect uses the first color.</p>}
      <ul className="flex flex-wrap gap-2" aria-label="Colors">
        {colors.map((color, i) => (
          <li key={i} className="group relative">
            <ColorPicker index={i} color={color} onPick={(value, gesture) => onChange((cs) => cs.map((c, k) => (k === i ? value : c)), gesture)} />
            {colors.length > 1 && (
              <button
                type="button"
                aria-label={`Remove color ${i + 1}`}
                data-tip={`Remove color ${i + 1}`}
                className="absolute -top-1.5 -right-1.5 hidden rounded-full bg-neutral-700 p-0.5 text-white group-focus-within:block group-hover:block"
                onClick={() => void onChange((cs) => (cs.length > 1 ? cs.filter((_, k) => k !== i) : cs))}
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
              onClick={() => void onChange((cs) => (cs.length < MAX_COLORS ? [...cs, cs[cs.length - 1] ?? "#ffffff"] : cs))}
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
export function ColorPicker({
  index,
  color,
  onPick,
  label = `Color ${index + 1}`,
}: {
  index: number;
  color: string;
  onPick: (value: string, gesture: string) => Promise<boolean>;
  label?: string;
}) {
  const picking = useLiveValue<string>(onPick);
  return (
    <input
      type="color"
      aria-label={label}
      title={label}
      className="h-8 w-8 cursor-pointer rounded border border-neutral-300 bg-transparent p-0.5 dark:border-neutral-700"
      value={picking.live ?? color}
      onChange={(e) => picking.push(e.target.value)}
      onBlur={picking.end}
    />
  );
}
