import { Plus, X } from "lucide-react";
import { useState } from "react";
import type { DmxValues, SequenceTarget } from "../../api/sequence";
import type { DmxChannel, Show } from "../../api/types";
import { DMX_ROLE_LABELS } from "../../lib/dmx";
import { memberProp } from "../../lib/shows";
import { targetProp } from "../../lib/submodels";
import { type CurveControl, FIELD, NumberSetting } from "./effectControls";

/** The channels of the first DMX fixture a row on `target` plays on (a group's first fixture), to name them by. */
export function fixtureChannels(show: Show | undefined, target: SequenceTarget): DmxChannel[] {
  const ids = "group" in target ? (show?.groups.find((g) => g.id === target.group)?.members ?? []).map(memberProp) : [targetProp(target)];
  for (const id of ids) {
    const prop = show?.props.find((p) => p.id === id);
    if (prop?.dmx) return prop.dmx.channels;
  }
  return [];
}

/** "3 · Blue": a channel by its number, with its name when the fixture has one there. */
function channelLabel(n: number, channels: DmxChannel[]): string {
  const channel = channels[n - 1];
  return channel ? `${n} · ${channel.name || DMX_ROLE_LABELS[channel.role]}` : `Channel ${n}`;
}

/**
 * A DMX effect's channels: each it sets, by number, as a slider from 0 to 255 that can also
 * change over the effect, with a button to stop setting it; and a list to set another by. The
 * channels are named as the row's fixture names them (a group: its first fixture's).
 */
export function DmxChannelsSetting({
  effectId,
  values,
  channels,
  onChange,
  animate,
}: {
  effectId: string;
  values: DmxValues;
  /** The row's fixture's channels, to name them by (none: just numbers). */
  channels: DmxChannel[];
  /** One channel set (`value` undefined: no longer set), with the gesture a slider's pull is. */
  onChange: (channel: number, value: number | undefined, gesture?: string) => Promise<boolean>;
  /** The curve on `channels.<n>`. */
  animate: (key: string) => CurveControl;
}) {
  const set = Object.keys(values)
    .map(Number)
    .filter((n) => Number.isInteger(n) && n >= 1 && n <= 512)
    .sort((a, b) => a - b);
  const choices = (channels.length > 0 ? channels.map((_, i) => i + 1) : Array.from({ length: 512 }, (_, i) => i + 1)).filter((n) => !set.includes(n));
  const [next, setNext] = useState("");
  return (
    <div className="flex flex-col gap-2.5" role="group" aria-label="DMX channels">
      {channels.length === 0 && <p className="text-xs text-neutral-500">This row has no DMX fixture: the channels are set on any fixture the effect plays on.</p>}
      {set.map((n) => {
        const key = `channels.${n}`;
        const setting = { key, label: channelLabel(n, channels), description: "", type: "int" as const, min: 0, max: 255, step: 1, default: 0 };
        return (
          <div key={`${effectId}:${n}`} className="flex items-start gap-1">
            <div className="min-w-0 flex-1">
              <NumberSetting setting={setting} value={values[String(n)] ?? 0} onChange={(v, gesture) => onChange(n, Number(v), gesture)} animate={animate(key)} />
            </div>
            <button
              type="button"
              aria-label={`Stop setting ${setting.label}`}
              title="Stop setting this channel"
              className="mt-5 rounded p-0.5 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
              onClick={() => void onChange(n, undefined)}
            >
              <X size={13} />
            </button>
          </div>
        );
      })}
      {choices.length > 0 && (
        <div className="flex items-center gap-1">
          <select aria-label="Channel to set" className={`${FIELD} min-w-0 flex-1`} value={next} onChange={(e) => setNext(e.target.value)}>
            <option value="">Set a channel…</option>
            {choices.map((n) => (
              <option key={n} value={n}>
                {channelLabel(n, channels)}
              </option>
            ))}
          </select>
          <button
            type="button"
            aria-label="Set this channel"
            title="Set this channel"
            disabled={next === ""}
            className="rounded p-1 text-violet-700 hover:bg-violet-50 disabled:opacity-40 dark:text-violet-300 dark:hover:bg-violet-950/60"
            onClick={() => {
              const n = Number(next);
              setNext("");
              void onChange(n, channels[n - 1]?.value ?? 0);
            }}
          >
            <Plus size={14} />
          </button>
        </div>
      )}
    </div>
  );
}
