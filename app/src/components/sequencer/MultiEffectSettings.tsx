import { Trash2 } from "lucide-react";
import type { Blend, Effect, EffectParams, Sequence } from "../../api/sequence";
import { effectsById, lengthEdits, shared, sharedKind, updateEach } from "../../lib/multiEdit";
import { targetName } from "../../lib/submodels";
import { formatTime, shiftEdits } from "../../lib/timelineMath";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { BLENDS, ColorList, FIELD, MIXED_OPTION, MixedOption, MsField, Panel, Section, SettingControl, clamp, faceNames } from "./effectControls";

/**
 * Settings for several selected effects at once: what they share (colors, mixing, fades, and
 * timing) and, when they're all one kind, that kind's settings. A field where they differ says
 * "Mixed" and changes nothing until it's changed. Each change goes to every selected effect in
 * one batch, built from the effects as they are when its turn comes: one undo step.
 */
export function MultiEffectSettings({ doc, ids }: { doc: Sequence; ids: string[] }) {
  const catalog = useSequencer((s) => s.catalog);
  const edit = useSequencer((s) => s.edit);
  const show = useApp((s) => s.snapshot?.show);
  const effects = effectsById(doc, ids);
  if (effects.length === 0) return <Panel>{null}</Panel>;

  const kind = sharedKind(effects);
  const info = kind ? catalog.find((c) => c.kind === kind) : undefined;
  const chosen = new Set(ids);
  const rows = doc.rows.filter((r) => r.layers.some((l) => l.effects.some((e) => chosen.has(e.id))));
  const faces = [...new Set(rows.flatMap((r) => faceNames(show, r.target)))];

  /** Changes every selected effect as it is when the change's turn comes (null leaves one alone). */
  const all = (change: (e: Effect, latest: Sequence) => Effect | null, gesture?: string) => edit((latest) => updateEach(latest, ids, change), gesture);
  const setParam = (key: string, value: unknown, gesture?: string) =>
    all((e) => (e.params.kind === kind ? { ...e, params: { ...e.params, [key]: value } as EffectParams } : null), gesture);

  const palette = shared(effects.map((e) => e.palette.colors));
  const blend = shared(effects.map((e) => e.blend));
  const fadeIn = shared(effects.map((e) => e.fadeInMs));
  const fadeOut = shared(effects.map((e) => e.fadeOutMs));
  const length = shared(effects.map((e) => e.endMs - e.startMs));
  const usesColors = effects.some((e) => e.params.kind !== "off" && e.params.kind !== "fire");
  const from = Math.min(...effects.map((e) => e.startMs));
  const to = Math.max(...effects.map((e) => e.endMs));
  const label = kind ? `${effects.length} ${info?.label ?? kind} effects` : `${effects.length} effects`;

  return (
    <Panel>
      <div className="flex items-start justify-between gap-2">
        <div>
          <h2 className="font-semibold">{label}</h2>
          <p className="text-xs text-neutral-500">
            On {rows.length === 1 ? targetName(show, rows[0].target) : `${rows.length} rows`}, {formatTime(from)} – {formatTime(to)}
          </p>
        </div>
        <button
          type="button"
          aria-label={`Delete ${effects.length} effects`}
          title={`Delete ${effects.length} effects`}
          className="rounded p-1 text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/60"
          onClick={() => edit(effects.map((e) => ({ type: "removeEffect" as const, id: e.id })))}
        >
          <Trash2 size={15} />
        </button>
      </div>
      <p className="mt-1 text-xs text-neutral-500">{kind ? `Changes here go to all ${effects.length} of them.` : "Showing settings these effects share."}</p>

      {info && info.settings.length > 0 && (
        <Section title="Settings">
          {info.settings.map((setting) => {
            const value = shared(effects.map((e) => (e.params as Record<string, unknown>)[setting.key] ?? setting.default));
            return (
              <SettingControl
                // A new selection starts the fields afresh.
                key={`${ids.join()}:${setting.key}`}
                setting={setting}
                value={value.value}
                mixed={value.mixed}
                onChange={(v, gesture) => setParam(setting.key, v, gesture)}
                faces={faces}
                tracks={doc.timingTracks}
              />
            );
          })}
        </Section>
      )}

      {usesColors && (
        <Section title="Colors">
          <ColorList
            key={ids.join()}
            colors={palette.value}
            usesOne={kind === "fade"}
            mixed={palette.mixed}
            onChange={(next, gesture) =>
              all((e, latest) => {
                // Built from the colors shown (the first effect's), so they all end up the same.
                const first = effectsById(latest, ids)[0];
                return { ...e, palette: { colors: next(first.palette.colors) } };
              }, gesture)
            }
          />
        </Section>
      )}

      <Section title="Mixing">
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-neutral-600 dark:text-neutral-400">With the layers below</span>
          <select
            className={FIELD}
            value={blend.mixed ? MIXED_OPTION : blend.value}
            onChange={(e) => {
              const value = e.target.value as Blend;
              void all((x) => ({ ...x, blend: value }));
            }}
          >
            <MixedOption mixed={blend.mixed} />
            {BLENDS.map((b) => (
              <option key={b.value} value={b.value} title={b.help}>
                {b.label}
              </option>
            ))}
          </select>
        </label>
        <div className="grid grid-cols-2 items-end gap-2">
          <MsField
            key={`${ids.join()}:fadeIn`}
            label="Fade in (ms)"
            hint="Each effect's fade stays within its length."
            value={fadeIn.value}
            mixed={fadeIn.mixed}
            onCommit={(v) => all((x) => ({ ...x, fadeInMs: clamp(v, 0, x.endMs - x.startMs) }))}
          />
          <MsField
            key={`${ids.join()}:fadeOut`}
            label="Fade out (ms)"
            hint="Each effect's fade stays within its length."
            value={fadeOut.value}
            mixed={fadeOut.mixed}
            onCommit={(v) => all((x) => ({ ...x, fadeOutMs: clamp(v, 0, x.endMs - x.startMs) }))}
          />
        </div>
      </Section>

      <Section title="Timing">
        <div className="grid grid-cols-2 items-end gap-2">
          <MsField
            key={`${ids.join()}:shift`}
            label="Move all by (ms)"
            hint="A negative number moves them earlier."
            value={0}
            blank
            placeholder="0"
            min={-Number.MAX_SAFE_INTEGER}
            onCommit={(v) =>
              edit((latest) => {
                const moved = shiftEdits(latest, ids, v);
                if (moved.edits.length === 0 && Math.round(v) !== 0) {
                  throw new Error("The selected effects can't move that way: they'd run into another effect or past an end of the song.");
                }
                return moved.edits;
              })
            }
          />
          <MsField
            key={`${ids.join()}:length`}
            label="Length of each (ms)"
            hint="Each one keeps its start, and stops at the next effect or the end of the song."
            value={length.value}
            mixed={length.mixed}
            onCommit={(v) => edit((latest) => lengthEdits(latest, ids, v))}
          />
        </div>
      </Section>
    </Panel>
  );
}
