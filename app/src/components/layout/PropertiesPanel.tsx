import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignHorizontalDistributeCenter,
  AlignStartHorizontal,
  AlignStartVertical,
  AlignVerticalDistributeCenter,
  Copy,
  ImagePlus,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import type { Background, ColorOrder, PreviewProp, Prop, ShapeSource, Show } from "../../api/types";
import { fileName, thousands } from "../../lib/format";
import { alignEdits, distributeEdits, duplicateEdits, removeEdits, updateEdits, wiringOf } from "../../lib/layoutEdits";
import { type Align, tidy } from "../../lib/layoutMath";
import { nodeCount, shapeLabel } from "../../lib/shows";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { Button, Input, Select } from "../ui";

const COLOR_ORDERS: ColorOrder[] = ["RGB", "RBG", "GRB", "GBR", "BRG", "BGR", "RGBW", "GRBW"];

interface ParamField {
  key: string;
  label: string;
  integer?: boolean;
  min: number;
}

const COUNT = (key: string, label: string, min = 1): ParamField => ({ key, label, integer: true, min });
const SIZE = (key: string, label: string, min = 0.01): ParamField => ({ key, label, min });

/** The size and pixel-count settings for each kind of generated prop. */
const SHAPE_FIELDS: Record<string, ParamField[]> = {
  line: [COUNT("nodes", "Pixels"), SIZE("length", "Length")],
  arch: [COUNT("nodes", "Pixels"), SIZE("width", "Width"), SIZE("height", "Height")],
  circle: [COUNT("nodes", "Pixels"), SIZE("radius", "Radius")],
  matrix: [COUNT("columns", "Columns"), COUNT("rows", "Rows"), SIZE("width", "Width"), SIZE("height", "Height")],
  tree: [
    COUNT("strings", "Strings"),
    COUNT("nodesPerString", "Pixels per string"),
    SIZE("height", "Height"),
    SIZE("baseRadius", "Base radius"),
    SIZE("topRadius", "Top radius", 0),
  ],
  star: [COUNT("points", "Points", 2), COUNT("nodes", "Pixels"), SIZE("outerRadius", "Outer radius"), SIZE("innerRadius", "Inner radius")],
};

/** A number box that saves when you press Enter or leave it, and goes back if what's typed isn't valid. */
export function NumberField({
  label,
  value,
  onCommit,
  min = -Infinity,
  integer = false,
  nonZero = false,
}: {
  label: string;
  value: number;
  onCommit: (value: number) => void;
  min?: number;
  integer?: boolean;
  /** Any number but zero (a negative scale mirrors the prop). */
  nonZero?: boolean;
}) {
  const shown = String(integer ? value : tidy(value));
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);
  const commit = () => {
    const n = Number(draft);
    if (draft.trim() === "" || !Number.isFinite(n) || n < min || (integer && !Number.isInteger(n)) || (nonZero && n === 0)) {
      setDraft(shown);
      return;
    }
    if (n !== Number(shown)) onCommit(n);
  };
  return (
    <label className="flex flex-col gap-1 text-xs">
      <span className="text-neutral-500 dark:text-neutral-400">{label}</span>
      <Input
        inputMode="decimal"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") {
            setDraft(shown);
            e.stopPropagation();
          }
        }}
        className="w-full tabular-nums"
      />
    </label>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="border-t border-neutral-200 py-3 first:border-t-0 first:pt-0 dark:border-neutral-800">
      <h3 className="mb-2 text-xs font-medium tracking-wide text-neutral-500 uppercase">{title}</h3>
      {children}
    </section>
  );
}

