import { Plus, Trash2, X } from "lucide-react";
import { useRef } from "react";
import type { Blend, Effect, EffectInfo, EffectParams, EffectSetting, Sequence } from "../../api/sequence";
import type { Show } from "../../api/types";
import { formatTime } from "../../lib/timelineMath";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { targetName } from "./Timeline";

const BLENDS: { value: Blend; label: string; help: string }[] = [
  { value: "normal", label: "Cover", help: "Covers the layers below where it's lit." },
  { value: "add", label: "Add light", help: "Adds its light to the layers below." },
  { value: "max", label: "Brighter of the two", help: "Keeps the brighter color, channel by channel." },
  { value: "multiply", label: "Tint", help: "Tints the layers below with its colors." },
];

const MAX_COLORS = 32;

const FIELD = "rounded-md border border-neutral-300 bg-white px-2 py-1 text-sm dark:border-neutral-700 dark:bg-neutral-950";

/** Shows numbers to the setting's step: 0.05 → 2 decimals. */
function decimals(step: number): number {
  const text = String(step);
  return text.includes(".") ? text.split(".")[1].length : 0;
}

/**
 * The selected effect's settings, built from the engine's effect catalog: its kind's settings,
 * colors, how it mixes with the layers below, fades, and timing. Changes show at once; a slider
 * dragged or a field typed in is one undo step.
 */
export function EffectSettings({ doc }: { doc: Sequence }) {
  const { selection, catalog, edit } = useSequencer();
  const show = useApp((s) => s.snapshot?.show);
  // One undo step per control interaction: the gesture id changes when a control is let go.
  const gestureCount = useRef(0);
  const gesture = (part: string) => `settings:${part}:${gestureCount.current}`;
  const endGesture = () => {
    gestureCount.current++;
  };

  const found = selection.length === 1 ? findEffect(doc, selection[0]) : null;
  if (selection.length > 1) {
    return (
      <Panel>
        <p className="text-sm">{selection.length} effects selected.</p>
        <p className="mt-1 text-xs text-neutral-500">Drag them together on the timeline, or press Delete to remove them.</p>
        <Button className="mt-3" variant="danger" onClick={() => edit(selection.map((id) => ({ type: "removeEffect" as const, id })))}>
          <Trash2 size={14} /> Delete {selection.length} effects
        </Button>
      </Panel>
    );
  }
  if (!found) {
    return (
      <Panel>
        <p className="text-sm text-neutral-500">Select an effect on the timeline to change how it looks.</p>
      </Panel>
    );
  }
  const { effect, rowName } = found;
  const info = catalog.find((c) => c.kind === effect.params.kind);
  const update = (next: Effect, part: string) => void edit([{ type: "updateEffect", effect: next }], gesture(`${effect.id}:${part}`));

  return (
    <Panel>
      <div className="flex items-start justify-between gap-2">
        <div>
          <h2 className="font-semibold">{info?.label ?? effect.params.kind}</h2>
          <p className="text-xs text-neutral-500">
            On {rowName(show)}, {formatTime(effect.startMs)} – {formatTime(effect.endMs)}
          </p>
        </div>
        <button
          type="button"
          aria-label="Delete effect"
          title="Delete effect"
          className="rounded p-1 text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/60"
          onClick={() => edit([{ type: "removeEffect", id: effect.id }])}
        >
          <Trash2 size={15} />
        </button>
      </div>
      {info?.description && <p className="mt-1 text-xs text-neutral-500">{info.description}</p>}

      {info && info.settings.length > 0 && (
        <Section title="Settings">
          {info.settings.map((setting) => (
            <SettingControl
              key={`${effect.id}:${setting.key}`}
              setting={setting}
              value={(effect.params as Record<string, unknown>)[setting.key]}
              onChange={(value) => update({ ...effect, params: { ...effect.params, [setting.key]: value } as EffectParams }, setting.key)}
              onDone={endGesture}
            />
          ))}
        </Section>
      )}

      {effect.params.kind !== "off" && effect.params.kind !== "fire" && (
        <Section title="Colors">
          <ColorList
            effect={effect}
            onChange={(colors, part) => update({ ...effect, palette: { colors } }, part)}
            onDone={endGesture}
            info={info}
          />
        </Section>
      )}

      <Section title="Mixing">
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-neutral-600 dark:text-neutral-400">With the layers below</span>
          <select
            className={FIELD}
            value={effect.blend}
            onChange={(e) => {
              update({ ...effect, blend: e.target.value as Blend }, "blend");
              endGesture();
            }}
          >
            {BLENDS.map((b) => (
              <option key={b.value} value={b.value} title={b.help}>
                {b.label}
              </option>
            ))}
          </select>
        </label>
        <div className="grid grid-cols-2 gap-2">
          <MsField label="Fade in (ms)" value={effect.fadeInMs} max={effect.endMs - effect.startMs} onChange={(v) => update({ ...effect, fadeInMs: v }, "fadeIn")} onDone={endGesture} />
          <MsField label="Fade out (ms)" value={effect.fadeOutMs} max={effect.endMs - effect.startMs} onChange={(v) => update({ ...effect, fadeOutMs: v }, "fadeOut")} onDone={endGesture} />
        </div>
      </Section>

      <Section title="Timing">
        <div className="grid grid-cols-2 gap-2">
          <MsField
            label="Starts (ms)"
            value={effect.startMs}
            max={effect.endMs - doc.frameMs}
            onChange={(v) => update({ ...effect, startMs: v }, "start")}
            onDone={endGesture}
          />
          <MsField
            label="Ends (ms)"
            value={effect.endMs}
            min={effect.startMs + doc.frameMs}
            max={doc.durationMs}
            onChange={(v) => update({ ...effect, endMs: v }, "end")}
            onDone={endGesture}
          />
        </div>
      </Section>
    </Panel>
  );
}

