import type { Prop } from "../../api/types";
import { MAX_EDITED_CELLS, clearGrid, resizeGrid, toggleCell } from "../../lib/customGrid";
import { updateEdits } from "../../lib/layoutEdits";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { NumberField, Section } from "./PropertiesPanel";

type GridShape = Extract<Prop["shape"], { type: "customGrid" }>;

/**
 * A custom grid's cells: click an empty cell to give it the next pixel number, click a numbered
 * one to empty it, clear them all, or change the grid's size. Each change is one undo step.
 */
export function CustomGridSection({ prop, shape }: { prop: Prop; shape: GridShape }) {
  const apply = useApp((s) => s.apply);
  const change = (edit: (s: GridShape) => GridShape) =>
    void apply(updateEdits(prop.id, (p) => (p.shape.source === "generator" && p.shape.type === "customGrid" ? { ...p, shape: edit(p.shape) } : p)));
  const { columns, rows, cells } = shape;
  const editable = columns * rows <= MAX_EDITED_CELLS;
  return (
    <Section title="Grid">
      <div className="grid grid-cols-2 gap-2">
        <NumberField label="Columns" integer min={1} max={1000} value={columns} onCommit={(n) => change((s) => ({ ...s, columns: n, cells: resizeGrid(s.cells, s.columns, n, s.rows) }))} />
        <NumberField label="Rows" integer min={1} max={1000} value={rows} onCommit={(n) => change((s) => ({ ...s, rows: n, cells: resizeGrid(s.cells, s.columns, s.columns, n) }))} />
      </div>
      {editable ? (
        <>
          <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-400">Click a square to give it the next pixel; click a numbered one to empty it.</p>
          <div className="mt-2 max-h-80 overflow-auto">
            <div role="grid" aria-label="Pixels in the grid" className="inline-grid gap-px" style={{ gridTemplateColumns: `repeat(${columns}, 1.75rem)` }}>
              {cells.map((c, i) => (
                <button
                  key={i}
                  type="button"
                  role="gridcell"
                  aria-label={`Row ${Math.floor(i / columns) + 1}, column ${(i % columns) + 1}${c ? `: pixel ${c}` : ": empty"}`}
                  onClick={() => change((s) => ({ ...s, cells: toggleCell(s.cells, i) }))}
                  className={`h-7 text-[10px] tabular-nums ${c ? "bg-accent-600 text-white" : "bg-neutral-200 text-neutral-500 hover:bg-neutral-300 dark:bg-neutral-800 dark:hover:bg-neutral-700"}`}
                >
                  {c || ""}
                </button>
              ))}
            </div>
          </div>
          <Button className="mt-2" onClick={() => change((s) => ({ ...s, cells: clearGrid(s.cells) }))}>
            Clear all
          </Button>
        </>
      ) : (
        <p className="mt-2 text-sm text-neutral-500">This grid is too big to edit square by square here.</p>
      )}
    </Section>
  );
}
