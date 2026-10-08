import { SlidersHorizontal, Trash2 } from "lucide-react";
import type { Blend, Curve, Effect, EffectParams, Sequence, SequenceTarget } from "../../api/sequence";
import type { Show } from "../../api/types";
import { withCurve } from "../../lib/curves";
import { effectBounds, formatTime } from "../../lib/timelineMath";
import { targetName } from "../../lib/submodels";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { useContext } from "react";
import {
  BLUR,
  BlendOptions,
  type Change,
  ColorList,
  ColorPicker,
  FIELD,
  KindSettings,
  MsField,
  NumberSetting,
  Panel,
  SPARKLES,
  Section,
  SettingControl,
  type SettingsPlacement,
  SettingsPlacementContext,
  clamp,
  faceNames,
} from "./effectControls";
import { MultiEffectSettings } from "./MultiEffectSettings";
import { More } from "../ui";
import { BUFFER_TRANSFORMS, renderStyleOptions } from "../../lib/renderStyles";
import type { BufferTransform, RenderStyle } from "../../api/sequence";

/**
 * The selected effect's settings, built from the engine's effect catalog: its kind's settings,
 * colors, how it mixes with the layers below, fades, and timing. Changes show at once; a slider
 * pulled or a field typed in is one undo step. Each change is built from the effect as it is when
 * its turn comes and touches only its own setting, so quick changes in a row all stick.
 */
export function EffectSettings({ doc, placement = "docked" }: { doc: Sequence; placement?: SettingsPlacement }) {
  return (
    <SettingsPlacementContext.Provider value={placement}>
      <Settings doc={doc} />
    </SettingsPlacementContext.Provider>
  );
}

