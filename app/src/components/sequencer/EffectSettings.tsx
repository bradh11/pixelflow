import { Trash2 } from "lucide-react";
import type { Blend, Effect, EffectParams, Sequence, SequenceTarget } from "../../api/sequence";
import type { Show } from "../../api/types";
import { effectBounds, formatTime } from "../../lib/timelineMath";
import { targetName } from "../../lib/submodels";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { BLENDS, type Change, ColorList, FIELD, MsField, Panel, Section, SettingControl, clamp, faceNames } from "./effectControls";
import { MultiEffectSettings } from "./MultiEffectSettings";

/**
 * The selected effect's settings, built from the engine's effect catalog: its kind's settings,
 * colors, how it mixes with the layers below, fades, and timing. Changes show at once; a slider
 * pulled or a field typed in is one undo step. Each change is built from the effect as it is when
 * its turn comes and touches only its own setting, so quick changes in a row all stick.
 */
export function EffectSettings({ doc }: { doc: Sequence }) {
  const selection = useSequencer((s) => s.selection);
  const catalog = useSequencer((s) => s.catalog);
  const edit = useSequencer((s) => s.edit);
  const show = useApp((s) => s.snapshot?.show);

  const found = selection.length === 1 ? findEffect(doc, selection[0]) : null;
  if (selection.length > 1) return <MultiEffectSettings doc={doc} ids={selection} />;
  if (!found) {
    return (
      <Panel>
        <p className="text-sm text-neutral-500">Select an effect on the timeline to change how it looks.</p>
      </Panel>
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
          {info.settings.map((setting) => (
            <SettingControl
              key={`${id}:${setting.key}`}
              setting={setting}
              value={(effect.params as Record<string, unknown>)[setting.key]}
              onChange={(value, gesture) => setParam(setting.key, value, gesture)}
              faces={faceNames(show, target)}
              tracks={doc.timingTracks}
            />
          ))}
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

      <Section title="Mixing">
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-neutral-600 dark:text-neutral-400">With the layers below</span>
          <select className={FIELD} value={effect.blend} onChange={(e) => {
              const blend = e.target.value as Blend;
              void change((x) => ({ ...x, blend }));
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

      <Section title="Timing">
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
      </Section>
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
