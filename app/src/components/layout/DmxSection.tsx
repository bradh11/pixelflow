import { Plus, X } from "lucide-react";
import type { DmxChannel, DmxRole, Prop } from "../../api/types";
import { DMX_LAYOUTS, DMX_ROLE_LABELS, fixtureFromLayout, fixtureProblem, fromColor, lastAddress, layoutChannels, layoutOf, perCell } from "../../lib/dmx";
import { updateEdits } from "../../lib/layoutEdits";
import { nodeCount } from "../../lib/shows";
import { useApp } from "../../state/store";
import { Input, More, Select } from "../ui";
import { NumberField, Section } from "./PropertiesPanel";

const ROLES = Object.keys(DMX_ROLE_LABELS) as DmxRole[];
const CUSTOM = "custom";

/** A fixture whose cells are `cells`: its shape is a row of that many. */
function withCells(prop: Prop, cells: number): Prop {
  const shape = prop.shape.source === "generator" && prop.shape.type === "line" ? { ...prop.shape, nodes: cells, length: Math.max(0.5, cells * 0.25) } : prop.shape;
  return { ...prop, shape };
}

/**
 * A DMX fixture's own settings: its channel layout (from the library, or its own), how many cells
 * it has (a bar), its start address, and its channels, each with what it does and, for one that
 * isn't worked out from the colour, the value it holds.
 */
export function DmxSection({ prop }: { prop: Prop }) {
  const apply = useApp((s) => s.apply);
  const fixture = prop.dmx;
  if (!fixture) return null;
  const update = (change: (p: Prop) => Prop) => void apply(updateEdits(prop.id, change));
  const cells = nodeCount(prop.shape);
  const layout = layoutOf(fixture, cells);
  const problem = fixtureProblem(fixture, cells);
  const setChannels = (channels: DmxChannel[]) => update((p) => (p.dmx ? { ...p, dmx: { ...p.dmx, channels, layout: undefined } } : p));
  const setChannel = (i: number, patch: Partial<DmxChannel>) =>
    setChannels(
      fixture.channels.map((c, k) => {
        if (k !== i) return c;
        const next = { ...c, ...patch };
        if (!perCell(next.role)) delete next.cell;
        if (fromColor(next.role)) delete next.value;
        return next;
      }),
    );
  const multi = layout?.multiCell || cells > 1;
  return (
    <Section title="DMX fixture">
      <label className="flex flex-col gap-1 text-xs">
        <span className="text-neutral-500 dark:text-neutral-400">Channels</span>
        <Select
          aria-label="Channel layout"
          value={layout?.id ?? CUSTOM}
          onChange={(e) => {
            const chosen = DMX_LAYOUTS.find((l) => l.id === e.target.value);
            if (!chosen) return;
            const n = chosen.multiCell ? Math.max(cells, 4) : 1;
            update((p) => ({ ...withCells(p, n), dmx: { ...fixtureFromLayout(chosen, n, p.dmx?.address ?? 1) } }));
          }}
        >
          {DMX_LAYOUTS.map((l) => (
            <option key={l.id} value={l.id}>
              {l.label}
            </option>
          ))}
          {!layout && <option value={CUSTOM}>Its own: {fixture.channels.length} channels</option>}
        </Select>
      </label>
      <div className={`mt-2 grid gap-2 ${multi ? "grid-cols-2" : "grid-cols-1"}`}>
        <NumberField
          label="Start address"
          hint="The DMX address set on the fixture (1-512)"
          value={fixture.address}
          integer
          min={1}
          max={512}
          onCommit={(address) => update((p) => (p.dmx ? { ...p, dmx: { ...p.dmx, address } } : p))}
        />
        {multi && (
          <NumberField
            label="Cells"
            hint="Separately colored sections along the bar"
            value={cells}
            integer
            min={1}
            max={64}
            onCommit={(n) =>
              update((p) => {
                if (!p.dmx) return p;
                const same = layout ?? DMX_LAYOUTS.find((l) => l.id === "bar-rgb")!;
                return { ...withCells(p, n), dmx: { ...p.dmx, channels: layoutChannels(same, n), layout: same.id } };
              })
            }
          />
        )}
      </div>
      <p className="mt-1 text-xs text-neutral-500">
        Uses channels {fixture.address}–{lastAddress(fixture)} of its universe. Wire it to a DMX controller's universe on the Wiring screen.
      </p>
      {problem && (
        <p role="alert" className="mt-1 text-xs text-red-700 dark:text-red-400">
          {problem}
        </p>
      )}
      <More id={`dmx-channels-${prop.id}`} label={`Each channel (${fixture.channels.length})`}>
        <ol className="flex flex-col gap-1" aria-label="DMX channels">
          {fixture.channels.map((channel, i) => (
            <li key={i} className="flex items-center gap-1 text-xs">
              <span className="w-7 shrink-0 text-right text-neutral-500 tabular-nums">{fixture.address + i}</span>
              <Input aria-label={`Channel ${i + 1} name`} className="min-w-0 flex-1" defaultValue={channel.name} onBlur={(e) => e.target.value !== channel.name && setChannel(i, { name: e.target.value })} />
              <Select aria-label={`Channel ${i + 1} does`} className="w-24" value={channel.role} onChange={(e) => setChannel(i, { role: e.target.value as DmxRole })}>
                {ROLES.map((r) => (
                  <option key={r} value={r}>
                    {DMX_ROLE_LABELS[r]}
                  </option>
                ))}
              </Select>
              {fromColor(channel.role) ? (
                perCell(channel.role) && cells > 1 ? (
                  <Input
                    aria-label={`Channel ${i + 1} cell`}
                    title="Which cell this color is for"
                    className="w-11 tabular-nums"
                    defaultValue={String((channel.cell ?? 0) + 1)}
                    onBlur={(e) => {
                      const n = Number(e.target.value);
                      if (Number.isInteger(n) && n >= 1 && n <= cells) setChannel(i, { cell: n - 1 });
                    }}
                  />
                ) : (
                  <span className="w-11 text-center text-neutral-400" title="Worked out from the color">
                    color
                  </span>
                )
              ) : (
                <Input
                  aria-label={`Channel ${i + 1} holds`}
                  title="The value it holds (0-255) unless a DMX effect sets it"
                  className="w-11 tabular-nums"
                  defaultValue={String(channel.value ?? 0)}
                  onBlur={(e) => {
                    const n = Number(e.target.value);
                    if (Number.isInteger(n) && n >= 0 && n <= 255) setChannel(i, { value: n });
                  }}
                />
              )}
              <button
                type="button"
                aria-label={`Remove channel ${i + 1}`}
                className="rounded p-0.5 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
                onClick={() => setChannels(fixture.channels.filter((_, k) => k !== i))}
              >
                <X size={12} />
              </button>
            </li>
          ))}
        </ol>
        <button
          type="button"
          className="mt-1 flex items-center gap-1 text-xs text-violet-700 dark:text-violet-300"
          onClick={() => setChannels([...fixture.channels, { name: "Other", role: "other" }])}
        >
          <Plus size={12} aria-hidden /> Add a channel
        </button>
        <p className="mt-1 text-xs text-neutral-500">Color and dimmer channels follow the colors drawn on it; the others hold their value unless a DMX effect sets them.</p>
      </More>
    </Section>
  );
}