function Settings({ doc }: { doc: Sequence }) {
  const placement = useContext(SettingsPlacementContext);
  const selection = useSequencer((s) => s.selection);
  const catalog = useSequencer((s) => s.catalog);
  const edit = useSequencer((s) => s.edit);
  const show = useApp((s) => s.snapshot?.show);

  const found = selection.length === 1 ? findEffect(doc, selection[0]) : null;
  if (selection.length > 1) return <MultiEffectSettings doc={doc} ids={selection} />;
  if (!found) {
    // Nothing to show: under the preview, a line saying so; beside the timeline, a slim strip.
    if (placement === "stacked") {
      return (
        <aside aria-label="Effect settings" data-collapsed="true" className="flex items-center gap-2 border-t border-neutral-200 p-3 text-xs text-neutral-500 dark:border-neutral-800">
          <SlidersHorizontal size={14} aria-hidden />
          <p>Select an effect on the timeline to change how it looks.</p>
        </aside>
      );
    }
    return (
      <aside
        aria-label="Effect settings"
        data-collapsed="true"
        className="flex w-10 shrink-0 flex-col items-center gap-3 border-l border-neutral-200 py-3 text-neutral-500 dark:border-neutral-800"
      >
        <SlidersHorizontal size={16} aria-hidden />
        <p className="text-xs [writing-mode:vertical-rl]">Select an effect on the timeline to change how it looks.</p>
      </aside>
    );
  }
  const { effect, rowName, target } = found;
  const id = effect.id;
  const info = catalog.find((c) => c.kind === effect.params.kind);
  const change: Change = (next, gesture) =>
    edit((latest) => {
      const current = findEffect(latest, id)?.effect;
      const changed = current ? next(current, latest) : null;
      return changed && JSON.stringify(changed) !== JSON.stringify(current) ? [{ type: "updateEffect", effect: changed }] : [];
    }, gesture);
  const setParam = (key: string, value: unknown, gesture?: string) =>
    change((e) => (e.params.kind === effect.params.kind ? { ...e, params: { ...e.params, [key]: value } as EffectParams } : null), gesture);
  /** The curve on setting `key`: changes it, or (null) holds the setting's value again. */
  const animate = (key: string) => ({
    curve: effect.curves?.[key] ?? null,
    onChange: (curve: Curve | null, gesture?: string) =>
      change((e) => (e.params.kind === effect.params.kind ? { ...e, curves: withCurve(e.curves, key, curve) } : null), gesture),
  });
  const length = effect.endMs - effect.startMs;

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
          onClick={() => edit([{ type: "removeEffect", id }])}
        >
          <Trash2 size={15} />
        </button>
      </div>
      {info?.description && <p className="mt-1 text-xs text-neutral-500">{info.description}</p>}

      {info && info.settings.length > 0 && (
        <Section title="Settings">
          <KindSettings
            settings={info.settings}
            control={(setting) => (
              <SettingControl
                key={`${id}:${setting.key}`}
                setting={setting}
                value={(effect.params as Record<string, unknown>)[setting.key]}
                onChange={(value, gesture) => setParam(setting.key, value, gesture)}
                faces={faceNames(show, target)}
                tracks={doc.timingTracks}
                animate={setting.type === "number" || setting.type === "int" ? animate(setting.key) : undefined}
              />
            )}
          />
        </Section>
      )}

      {effect.params.kind !== "off" && effect.params.kind !== "fire" && (
        <Section title="Colors">
          <ColorList
            key={id}
            colors={effect.palette.colors}
            usesOne={effect.params.kind === "fade"}
            onChange={(next, gesture) => change((e) => ({ ...e, palette: { colors: next(e.palette.colors) } }), gesture)}
          />
        </Section>
      )}

      <Section title="Fades">
        <div className="grid grid-cols-2 gap-2">
          <MsField
            key={`${id}:fadeIn`}
            label="Fade in (ms)"
            hint={`0 to ${length} ms`}
            value={effect.fadeInMs}
            onCommit={(v) => change((x) => ({ ...x, fadeInMs: clamp(v, 0, x.endMs - x.startMs) }))}
          />
          <MsField
            key={`${id}:fadeOut`}
            label="Fade out (ms)"
            hint={`0 to ${length} ms`}
            value={effect.fadeOutMs}
            onCommit={(v) => change((x) => ({ ...x, fadeOutMs: clamp(v, 0, x.endMs - x.startMs) }))}
          />
        </div>
      </Section>

      <More id="effect-settings" label="More: mixing, render style, sparkles, blur, and timing">
        <div className="flex flex-col gap-2.5">
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-neutral-600 dark:text-neutral-400">With the layers below</span>
            <select className={FIELD} value={effect.blend} onChange={(e) => {
                const blend = e.target.value as Blend;
                void change((x) => ({ ...x, blend }));
              }}
            >
              <BlendOptions />
            </select>
          </label>
          <div className="grid grid-cols-2 gap-2">
            <label className="flex min-w-0 flex-col gap-1 text-sm">
              <span className="text-neutral-600 dark:text-neutral-400">Render style</span>
              <select
                className={FIELD}
                value={effect.renderStyle ?? "default"}
                title="How the lights are laid out for this effect"
                onChange={(e) => {
                  const renderStyle = e.target.value as RenderStyle;
                  void change((x) => ({ ...x, renderStyle }));
                }}
              >
                {renderStyleOptions("group" in target).map((s) => (
                  <option key={s.value} value={s.value}>
                    {s.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex min-w-0 flex-col gap-1 text-sm">
              <span className="text-neutral-600 dark:text-neutral-400">Turn or flip</span>
              <select
                className={FIELD}
                value={effect.bufferTransform ?? "none"}
                onChange={(e) => {
                  const bufferTransform = e.target.value as BufferTransform;
                  void change((x) => ({ ...x, bufferTransform }));
                }}
              >
                {BUFFER_TRANSFORMS.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="flex items-end gap-2">
            <div className="min-w-0 flex-1">
              <NumberSetting
                key={`${id}:sparkles`}
                setting={SPARKLES}
                value={effect.sparkles ?? 0}
                onChange={(v, gesture) => change((x) => ({ ...x, sparkles: v as number }), gesture)}
                animate={animate("sparkles")}
              />
            </div>
            <ColorPicker
              key={`${id}:sparkleColor`}
              index={0}
              label="Sparkle color"
              color={effect.sparkleColor ?? "#ffffff"}
              onPick={(value, gesture) => change((x) => ({ ...x, sparkleColor: value }), gesture)}
            />
          </div>
          <NumberSetting
            key={`${id}:blur`}
            setting={BLUR}
            value={effect.blur ?? 0}
            onChange={(v, gesture) => change((x) => ({ ...x, blur: v as number }), gesture)}
            animate={animate("blur")}
          />
          <h4 className="mt-1 text-xs font-semibold tracking-wide text-neutral-500 uppercase">Timing</h4>
          <div className="grid grid-cols-2 gap-2">
            <MsField
              key={`${id}:start`}
              label="Starts (ms)"
              value={effect.startMs}
              onCommit={(v) =>
                change((x, latest) => {
                  // Not past its end, and not into the effect before it on its layer.
                  const lo = effectBounds(latest, x.id)?.lo ?? 0;
                  return { ...x, startMs: clamp(v, lo, x.endMs - latest.frameMs) };
                })
              }
            />
            <MsField
              key={`${id}:end`}
              label="Ends (ms)"
              value={effect.endMs}
              onCommit={(v) =>
                change((x, latest) => {
                  // Not before its start, and not into the effect after it on its layer.
                  const hi = Math.min(latest.durationMs, effectBounds(latest, x.id)?.hi ?? latest.durationMs);
                  return { ...x, endMs: clamp(v, x.startMs + latest.frameMs, hi) };
                })
              }
            />
          </div>
        </div>
      </More>
    </Panel>
  );
}

function findEffect(
  doc: Sequence,
  id: string,
): { effect: Effect; target: SequenceTarget; rowName: (show: Show | undefined) => string } | null {
  for (const row of doc.rows) {
    for (const layer of row.layers) {
      const effect = layer.effects.find((e) => e.id === id);
      if (effect) return { effect, target: row.target, rowName: (show) => targetName(show, row.target) };
    }
  }
  return null;
}
