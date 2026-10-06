import { Merge, Scissors, Spline } from "lucide-react";
import { useState } from "react";
import type { Prop, Show } from "../../api/types";
import { updateEdits, wiringOf } from "../../lib/layoutEdits";
import { tidy } from "../../lib/layoutMath";
import {
  type PolyShape,
  addBend,
  bendSegment,
  joinLines,
  joinable,
  removeVertex,
  segmentLengths,
  setSegmentNodes,
  setSpread,
  splitAt,
  straighten,
} from "../../lib/polylineMath";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { NumberField, Section } from "./PropertiesPanel";

/** How close (layout units) the ends of two lines must be to join them. */
export const JOIN_TOLERANCE = 0.15;

/** The first free name "Roof (2)", "Roof (3)", … for the part split off `name`. */
function partName(name: string, show: Show): string {
  const taken = new Set(show.props.map((p) => p.name));
  for (let n = 2; ; n++) if (!taken.has(`${name} (${n})`)) return `${name} (${n})`;
}

/**
 * A poly line's stretches and pixels: each stretch's pixel count and whether it curves, or
 * one count spread evenly over the whole line; and splitting it at the picked point.
 */
export function PolyLineSection({ prop, shape }: { prop: Prop; shape: PolyShape }) {
  const apply = useApp((s) => s.apply);
  const polyPoint = useLayoutEditor((s) => s.polyPoint);
  const select = useLayoutEditor((s) => s.select);
  const setPolyPoint = useLayoutEditor((s) => s.setPolyPoint);
  const reshape = (change: (s: PolyShape) => PolyShape) =>
    apply(updateEdits(prop.id, (p) => (p.shape.source === "generator" && p.shape.type === "polyLine" ? { ...p, shape: change(p.shape) } : p)));
  const spread = shape.spreadNodes != null;
  const lengths = segmentLengths(shape);
  const picked = polyPoint?.prop === prop.id && polyPoint.index < shape.vertices.length ? polyPoint.index : null;
  const inside = picked !== null && picked > 0 && picked < shape.vertices.length - 1;

  const split = async () => {
    if (picked === null) return;
    let made: string | null = null;
    const ok = await apply((show) => {
      const now = show.props.find((p) => p.id === prop.id);
      const parts = now && splitAt(now, picked, crypto.randomUUID(), partName(now.name, show));
      if (!parts) return [];
      made = parts[1].id;
      return [
        { type: "updateProp", prop: parts[0] },
        { type: "addProp", prop: parts[1] },
      ];
    });
    if (ok && made) select([prop.id, made]);
  };

  return (
    <Section title="Points and pixels">
      <p className="mb-2 text-sm text-neutral-600 dark:text-neutral-400">
        {shape.vertices.length} points. Drag them on the canvas; click the plus in the middle of a stretch to add a point, or drag it to curve the
        stretch.
      </p>
      <label className="mb-2 flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={spread}
          onChange={(e) => {
            // Read now: the edit is built later, after React has put the box back.
            const on = e.target.checked;
            void reshape((s) => setSpread(s, on));
          }}
        />
        Spread the pixels evenly along the whole line
      </label>
      {spread ? (
        <NumberField label="Pixels" integer min={1} value={shape.spreadNodes ?? 0} onCommit={(n) => void reshape((s) => ({ ...s, spreadNodes: n }))} />
      ) : (
        <ol aria-label="Stretches" className="flex flex-col gap-2">
          {shape.segments.map((seg, k) => (
            <li key={k} className="grid grid-cols-[1fr_auto] items-end gap-2">
              <NumberField
                label={`Stretch ${k + 1} pixels (${tidy(lengths[k] ?? 0)} long)`}
                integer
                min={0}
                value={seg.nodes}
                onCommit={(n) => void reshape((s) => setSegmentNodes(s, k, n))}
              />
              <Button
                aria-label={seg.curve ? `Straighten stretch ${k + 1}` : `Curve stretch ${k + 1}`}
                title={seg.curve ? "Make this stretch straight again" : "Bend this stretch into a curve (then drag its handles)"}
                onClick={() =>
                  void reshape((s) => {
                    if (s.segments[k]?.curve) return straighten(s, k);
                    const [a, b] = [s.vertices[k], s.vertices[k + 1]];
                    // Bulge to the left of the stretch by a quarter of its length.
                    const [dx, dy] = [b.x - a.x, b.y - a.y];
                    const mid = { x: (a.x + b.x) / 2 - dy / 4, y: (a.y + b.y) / 2 + dx / 4, z: (a.z + b.z) / 2 };
                    return bendSegment(s, k, mid);
                  })
                }
              >
                {seg.curve ? "Straighten" : "Curve"}
              </Button>
            </li>
          ))}
        </ol>
      )}
      {picked !== null && (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <span className="text-sm">Point {picked + 1} picked</span>
          {inside && (
            <Button title="Cut the line in two at this point" onClick={() => void split()}>
              <Scissors size={16} aria-hidden /> Split here
            </Button>
          )}
          <Button
            disabled={shape.vertices.length <= 2}
            onClick={() => {
              setPolyPoint(null);
              void reshape((s) => removeVertex(s, picked) ?? s);
            }}
          >
            Remove point
          </Button>
        </div>
      )}
    </Section>
  );
}

