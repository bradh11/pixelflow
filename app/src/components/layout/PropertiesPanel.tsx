import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignHorizontalDistributeCenter,
  AlignStartHorizontal,
  AlignStartVertical,
  AlignVerticalDistributeCenter,
  Cable,
  Copy,
  ImagePlus,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import type { Background, ColorOrder, FileRole, PreviewProp, Prop, ShapeSource, Show } from "../../api/types";
import { fileName, shownPath, thousands } from "../../lib/format";
import { alignEdits, distributeEdits, duplicateEdits, portsWithPropsAfter, removeEdits, updateEdits, wiringOf } from "../../lib/layoutEdits";
import { type Align, tidy } from "../../lib/layoutMath";
import { nodeCount, shapeLabel } from "../../lib/shows";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { useView3d } from "../../state/view3d";
import { HouseModelPanel } from "../layout3d/HouseModelPanel";
import { deleteProps } from "./PropsList";
import { SubmodelsSection } from "./SubmodelsSection";
import { AddBendButton, JoinLines, PolyLineSection } from "./PolyLineSection";
import { CustomGridSection } from "./CustomGridSection";
import { isPoly } from "../../lib/polylineMath";
import { SHAPE_FIELDS, type ShapeField, fieldValue, parseNumbers, withField } from "./shapeFields";
import { MissingFileNotice, useMissingFile } from "../MissingFiles";
import { Button, Input, More, Select } from "../ui";

const COLOR_ORDERS: ColorOrder[] = ["RGB", "RBG", "GRB", "GBR", "BRG", "BGR", "RGBW", "GRBW"];

/** Settings a shape may leave out (an older matrix has no wiring of its own), and what they read as. */
const COMMON_DEFAULTS: Record<string, unknown> = {
  wiring: { start: "bottomLeft", orientation: "horizontal", serpentine: true },
  degrees: 360,
  startAngle: 0,
};
const TYPE_DEFAULTS: Record<string, Record<string, unknown>> = {
  arch: { arches: 1, arc: 180, gap: 0, skewDeg: 0, hollow: 70, startRight: false, zigZag: false, startInside: false },
  circle: { innerPercent: 50, startInside: false, startAtBottom: false, counterClockwise: false },
  star: { start: "top", counterClockwise: false, innerPercent: 50, startInside: false },
  tree: { start: "bottomLeft", strandsPerString: 0, alternateNodes: false, spiralRotations: 0 },
};
const shapeDefaults = (shape: ShapeSource): Record<string, unknown> => ({
  ...COMMON_DEFAULTS,
  ...(shape.source === "generator" ? TYPE_DEFAULTS[shape.type] : undefined),
});

/** `shape` with one setting changed. Giving an arch, circle or star its layers makes its pixel
 * count theirs, as xLights does, so no pixels are left over in the middle; clearing an arch's
 * layers leaves one arch of those pixels. */
function withSetting(shape: ShapeSource, key: string, value: unknown): ShapeSource {
  const next = withField(shape, key, value, shapeDefaults(shape));
  if (key !== "layers" || !Array.isArray(value) || next.source !== "generator" || !("nodes" in next)) return next;
  if (value.length > 0) return { ...next, nodes: (value as number[]).reduce((a, b) => a + b, 0) };
  return next.type === "arch" ? { ...next, arches: 1 } : next;
}