function findEffect(doc: Sequence, id: string): { effect: Effect; rowName: (show: Show | undefined) => string } | null {
  for (const row of doc.rows) {
    for (const layer of row.layers) {
      const effect = layer.effects.find((e) => e.id === id);
      if (effect) return { effect, rowName: (show) => targetName(show, row.target) };
    }
  }
  return null;
}

function Panel({ children }: { children: React.ReactNode }) {
  return (
    <aside aria-label="Effect settings" className="w-72 shrink-0 overflow-auto border-l border-neutral-200 p-3 dark:border-neutral-800">
      {children}
    </aside>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section aria-label={title} className="mt-4 flex flex-col gap-2.5">
      <h3 className="text-xs font-semibold tracking-wide text-neutral-500 uppercase">{title}</h3>
      {children}
    </section>
  );
}

/** One setting from the catalog: a slider with a number box, a checkbox, or a list. */
function SettingControl({
  setting,
  value,
  onChange,
  onDone,
}: {
  setting: EffectSetting;
  value: unknown;
  onChange: (value: unknown) => void;
  onDone: () => void;
}) {
  if (setting.type === "bool") {
    return (
      <label className="flex items-center gap-2 text-sm" title={setting.description}>
        <input
          type="checkbox"
          checked={typeof value === "boolean" ? value : setting.default}
          onChange={(e) => {
            onChange(e.target.checked);
            onDone();
          }}
        />
        {setting.label}
      </label>
    );
  }
  if (setting.type === "choice") {
    return (
      <label className="flex flex-col gap-1 text-sm" title={setting.description}>
        <span className="text-neutral-600 dark:text-neutral-400">{setting.label}</span>
        <select
          className={FIELD}
          value={typeof value === "string" ? value : setting.default}
          onChange={(e) => {
            onChange(e.target.value);
            onDone();
          }}
        >
          {setting.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </label>
    );
  }
  const current = typeof value === "number" ? value : setting.default;
  const places = setting.type === "int" ? 0 : decimals(setting.step);
  const clamp = (v: number) => Math.min(setting.max, Math.max(setting.min, setting.type === "int" ? Math.round(v) : v));
  const id = `setting-${setting.key}`;
  return (
    <div className="flex flex-col gap-1 text-sm" title={setting.description}>
      <label htmlFor={id} className="flex justify-between text-neutral-600 dark:text-neutral-400">
        <span>{setting.label}</span>
        {setting.unit && <span className="text-xs text-neutral-400">{setting.unit}</span>}
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
          onChange={(e) => onChange(clamp(Number(e.target.value)))}
          onPointerUp={onDone}
          onKeyUp={onDone}
          onBlur={onDone}
        />
        <input
          type="number"
          aria-label={`${setting.label} value`}
          className={`${FIELD} w-20 tabular-nums`}
          min={setting.min}
          max={setting.max}
          step={setting.step}
          value={Number(current.toFixed(places))}
          onChange={(e) => {
            const v = Number(e.target.value);
            if (e.target.value !== "" && Number.isFinite(v)) onChange(clamp(v));
          }}
          onBlur={onDone}
        />
      </div>
    </div>
  );
}

function MsField({
  label,
  value,
  min = 0,
  max,
  onChange,
  onDone,
}: {
  label: string;
  value: number;
  min?: number;
  max: number;
  onChange: (v: number) => void;
  onDone: () => void;
}) {
  return (
    <label className="flex flex-col gap-1 text-sm">
      <span className="text-neutral-600 dark:text-neutral-400">{label}</span>
      <input
        type="number"
        className={`${FIELD} tabular-nums`}
        min={min}
        max={max}
        step={25}
        value={value}
        onChange={(e) => {
          const v = Math.round(Number(e.target.value));
          if (e.target.value !== "" && Number.isFinite(v)) onChange(Math.max(min, Math.min(max, v)));
        }}
        onBlur={onDone}
      />
    </label>
  );
}

/** The effect's colors: change one with the color picker, add, or remove. */
function ColorList({
  effect,
  info,
  onChange,
  onDone,
}: {
  effect: Effect;
  info: EffectInfo | undefined;
  onChange: (colors: string[], part: string) => void;
  onDone: () => void;
}) {
  const colors = effect.palette.colors;
  const usesOne = info && ["fade"].includes(info.kind);
  return (
    <div className="flex flex-col gap-2">
      {usesOne && <p className="text-xs text-neutral-500">This effect uses the first color.</p>}
      <ul className="flex flex-wrap gap-2" aria-label="Colors">
        {colors.map((color, i) => (
          <li key={i} className="group relative">
            <input
              type="color"
              aria-label={`Color ${i + 1}`}
              className="h-8 w-8 cursor-pointer rounded border border-neutral-300 bg-transparent p-0.5 dark:border-neutral-700"
              value={color}
              onChange={(e) => onChange(colors.map((c, k) => (k === i ? e.target.value : c)), `color${i}`)}
              onBlur={onDone}
            />
            {colors.length > 1 && (
              <button
                type="button"
                aria-label={`Remove color ${i + 1}`}
                className="absolute -top-1.5 -right-1.5 hidden rounded-full bg-neutral-700 p-0.5 text-white group-focus-within:block group-hover:block"
                onClick={() => {
                  onChange(
                    colors.filter((_, k) => k !== i),
                    "remove",
                  );
                  onDone();
                }}
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
              onClick={() => {
                onChange([...colors, colors[colors.length - 1] ?? "#ffffff"], "add");
                onDone();
              }}
            >
              <Plus size={14} />
            </button>
          </li>
        )}
      </ul>
    </div>
  );
}