/** For a straight line: turn it into a poly line with a bend in the middle. */
export function AddBendButton({ prop }: { prop: Prop }) {
  const apply = useApp((s) => s.apply);
  const setPolyPoint = useLayoutEditor((s) => s.setPolyPoint);
  if (!joinable(prop) || prop.shape.source !== "generator" || prop.shape.type !== "line") return null;
  return (
    <Button
      title="Turn this line into a poly line with a point in the middle you can drag"
      onClick={async () => {
        if (await apply(updateEdits(prop.id, addBend))) setPolyPoint({ prop: prop.id, index: 1 });
      }}
    >
      <Spline size={16} aria-hidden /> Add bend
    </Button>
  );
}

/**
 * For two selected lines whose ends touch: join them into one poly line (one undo step). The
 * line whose start becomes the joined line's start keeps its wiring unless the other is picked;
 * both lines' submodels and faces follow their pixels. What happens is said before the click.
 */
export function JoinLines({ ids }: { ids: string[] }) {
  const apply = useApp((s) => s.apply);
  const show = useApp((s) => s.snapshot?.show);
  const select = useLayoutEditor((s) => s.select);
  const [keep, setKeep] = useState<string | null>(null);
  if (!show || ids.length !== 2) return null;
  const [a, b] = ids.map((id) => show.props.find((p) => p.id === id));
  if (!a || !b || !joinable(a) || !joinable(b)) return null;
  const chosen = keep === a.id || keep === b.id ? keep : undefined;
  const join = joinLines(a, b, JOIN_TOLERANCE, chosen);
  if (!join) {
    return (
      <Section title="Join">
        <p className="text-sm text-neutral-500">These two lines can be joined into one poly line once an end of one touches an end of the other.</p>
      </Section>
    );
  }
  const name = (id: string | null) => (id === a.id ? a.name : b.name);
  const kept = name(join.kept);
  const notes: string[] = [];
  if (join.reversed) notes.push(`${name(join.reversed)} will run backwards, from its far end, in the joined line.`);
  if (join.kept !== join.first && wiringOf(show, join.kept).length > 0) {
    notes.push(`The joined line starts at ${name(join.first)}'s start, so ${kept}'s controller port feeds it from there.`);
  }
  if (wiringOf(show, join.removed).length > 0) notes.push(`${name(join.removed)}'s own wiring is removed; the joined line keeps ${kept}'s.`);
  if (join.dropped.length > 0) notes.push(`These can't move with their pixels and are left out: ${join.dropped.join(", ")}.`);
  const regions = a.regions.length + b.regions.length;
  return (
    <Section title="Join">
      <fieldset className="mb-2 text-sm">
        <legend className="mb-1 text-xs text-neutral-500 dark:text-neutral-400">Keep the name and wiring of</legend>
        {[a, b].map((p) => (
          <label key={p.id} className="mr-4 inline-flex items-center gap-1.5">
            <input type="radio" name="join-keep" checked={join.kept === p.id} onChange={() => setKeep(p.id)} />
            {p.name}
            {p.id === join.first && <span className="text-neutral-500"> (comes first)</span>}
          </label>
        ))}
      </fieldset>
      <Button
        onClick={async () => {
          const ok = await apply((latest) => {
            const [la, lb] = ids.map((id) => latest.props.find((p) => p.id === id));
            const now = la && lb ? joinLines(la, lb, JOIN_TOLERANCE, join.kept) : null;
            return now
              ? [
                  { type: "updateProp", prop: now.prop },
                  { type: "removeProp", id: now.removed },
                ]
              : [];
          });
          if (ok) select([join.kept]);
        }}
      >
        <Merge size={16} aria-hidden /> Join into one poly line
      </Button>
      <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-400">
        {a.name} and {b.name} become one poly line named {kept}, starting at {name(join.first)}'s start.
        {regions > 0 && " Submodels and faces of both lines move with their pixels."}
      </p>
      {notes.length > 0 && (
        <ul className="mt-1 list-disc pl-4 text-sm text-amber-700 dark:text-amber-400">
          {notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
    </Section>
  );
}