/** A shape's settings: numbers two to a row, then choices, lists, and checkboxes one to a row. */
function ShapeFields({ fields, shape, onChange }: { fields: ShapeField[]; shape: ShapeSource; onChange: (key: string, value: unknown) => void }) {
  const defaults = shapeDefaults(shape);
  const value = (key: string) => fieldValue(shape, key) ?? fieldValue(defaults, key);
  const shown = fields.filter((f) => !f.showIf || f.showIf(shape as unknown as Record<string, unknown>));
  const numbers = shown.filter((f) => f.kind === "number");
  const others = shown.filter((f) => f.kind !== "number");
  return (
    <>
      <div className="grid grid-cols-2 gap-2">
        {numbers.map((f) => (
          <NumberField
            key={f.key}
            label={f.label}
            hint={f.hint}
            value={Number(value(f.key) ?? 0)}
            min={f.min}
            max={f.max}
            integer={f.integer}
            onCommit={(v) => onChange(f.key, v)}
          />
        ))}
      </div>
      {others.map((f) =>
        f.kind === "bool" ? (
          <label key={f.key} className="mt-2 flex items-center gap-2 text-sm" title={f.hint}>
            <input
              type="checkbox"
              checked={value(f.key) === true}
              onChange={(e) => {
                const on = e.target.checked;
                onChange(f.key, on);
              }}
            />
            {f.label}
          </label>
        ) : f.kind === "choice" ? (
          <label key={f.key} className="mt-2 flex flex-col gap-1 text-xs" title={f.hint}>
            <span className="text-neutral-500 dark:text-neutral-400">{f.label}</span>
            <Select value={String(value(f.key))} onChange={(e) => onChange(f.key, e.target.value)}>
              {f.options.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </label>
        ) : f.kind === "numbers" ? (
          <div key={f.key} className="mt-2">
            <ListField
              label={f.label}
              hint={f.hint}
              value={(value(f.key) as number[] | undefined) ?? []}
              min={f.min}
              max={f.max}
              allowEmpty={f.allowEmpty}
              onCommit={(v) => onChange(f.key, v)}
            />
          </div>
        ) : null,
      )}
    </>
  );
}

/** A comma list of whole numbers ("3,4,5,4"), saved on Enter or leaving it; goes back if it isn't valid. */
function ListField({
  label,
  hint,
  value,
  min,
  max,
  allowEmpty,
  onCommit,
}: {
  label: string;
  hint?: string;
  value: number[];
  min: number;
  max?: number;
  allowEmpty?: boolean;
  onCommit: (v: number[]) => void;
}) {
  const shown = value.join(",");
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);
  const commit = () => {
    const nums = parseNumbers(draft, min, max, allowEmpty);
    if (!nums) return setDraft(shown);
    if (nums.join(",") !== shown) onCommit(nums);
  };
  return (
    <label className="flex flex-col gap-1 text-xs" title={hint}>
      <span className="text-neutral-500 dark:text-neutral-400">{label}</span>
      <Input
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
      />
    </label>
  );
}

/** A number box that saves when you press Enter or leave it, and goes back if what's typed isn't valid. */
export function NumberField({
  label,
  value,
  onCommit,
  min = -Infinity,
  max = Infinity,
  integer = false,
  nonZero = false,
  hint,
}: {
  label: string;
  /** More about the field, shown on hover. */
  hint?: string;
  value: number;
  onCommit: (value: number) => void;
  min?: number;
  max?: number;
  integer?: boolean;
  /** Any number but zero (a negative scale mirrors the prop). */
  nonZero?: boolean;
}) {
  const shown = String(integer ? value : tidy(value));
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);
  const commit = () => {
    const n = Number(draft);
    if (draft.trim() === "" || !Number.isFinite(n) || n < min || n > max || (integer && !Number.isInteger(n)) || (nonZero && n === 0)) {
      setDraft(shown);
      return;
    }
    if (n !== Number(shown)) onCommit(n);
  };
  return (
    <label className="flex flex-col gap-1 text-xs" title={hint}>
      <span className="truncate text-neutral-500 dark:text-neutral-400">{label}</span>
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

export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section aria-label={title} className="border-t border-neutral-200 py-3 first:border-t-0 first:pt-0 dark:border-neutral-800">
      <h3 className="mb-2 text-xs font-medium tracking-wide text-neutral-500 uppercase">{title}</h3>
      {children}
    </section>
  );
}