function OnePropPanel({ prop }: { prop: Prop }) {
  const apply = useApp((s) => s.apply);
  const show = useApp((s) => s.snapshot!.show);
  const setScreen = useApp((s) => s.setScreen);
  const clear = useLayoutEditor((s) => s.clear);
  const [name, setName] = useState(prop.name);
  useEffect(() => setName(prop.name), [prop.name]);
  // Each change applies to the prop as it is when the edit is sent, so it can't undo a move on its way.
  const update = (change: (p: Prop) => Prop) => void apply(updateEdits(prop.id, change));
  const t = prop.transform;
  const setTransform = (patch: Partial<{ x: number; y: number; rotation: number; sx: number; sy: number }>) =>
    update((p) => {
      const { position, rotationDeg, scale } = p.transform;
      return {
        ...p,
        transform: {
          position: { ...position, x: patch.x ?? position.x, y: patch.y ?? position.y },
          rotationDeg: { ...rotationDeg, z: patch.rotation ?? rotationDeg.z },
          scale: { ...scale, x: patch.sx ?? scale.x, y: patch.sy ?? scale.y },
        },
      };
    });
  const shape = prop.shape;
  const fields = shape.source === "generator" ? (SHAPE_FIELDS[shape.type] ?? []) : [];
  const wiring = wiringOf(show, prop.id);
  const commitName = () => {
    const trimmed = name.trim();
    if (trimmed && trimmed !== prop.name) update((p) => ({ ...p, name: trimmed }));
    else setName(prop.name);
  };

  return (
    <div>
      <Section title="Prop">
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-neutral-500 dark:text-neutral-400">Name</span>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onBlur={commitName}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
          />
        </label>
        <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-400">
          {shapeLabel(shape)} · {thousands(nodeCount(shape))} pixels
        </p>
      </Section>
      <Section title="Size and pixels">
        {fields.length > 0 ? (
          <div className="grid grid-cols-2 gap-2">
            {fields.map((f) => (
              <NumberField
                key={f.key}
                label={f.label}
                value={(shape as unknown as Record<string, number>)[f.key]}
                min={f.min}
                integer={f.integer}
                onCommit={(v) => update((p) => ({ ...p, shape: { ...p.shape, [f.key]: v } as ShapeSource }))}
              />
            ))}
          </div>
        ) : (
          <p className="text-sm text-neutral-500">
            {shape.source === "measured"
              ? "This prop's pixels were placed one by one (imported), so its size is changed by resizing it on the canvas."
              : "This custom grid's pixels can't be changed here yet."}
          </p>
        )}
        <label className="mt-2 flex flex-col gap-1 text-xs">
          <span className="text-neutral-500 dark:text-neutral-400">Color order</span>
          <Select
            value={prop.colorOrder}
            onChange={(e) => {
              const colorOrder = e.target.value as ColorOrder;
              update((p) => ({ ...p, colorOrder }));
            }}
          >
            {COLOR_ORDERS.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </Select>
        </label>
      </Section>
      <Section title="Placement">
        <div className="grid grid-cols-2 gap-2">
          <NumberField label="Position X" value={t.position.x} onCommit={(x) => setTransform({ x })} />
          <NumberField label="Position Y" value={t.position.y} onCommit={(y) => setTransform({ y })} />
          <NumberField label="Rotation (degrees)" value={t.rotationDeg.z} onCommit={(rotation) => setTransform({ rotation })} />
          <div />
          <NumberField label="Scale X" value={t.scale.x} nonZero onCommit={(sx) => setTransform({ sx })} />
          <NumberField label="Scale Y" value={t.scale.y} nonZero onCommit={(sy) => setTransform({ sy })} />
        </div>
      </Section>
      <Section title="Wiring">
        {wiring.length > 0 ? (
          <ul className="text-sm">
            {wiring.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-neutral-500">
            Not wired.{" "}
            <button type="button" className="text-accent-500 underline" onClick={() => setScreen("wiring")}>
              Wire it to a controller
            </button>
          </p>
        )}
      </Section>
      <Section title="Actions">
        <div className="flex flex-wrap gap-2">
          <DuplicateButton ids={[prop.id]} />
          <Button
            variant="danger"
            onClick={async () => {
              if (await apply(removeEdits([prop.id]))) clear();
            }}
          >
            <Trash2 size={16} aria-hidden /> Delete prop
          </Button>
        </div>
      </Section>
    </div>
  );
}

function DuplicateButton({ ids }: { ids: string[] }) {
  const apply = useApp((s) => s.apply);
  const select = useLayoutEditor((s) => s.select);
  return (
    <Button
      title="Duplicate (⌘D)"
      onClick={async () => {
        let copies: string[] = [];
        const duplicate = (show: Show) => {
          const made = duplicateEdits(show, ids);
          copies = made.ids;
          return made.edits;
        };
        if (await apply(duplicate)) select(copies);
      }}
    >
      <Copy size={16} aria-hidden /> Duplicate
    </Button>
  );
}

const ALIGNS: { how: Align; label: string; icon: LucideIcon }[] = [
  { how: "left", label: "Align left edges", icon: AlignStartVertical },
  { how: "center", label: "Align centers left to right", icon: AlignCenterVertical },
  { how: "right", label: "Align right edges", icon: AlignEndVertical },
  { how: "top", label: "Align top edges", icon: AlignStartHorizontal },
  { how: "middle", label: "Align middles top to bottom", icon: AlignCenterHorizontal },
  { how: "bottom", label: "Align bottom edges", icon: AlignEndHorizontal },
];

function ManyPropsPanel({ ids, preview }: { ids: string[]; preview: PreviewProp[] }) {
  const apply = useApp((s) => s.apply);
  const clear = useLayoutEditor((s) => s.clear);
  const iconButton = "rounded-md border border-neutral-300 p-1.5 hover:bg-neutral-100 disabled:opacity-40 dark:border-neutral-700 dark:hover:bg-neutral-800";
  return (
    <div>
      <Section title="Selection">
        <p className="text-sm">{ids.length} props selected</p>
      </Section>
      <Section title="Line up">
        <div className="flex flex-wrap gap-1">
          {ALIGNS.map(({ how, label, icon: Icon }) => (
            <button key={how} type="button" aria-label={label} title={label} className={iconButton} onClick={() => void apply((show) => alignEdits(show, preview, ids, how))}>
              <Icon size={16} aria-hidden />
            </button>
          ))}
        </div>
        <div className="mt-2 flex gap-1">
          <button
            type="button"
            aria-label="Space evenly left to right"
            title={ids.length < 3 ? "Select three or more props to space them evenly" : "Space evenly left to right"}
            disabled={ids.length < 3}
            className={iconButton}
            onClick={() => void apply((show) => distributeEdits(show, preview, ids, "horizontal"))}
          >
            <AlignHorizontalDistributeCenter size={16} aria-hidden />
          </button>
          <button
            type="button"
            aria-label="Space evenly bottom to top"
            title={ids.length < 3 ? "Select three or more props to space them evenly" : "Space evenly bottom to top"}
            disabled={ids.length < 3}
            className={iconButton}
            onClick={() => void apply((show) => distributeEdits(show, preview, ids, "vertical"))}
          >
            <AlignVerticalDistributeCenter size={16} aria-hidden />
          </button>
        </div>
      </Section>
      <Section title="Actions">
        <div className="flex flex-wrap gap-2">
          <DuplicateButton ids={ids} />
          <Button
            variant="danger"
            onClick={async () => {
              if (await apply(removeEdits(ids))) clear();
            }}
          >
            <Trash2 size={16} aria-hidden /> Delete {ids.length} props
          </Button>
        </div>
      </Section>
    </div>
  );
}

function PhotoPanel({ problem, onRetry, onChoosePhoto }: { problem: string | null; onRetry: () => void; onChoosePhoto: () => void }) {
  const apply = useApp((s) => s.apply);
  const background = useApp((s) => s.snapshot!.show.background ?? null);
  const { editPhoto, setEditPhoto, photoDraft, setPhotoDraft } = useLayoutEditor(
    useShallow((s) => ({ editPhoto: s.editPhoto, setEditPhoto: s.setEditPhoto, photoDraft: s.photoDraft, setPhotoDraft: s.setPhotoDraft })),
  );
  /** The draft last sent: releasing the slider, a key, and leaving it can all fire for one change. */
  const sent = useRef<Background | null>(null);
  if (!background) {
    return (
      <Section title="Background photo">
        <p className="mb-2 text-sm text-neutral-500">Add a photo of your house, then draw your props right over it.</p>
        <Button onClick={onChoosePhoto}>
          <ImagePlus size={16} aria-hidden /> Choose photo…
        </Button>
      </Section>
    );
  }
  const strength = Math.round((photoDraft ?? background).opacity * 100);
  const slide = (percent: number) => setPhotoDraft({ ...(photoDraft ?? background), opacity: percent / 100 });
  const commit = () => {
    const draft: Background | null = useLayoutEditor.getState().photoDraft;
    if (!draft || draft === sent.current) return;
    if (draft.opacity === background.opacity) return setPhotoDraft(null);
    sent.current = draft;
    const { opacity } = draft;
    void apply((show) => (show.background ? [{ type: "setBackground", background: { ...show.background, opacity } }] : [])).then(() => {
      // Keep a newer slide that started meanwhile.
      if (useLayoutEditor.getState().photoDraft === draft) setPhotoDraft(null);
    });
  };
  return (
    <Section title="Background photo">
      <p className="mb-2 truncate text-sm" title={background.path}>
        {fileName(background.path)}
      </p>
      {problem && (
        <div className="mb-2 text-sm text-red-600 dark:text-red-400">
          <p>{problem}</p>
          <button type="button" className="mt-1 text-accent-500 underline" onClick={onRetry}>
            Try again
          </button>
        </div>
      )}
      <label className="flex flex-col gap-1 text-xs">
        <span className="text-neutral-500 dark:text-neutral-400">Photo strength: {strength}%</span>
        <input
          type="range"
          min={0}
          max={100}
          value={strength}
          aria-label="Photo strength"
          onChange={(e) => slide(Number(e.target.value))}
          onPointerUp={commit}
          onKeyUp={commit}
          onBlur={commit}
          className="accent-accent-500"
        />
      </label>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button aria-pressed={editPhoto} onClick={() => setEditPhoto(!editPhoto)}>
          {editPhoto ? "Done moving photo" : "Move or resize photo"}
        </Button>
        <Button onClick={onChoosePhoto}>Replace…</Button>
        <Button variant="danger" onClick={() => void apply([{ type: "setBackground", background: null }])}>
          Remove photo
        </Button>
      </div>
    </Section>
  );
}

/** Details of what's selected, or the photo settings and tips when nothing is. */
export function PropertiesPanel({
  preview,
  photoProblem,
  onRetryPhoto,
  onChoosePhoto,
}: {
  preview: PreviewProp[];
  photoProblem: string | null;
  onRetryPhoto: () => void;
  onChoosePhoto: () => void;
}) {
  const show = useApp((s) => s.snapshot?.show);
  const selected = useLayoutEditor((s) => s.selected);
  if (!show) return null;
  const ids = selected.filter((id) => show.props.some((p) => p.id === id));
  return (
    <aside aria-label="Properties" className="h-full overflow-auto rounded-lg border border-neutral-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-900">
      {ids.length === 1 ? (
        <OnePropPanel key={ids[0]} prop={show.props.find((p) => p.id === ids[0])!} />
      ) : ids.length > 1 ? (
        <ManyPropsPanel ids={ids} preview={preview} />
      ) : (
        <div>
          <PhotoPanel problem={photoProblem} onRetry={onRetryPhoto} onChoosePhoto={onChoosePhoto} />
          <Section title="Tips">
            <ul className="list-disc space-y-1 pl-4 text-sm text-neutral-600 dark:text-neutral-400">
              <li>Pick a tool above and drag on the canvas to draw a prop.</li>
              <li>Click a prop to select it; shift-click or drag a box to select more.</li>
              <li>Drag corners to resize, the round handle to turn. Hold Shift for free stretching or 15° steps.</li>
              <li>Arrow keys nudge, ⌘D duplicates, Delete removes, ⌘Z undoes.</li>
              <li>Scroll or Space-drag to move around; pinch or hold ⌘ and scroll to zoom.</li>
            </ul>
          </Section>
        </div>
      )}
    </aside>
  );
}
