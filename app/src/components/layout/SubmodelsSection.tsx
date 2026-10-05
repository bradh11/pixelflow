import { Plus, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { BufferStyle, LineLayout, Phoneme, Prop, Region } from "../../api/types";
import { updateEdits } from "../../lib/layoutEdits";
import { nodeCount } from "../../lib/shows";
import { PHONEMES, formatLine, formatRanges, newSubmodel, parseLine, regionKindLabel, regionNameProblem, regionNodes, regionUses } from "../../lib/submodels";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { Button, Input, Select } from "../ui";
import { NumberField, Section } from "./PropertiesPanel";

const LAYOUTS: { value: LineLayout; label: string }[] = [
  { value: "horizontal", label: "Rows (first line at the bottom)" },
  { value: "vertical", label: "Columns (first line on the left)" },
];

const BUFFERS: { value: BufferStyle; label: string; help: string }[] = [
  { value: "default", label: "Lines side by side", help: "Each line is its own row or column, so an effect sweeps across them." },
  { value: "keepXY", label: "Where the pixels are", help: "Effects see the pixels where they really are on the prop." },
  { value: "stackedStrands", label: "Lines on top of each other", help: "Every line shows the same part of the effect, like rings of a star doing the same thing." },
];

/**
 * A prop's submodels and faces: pick one to see its pixels on the canvas, add, rename, and
 * delete submodels, and edit a submodel's pixel lists. Faces (imported from xLights) are listed
 * with the pixels each part uses, and can show any mouth shape on the canvas.
 */
export function SubmodelsSection({ prop, points }: { prop: Prop; points: ArrayLike<number> | undefined }) {
  const apply = useApp((s) => s.apply);
  const highlight = useLayoutEditor((s) => s.highlight);
  const setHighlight = useLayoutEditor((s) => s.setHighlight);
  const count = nodeCount(prop.shape);
  const picked = highlight?.prop === prop.id ? prop.regions.find((r) => r.id === highlight.region) : undefined;
  const update = (change: (regions: Region[]) => Region[]) => apply(updateEdits(prop.id, (p) => ({ ...p, regions: change(p.regions) })));
  const pick = (region: Region | null) => setHighlight(region ? { prop: prop.id, region: region.id, phoneme: null } : null);

  const add = async () => {
    const region = newSubmodel(prop);
    if (await update((regions) => [...regions, region])) pick(region);
  };

  return (
    <Section title="Submodels & faces">
      {prop.regions.length === 0 ? (
        <p className="mb-2 text-sm text-neutral-500">
          No submodels yet. A submodel is part of a prop (one arch of a set, a ring of a star, a window of a matrix) that gets its own row in a sequence.
        </p>
      ) : (
        <ul aria-label="Submodels and faces" className="mb-2 flex flex-col gap-0.5">
          {prop.regions.map((r) => {
            const selected = picked?.id === r.id;
            const pixels = regionNodes(r, count, points).length;
            return (
              <li key={r.id}>
                <button
                  type="button"
                  aria-pressed={selected}
                  onClick={() => pick(selected ? null : r)}
                  className={`flex w-full items-baseline justify-between gap-2 rounded px-2 py-1 text-left text-sm ${
                    selected ? "bg-accent-500/15 text-accent-700 dark:text-accent-300" : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
                  }`}
                >
                  <span className="truncate">{r.name}</span>
                  <span className="shrink-0 text-xs text-neutral-500">
                    {regionKindLabel(r)} · {pixels === 1 ? "1 pixel" : `${pixels} pixels`}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <Button onClick={() => void add()}>
        <Plus size={16} aria-hidden /> Add submodel
      </Button>
      {picked && (
        <div className="mt-3 rounded-md border border-neutral-200 p-2 dark:border-neutral-800">
          {picked.kind === "face" ? (
            <div className="flex flex-col gap-1 text-xs">
              <span className="text-neutral-500 dark:text-neutral-400">Name</span>
              <span className="text-sm">{picked.name}</span>
              <span className="text-neutral-500">Faces effects find a face by its name, so faces keep the name they came with from xLights.</span>
            </div>
          ) : (
            <NameField key={`name:${picked.id}`} prop={prop} region={picked} rename={(name) => update((rs) => rs.map((r) => (r.id === picked.id ? { ...r, name } : r)))} />
          )}
          {picked.kind === "face" ? (
            <FaceDetails region={picked} phoneme={highlight?.phoneme ?? null} show={(phoneme) => setHighlight({ prop: prop.id, region: picked.id, phoneme })} />
          ) : picked.kind === "subBuffer" ? (
            <RectangleEditor region={picked} update={(next) => update((rs) => rs.map((r) => (r.id === picked.id ? next : r)))} />
          ) : (
            <LinesEditor key={`lines:${picked.id}`} prop={prop} region={picked} update={(next) => update((rs) => rs.map((r) => (r.id === picked.id ? next : r)))} />
          )}
          <DeleteRegion
            key={`delete:${picked.id}`}
            prop={prop}
            region={picked}
            remove={async () => {
              if (await update((rs) => rs.filter((r) => r.id !== picked.id))) pick(null);
            }}
          />
        </div>
      )}
    </Section>
  );
}

/** The delete button; when something uses the region it says what, and asks first. */
function DeleteRegion({ prop, region, remove }: { prop: Prop; region: Region; remove: () => Promise<void> }) {
  const show = useApp((s) => s.snapshot?.show);
  const doc = useSequencer((s) => s.doc);
  const [asking, setAsking] = useState<string | null>(null);
  const label = `Delete ${region.kind === "face" ? "face" : "submodel"}`;
  if (asking)
    return (
      <div role="alert" className="mt-3 flex flex-col gap-2 rounded-md border border-amber-300 bg-amber-50 p-2 text-xs text-amber-900 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
        <p>{asking}</p>
        <div className="flex gap-2">
          <Button variant="danger" onClick={() => void remove()}>
            <Trash2 size={16} aria-hidden /> Delete anyway
          </Button>
          <Button onClick={() => setAsking(null)}>Keep it</Button>
        </div>
      </div>
    );
  return (
    <Button
      variant="danger"
      className="mt-3"
      onClick={() => {
        const uses = regionUses(show, doc, prop, region);
        if (uses) setAsking(uses);
        else void remove();
      }}
    >
      <Trash2 size={16} aria-hidden /> {label}
    </Button>
  );
}

/** The region's name, saved when it's a name the prop doesn't already use. */
function NameField({ prop, region, rename }: { prop: Prop; region: Region; rename: (name: string) => Promise<boolean> }) {
  const [draft, setDraft] = useState(region.name);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => setDraft(region.name), [region.name]);
  const commit = () => {
    if (draft.trim() === region.name) return setProblem(null);
    const why = regionNameProblem(prop, draft, region.id);
    setProblem(why);
    if (!why) void rename(draft.trim());
  };
  return (
    <label className="flex flex-col gap-1 text-xs">
      <span className="text-neutral-500 dark:text-neutral-400">Name</span>
      <Input
        value={draft}
        aria-invalid={problem !== null}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") {
            setDraft(region.name);
            setProblem(null);
            e.stopPropagation();
          }
        }}
      />
      {problem && <span className="text-red-600 dark:text-red-400">{problem}</span>}
    </label>
  );
}

type NodesRegion = Extract<Region, { kind: "nodes" }>;

/** A lines submodel: how its lines are laid out for effects, and each line's pixels. */
function LinesEditor({ prop, region, update }: { prop: Prop; region: NodesRegion; update: (next: NodesRegion) => Promise<boolean> }) {
  const buffer = BUFFERS.find((b) => b.value === region.buffer);
  return (
    <div className="mt-2 flex flex-col gap-2">
      <label className="flex flex-col gap-1 text-xs">
        <span className="text-neutral-500 dark:text-neutral-400">Lines are</span>
        <Select value={region.layout} onChange={(e) => void update({ ...region, layout: e.target.value as LineLayout })}>
          {LAYOUTS.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </Select>
      </label>
      <label className="flex flex-col gap-1 text-xs" title={buffer?.help}>
        <span className="text-neutral-500 dark:text-neutral-400">Effects see</span>
        <Select value={region.buffer} onChange={(e) => void update({ ...region, buffer: e.target.value as BufferStyle })}>
          {BUFFERS.map((b) => (
            <option key={b.value} value={b.value}>
              {b.label}
            </option>
          ))}
        </Select>
      </label>
      <p className="text-xs text-neutral-500">Pixel numbers from 1: ranges like 1-10 (or 10-1 to run backwards), single pixels like 15, and 0 for an empty spot.</p>
      {region.lines.map((line, i) => (
        <LineField
          // A removed line shifts the others, so each field is keyed by its text too.
          key={`${i}:${formatLine(line)}`}
          number={i + 1}
          line={line}
          prop={{ name: prop.name, count: nodeCount(prop.shape) }}
          canRemove={region.lines.length > 1}
          save={(next) => update({ ...region, lines: region.lines.map((l, k) => (k === i ? next : l)) })}
          remove={() => update({ ...region, lines: region.lines.filter((_, k) => k !== i) })}
        />
      ))}
      <Button onClick={() => void update({ ...region, lines: [...region.lines, []] })}>
        <Plus size={16} aria-hidden /> Add line
      </Button>
    </div>
  );
}

function LineField({
  number,
  line,
  prop,
  canRemove,
  save,
  remove,
}: {
  number: number;
  line: NodesRegion["lines"][number];
  prop: { name: string; count: number };
  canRemove: boolean;
  save: (line: NodesRegion["lines"][number]) => Promise<boolean>;
  remove: () => Promise<boolean>;
}) {
  const shown = formatLine(line);
  const [draft, setDraft] = useState(shown);
  const [problem, setProblem] = useState<string | null>(null);
  const commit = () => {
    if (draft === shown) return setProblem(null);
    const read = parseLine(draft, prop);
    if ("error" in read) return setProblem(read.error);
    setProblem(null);
    void save(read.line);
  };
  return (
    <div className="flex flex-col gap-1 text-xs">
      <div className="flex items-end gap-1">
        <label className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="text-neutral-500 dark:text-neutral-400">Line {number} pixels</span>
          <Input
            value={draft}
            placeholder="1-10, 15"
            aria-invalid={problem !== null}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              if (e.key === "Escape") {
                setDraft(shown);
                setProblem(null);
                e.stopPropagation();
              }
            }}
            className="w-full font-mono"
          />
        </label>
        {canRemove && (
          <button
            type="button"
            aria-label={`Remove line ${number}`}
            title={`Remove line ${number}`}
            onClick={() => void remove()}
            className="mb-1 rounded p-1 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
          >
            <X size={14} aria-hidden />
          </button>
        )}
      </div>
      {problem && <span className="text-red-600 dark:text-red-400">{problem}</span>}
    </div>
  );
}

type RectRegion = Extract<Region, { kind: "subBuffer" }>;

/** A rectangle submodel: its edges as percentages of the prop. */
function RectangleEditor({ region, update }: { region: RectRegion; update: (next: RectRegion) => Promise<boolean> }) {
  const edge = (key: "x1" | "y1" | "x2" | "y2", label: string) => (
    <NumberField label={label} value={region[key]} min={0} max={100} onCommit={(v) => void update({ ...region, [key]: v })} />
  );
  return (
    <div className="mt-2">
      <p className="mb-2 text-xs text-neutral-500">The pixels inside this part of the prop, in percent of its width and height.</p>
      <div className="grid grid-cols-2 gap-2">
        {edge("x1", "Left %")}
        {edge("x2", "Right %")}
        {edge("y1", "Bottom %")}
        {edge("y2", "Top %")}
      </div>
    </div>
  );
}

type FaceRegion = Extract<Region, { kind: "face" }>;

/** A face's parts and the pixels each uses, and a preview of each mouth shape on the canvas. */
function FaceDetails({ region, phoneme, show }: { region: FaceRegion; phoneme: Phoneme | null; show: (phoneme: Phoneme | null) => void }) {
  const rows: [string, string][] = [
    ...PHONEMES.filter((p) => (region.mouths[p.value] ?? []).length > 0).map((p): [string, string] => [`Mouth ${p.label}`, formatRanges(region.mouths[p.value] ?? [])]),
    ["Eyes open", formatRanges(region.eyesOpen)],
    ["Eyes closed", formatRanges(region.eyesClosed)],
    ["Outline", formatRanges(region.outline)],
  ];
  return (
    <div className="mt-2">
      <p className="mb-1 text-xs text-neutral-500">Show a mouth shape on the canvas:</p>
      <div role="group" aria-label="Mouth shape preview" className="flex flex-wrap gap-1">
        {PHONEMES.map((p) => (
          <button
            key={p.value}
            type="button"
            aria-pressed={phoneme === p.value}
            onClick={() => show(phoneme === p.value ? null : p.value)}
            className={`rounded border px-1.5 py-0.5 text-xs ${
              phoneme === p.value
                ? "border-accent-500 bg-accent-500/15 text-accent-700 dark:text-accent-300"
                : "border-neutral-300 hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-800"
            }`}
          >
            {p.label}
          </button>
        ))}
      </div>
      <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5 text-xs">
        {rows.map(([label, pixels]) => (
          <div key={label} className="contents">
            <dt className="text-neutral-500">{label}</dt>
            <dd className="truncate font-mono" title={pixels}>
              {pixels || "none"}
            </dd>
          </div>
        ))}
      </dl>
      <p className="mt-2 text-xs text-neutral-500">Faces come from xLights; their pixels can't be changed here yet. Put a Faces effect on this prop's row to make it sing.</p>
    </div>
  );
}