function OnePropPanel({ prop, points }: { prop: Prop; points: ArrayLike<number> | undefined }) {
  const apply = useApp((s) => s.apply);
  const show = useApp((s) => s.snapshot!.show);
  const setScreen = useApp((s) => s.setScreen);
  const [name, setName] = useState(prop.name);
  useEffect(() => setName(prop.name), [prop.name]);
  // Each change applies to the prop as it is when the edit is sent, so it can't undo a move on its way.
  const update = (change: (p: Prop) => Prop) => void apply(updateEdits(prop.id, change));
  const t = prop.transform;
  const setTransform = (patch: Partial<{ x: number; y: number; z: number; rotation: number; tilt: number; turn: number; sx: number; sy: number }>) =>
    update((p) => {
      const { position, rotationDeg, scale } = p.transform;
      return {
        ...p,
        transform: {
          position: { x: patch.x ?? position.x, y: patch.y ?? position.y, z: patch.z ?? position.z },
          rotationDeg: { x: patch.tilt ?? rotationDeg.x, y: patch.turn ?? rotationDeg.y, z: patch.rotation ?? rotationDeg.z },
          scale: { ...scale, x: patch.sx ?? scale.x, y: patch.sy ?? scale.y },
        },
      };
    });
  const shape = prop.shape;
  const fields = shape.source === "generator" ? (SHAPE_FIELDS[shape.type] ?? []) : [];
  const wiring = wiringOf(show, prop.id);
  // A shape edit that changes how many pixels a wired prop has moves the props after it.
  const [pixelNote, setPixelNote] = useState<string | null>(null);
  useEffect(() => setPixelNote(null), [prop.id]);
  const setShapeField = (key: string, v: unknown) => {
    const [before, after] = [nodeCount(prop.shape), nodeCount(withSetting(prop.shape, key, v))];
    const ports = portsWithPropsAfter(show, prop.id);
    setPixelNote(
      before !== after && ports.length > 0
        ? `This changes ${prop.name} from ${thousands(before)} to ${thousands(after)} pixels; props after it on ${ports.join(" and ")} move.`
        : null,
    );
    update((p) => ({ ...p, shape: withSetting(p.shape, key, v) }));
  };
  const in3d = useView3d((s) => s.mode === "3d");
  // In 2D, depth, tilt, and turn show only when set, so a prop that looks squashed says why.
  const showZ = in3d || t.position.z !== 0;
  const showTilt = in3d || t.rotationDeg.x !== 0;
  const showTurn = in3d || t.rotationDeg.y !== 0;
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
        {/* Where it's wired, up top: it's the next thing to check after placing a prop. */}
        <p className="mt-1 flex flex-wrap items-center gap-x-1.5 text-sm" data-testid="prop-wiring">
          <Cable size={13} aria-hidden className={wiring.length > 0 ? "text-emerald-600 dark:text-emerald-400" : "text-amber-600 dark:text-amber-400"} />
          {wiring.length > 0 ? (
            wiring.map((w, i) => (
              <span key={w} className="text-neutral-700 dark:text-neutral-300">
                {i > 0 && "· "}
                <span>{w}</span>
              </span>
            ))
          ) : (
            <span className="text-amber-700 dark:text-amber-400">
              Not wired.{" "}
              <button type="button" className="text-accent-600 underline dark:text-accent-400" onClick={() => setScreen("wiring")}>
                Wire it to a controller
              </button>
            </span>
          )}
        </p>
      </Section>
      {isPoly(shape) && <PolyLineSection prop={prop} shape={shape} />}
      {shape.source === "generator" && shape.type === "customGrid" && <CustomGridSection prop={prop} shape={shape} />}
      {(shape.source === "measured" || fields.length > 0) && (
        <Section title="Size and pixels">
          {shape.source === "measured" ? (
            <p className="text-sm text-neutral-500">
              This prop's pixels were placed one by one (imported), so its size is changed by resizing it on the canvas.
            </p>
          ) : fields.length > 0 ? (
            <>
              <ShapeFields fields={fields} shape={shape} onChange={setShapeField} />
              {pixelNote && (
                <p role="status" className="mt-2 text-xs text-amber-700 dark:text-amber-400">
                  {pixelNote}
                </p>
              )}
            </>
          ) : null}
        </Section>
      )}
      <Section title="Placement">
        {/* Depth, tilt, and turn only mean something in the 3D view, so they're shown there. */}
        <div className={`grid gap-2 ${showZ ? "grid-cols-3" : "grid-cols-2"}`}>
          <NumberField label="Position X" value={t.position.x} onCommit={(x) => setTransform({ x })} />
          <NumberField label="Position Y" value={t.position.y} onCommit={(y) => setTransform({ y })} />
          {showZ && <NumberField label="Position Z" hint="Depth: toward the street" value={t.position.z} onCommit={(z) => setTransform({ z })} />}
        </div>
        <div className={`mt-2 grid gap-2 ${showTilt || showTurn ? "grid-cols-3" : "grid-cols-2"}`}>
          <NumberField label="Rotation°" hint="Turned in the front view (around Z), in degrees" value={t.rotationDeg.z} onCommit={(rotation) => setTransform({ rotation })} />
          {showTilt && <NumberField label="Tilt (X°)" hint="Tipped forward or back (around X)" value={t.rotationDeg.x} onCommit={(tilt) => setTransform({ tilt })} />}
          {showTurn && <NumberField label="Turn (Y°)" hint="Turned to face left or right (around Y)" value={t.rotationDeg.y} onCommit={(turn) => setTransform({ turn })} />}
        </div>
        {!(showZ && showTilt && showTurn) && <p className="mt-1 text-xs text-neutral-500">Depth, tilt, and turn are in the 3D view (V).</p>}
        <More id="layout-prop" label="More: color order and scale">
          <label className="flex flex-col gap-1 text-xs">
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
          <div className="mt-2 grid grid-cols-2 gap-2">
            <NumberField label="Scale X" value={t.scale.x} nonZero onCommit={(sx) => setTransform({ sx })} />
            <NumberField label="Scale Y" value={t.scale.y} nonZero onCommit={(sy) => setTransform({ sy })} />
          </div>
        </More>
      </Section>
      <SubmodelsSection prop={prop} points={points} />
      <Section title="Actions">
        <div className="flex flex-wrap gap-2">
          <AddBendButton prop={prop} />
          <DuplicateButton ids={[prop.id]} />
          <Button variant="danger" title="Delete this prop (Undo brings it back)" onClick={() => void deleteProps([prop.id], [prop.name])}>
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
      <JoinLines ids={ids} />
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

const PHOTO: FileRole = { kind: "photo" };

/**
 * The background photo's settings (and in 3D, its depth and the house model): in the tool bar's
 * Photo menu, and in the properties panel when it's kept open with nothing selected.
 */
export function PhotoControls({
  preview,
  problem,
  onRetry,
  onChoosePhoto,
}: {
  preview: PreviewProp[];
  problem: string | null;
  onRetry: () => void;
  onChoosePhoto: () => void;
}) {
  const in3d = useView3d((s) => s.mode === "3d");
  const hasPhoto = useApp((s) => Boolean(s.snapshot?.show.background));
  return (
    <div>
      <PhotoPanel problem={problem} onRetry={onRetry} onChoosePhoto={onChoosePhoto} />
      {in3d && hasPhoto && <PhotoDepth />}
      {in3d && <HouseModelPanel preview={preview} />}
    </div>
  );
}

function PhotoPanel({ problem, onRetry, onChoosePhoto }: { problem: string | null; onRetry: () => void; onChoosePhoto: () => void }) {
  const in3d = useView3d((s) => s.mode === "3d");
  const apply = useApp((s) => s.apply);
  const background = useApp((s) => s.snapshot!.show.background ?? null);
  const missing = useMissingFile(PHOTO);
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
      <p className="mb-2 truncate text-sm" title={shownPath(background.path)}>
        {fileName(background.path)}
      </p>
      {missing ? (
        <div className="mb-2">
          <MissingFileNotice missing={missing} />
        </div>
      ) : problem && (
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
        <Button
          aria-pressed={editPhoto}
          aria-disabled={in3d || undefined}
          title={in3d ? "Move the photo in the 2D view — switch with V" : "Drag the photo to move it, or its corners to resize it"}
          onClick={in3d ? undefined : () => setEditPhoto(!editPhoto)}
        >
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

/** How far behind the props the photo stands in 3D (remembered on this computer). */
function PhotoDepth() {
  const depth = useView3d((s) => s.photoDepth);
  const setDepth = useView3d((s) => s.setPhotoDepth);
  return (
    <label className="mt-3 flex flex-col gap-1 text-xs">
      <span className="text-neutral-500 dark:text-neutral-400">Photo depth in 3D: {tidy(depth)} behind the props</span>
      <input
        type="range"
        min={0}
        max={20}
        step={0.05}
        value={depth}
        aria-label="Photo depth"
        onChange={(e) => setDepth(Number(e.target.value))}
        className="accent-accent-500"
      />
    </label>
  );
}

export const TIPS_2D = [
  "Pick a tool above and drag on the canvas to draw a prop.",
  "Click a prop to select it; shift-click or Shift-drag a box to select more.",
  "Drag corners to resize, the round handle to turn. Hold Shift for free stretching or 15° steps.",
  "Arrow keys nudge, ⌘D duplicates, Delete removes, ⌘Z undoes.",
  "Drag empty space or scroll to move around; pinch or hold ⌘ and scroll to zoom.",
];

export const TIPS_3D = [
  "Drag to orbit, right-drag or Space-drag to pan, scroll or pinch to zoom. Double-click a prop to zoom to it.",
  "1–5 pick the Front, Top, Left, Right, and Street views; F fits everything in.",
  "Click a prop to select it; shift-click or Shift-drag a box to select more.",
  "Drag the arrows to move along one direction, or the squares across a plane. Drag a prop itself to slide it over the ground.",
  "With a house model, a dragged prop sticks to its walls and roof; hold Alt to drag it freely.",
  "Set depth and tilt exactly under Placement. V switches back to 2D for drawing.",
];

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
        <OnePropPanel key={ids[0]} prop={show.props.find((p) => p.id === ids[0])!} points={preview.find((p) => p.prop === ids[0])?.points} />
      ) : ids.length > 1 ? (
        <ManyPropsPanel ids={ids} preview={preview} />
      ) : (
        <div>
          <p className="mb-3 pr-6 text-sm text-neutral-500">Nothing selected. Click a prop to see its properties here.</p>
          <PhotoControls preview={preview} problem={photoProblem} onRetry={onRetryPhoto} onChoosePhoto={onChoosePhoto} />
        </div>
      )}
    </aside>
  );
}
