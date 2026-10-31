import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Edit, Prop } from "../api/types";
import { frontView } from "./geometry";
import { alignEdits, besideOthers, distributeEdits, duplicateEdits, gestureEdits, pasteEdits, placedInView, removeEdits, updateEdits, visibleBox, wiringOf } from "./layoutEdits";
import { boxOfPoints } from "./layoutMath";
import { newProp } from "./shows";

const props = (edits: Edit[]) => edits.map((e) => (e.type === "updateProp" || e.type === "addProp" ? e.prop : null)) as Prop[];

describe("layout edits", () => {
  it("turns a gesture into one update per selected prop, or nothing for no change", () => {
    const show = demoShow();
    const [a, b] = show.props;
    const edits = gestureEdits(show, [a.id, b.id], { kind: "move", dx: 1, dy: 2 });
    expect(edits.map((e) => e.type)).toEqual(["updateProp", "updateProp"]);
    expect(props(edits)[0].transform.position).toMatchObject({ x: a.transform.position.x + 1, y: a.transform.position.y + 2 });
    expect(gestureEdits(show, [a.id], { kind: "move", dx: 0, dy: 0 })).toEqual([]);
  });

  it("removes props in one batch", () => {
    expect(removeEdits(["a", "b"])).toEqual([
      { type: "removeProp", id: "a" },
      { type: "removeProp", id: "b" },
    ]);
  });

  it("duplicates props offset slightly, with new ids and names", () => {
    const show = demoShow();
    const arch = show.props[0];
    const { edits, ids } = duplicateEdits(show, [arch.id]);
    const [copy] = props(edits);
    expect(ids).toEqual([copy.id]);
    expect(copy.id).not.toBe(arch.id);
    expect(copy.name).toBe("Garage Arch copy");
    expect(copy.transform.position.x).toBeCloseTo(arch.transform.position.x + 0.5);
    expect(copy.transform.position.y).toBeCloseTo(arch.transform.position.y - 0.5);
    expect(copy.shape).toEqual(arch.shape);
  });

  it("pastes copied props as new ones, offset, renamed only where the name is taken", () => {
    const show = demoShow();
    const [arch, star] = show.props;
    const copied = structuredClone([arch, star]);
    // Pasted into a show where the arch is gone: it keeps its own name there.
    const other = { ...show, props: show.props.filter((p) => p.id !== arch.id) };
    const { edits, ids } = pasteEdits(other, copied, 1);
    const [a, b] = props(edits);
    expect(edits.map((e) => e.type)).toEqual(["addProp", "addProp"]);
    expect(ids).toEqual([a.id, b.id]);
    expect([a.id, b.id]).not.toContain(arch.id);
    expect(a.name).toBe(arch.name);
    expect(b.name).toBe(`${star.name} copy`);
    expect(a.transform.position).toMatchObject({ x: arch.transform.position.x + 1, y: arch.transform.position.y - 1 });
    expect(pasteEdits(show, copied, 0).edits.map((e) => (e as { prop: Prop }).prop.transform.position)).toEqual([
      arch.transform.position,
      star.transform.position,
    ]);
    // The copies are separate objects: changing one doesn't touch what was copied.
    a.transform.scale.x = 9;
    expect(copied[0].transform.scale.x).toBe(arch.transform.scale.x);
  });

  it("aligns and distributes by the props' pixels", async () => {
    const backend = new MemoryBackend(demoShow());
    const show = backend.show;
    const preview = (await backend.previewProps()).props;
    const ids = show.props.map((p) => p.id);
    const aligned = await backend.applyEdits(alignEdits(show, preview, ids, "bottom"));
    const after = (await backend.previewProps()).props;
    const bottoms = after.map((p) => Math.min(...Array.from(p.points).filter((_, i) => i % 2 === 1)));
    for (const b of bottoms) expect(b).toBeCloseTo(bottoms[0], 3);
    expect(aligned.canUndo).toBe(true);
    expect(distributeEdits(show, preview, ids.slice(0, 2), "horizontal")).toEqual([]);
  });

  it("puts a new prop just right of the others", () => {
    const show = demoShow();
    const placed = besideOthers(newProp("line", show), show);
    // The mega tree's right edge is at x = 9.5; a 5-unit line centered on its origin starts 1 unit later.
    expect(placed.transform.position.x).toBeCloseTo(13, 1);
    expect(besideOthers(newProp("line", emptyShow("x")), emptyShow("x")).transform.position).toMatchObject({ x: 0, y: 0 });
  });

  it("puts a new prop in the middle of what the canvas shows", () => {
    const show = demoShow();
    const visible = { minX: 100, minY: 40, maxX: 140, maxY: 60 };
    const placed = placedInView(newProp("line", show), show, [], visible);
    const box = boxOfPoints(frontView(placed))!;
    expect((box.minX + box.maxX) / 2).toBeCloseTo(120, 2);
    expect((box.minY + box.maxY) / 2).toBeCloseTo(50, 2);
  });

  it("puts each new prop beside the last ones, in view and on none of them, nearest the middle", () => {
    let show = demoShow();
    const visible = { minX: 100, minY: 40, maxX: 140, maxY: 60 };
    const boxes = [];
    for (const kind of ["tree", "tree", "arch", "matrix", "star", "line", "tree"] as const) {
      const placed = placedInView(newProp(kind, show), show, [], visible);
      boxes.push(boxOfPoints(frontView(placed))!);
      show = { ...show, props: [...show.props, placed] };
    }
    const [first, second] = boxes;
    // The first in the middle; the next one right beside it, at the same height.
    expect((first.minX + first.maxX) / 2).toBeCloseTo(120, 2);
    expect((second.minY + second.maxY) / 2).toBeCloseTo(50, 2);
    expect(second.minX).toBeGreaterThan(first.maxX);
    for (const [i, b] of boxes.entries()) {
      expect(b.minX >= visible.minX && b.maxX <= visible.maxX && b.minY >= visible.minY && b.maxY <= visible.maxY).toBe(true);
      for (const other of boxes.slice(0, i)) {
        const apart = b.minX >= other.maxX || b.maxX <= other.minX || b.minY >= other.maxY || b.maxY <= other.minY;
        expect(apart).toBe(true);
      }
    }
  });

  it("steps aside from the middle when nothing in view is clear, enough to leave the last one to pick", () => {
    // A matrix as big as the view, in the middle of it: nothing in view is clear.
    const show = emptyShow("t");
    const wall = newProp("matrix", show);
    if (wall.shape.source === "generator" && wall.shape.type === "matrix") Object.assign(wall.shape, { width: 24, height: 24 });
    show.props = [wall];
    const visible = { minX: -10, minY: -10, maxX: 10, maxY: 10 };
    const first = placedInView(newProp("tree", show), show, [], visible);
    const withFirst = { ...show, props: [...show.props, first] };
    const second = placedInView(newProp("tree", withFirst), withFirst, [], visible);
    const [a, b] = [boxOfPoints(frontView(first))!, boxOfPoints(frontView(second))!];
    // A quarter of a tree's size (the tree is 5 tall) down and to the right of the last one.
    expect(b.minX - a.minX).toBeGreaterThanOrEqual(1.25 - 1e-6);
    expect(a.minY - b.minY).toBeGreaterThanOrEqual(1.25 - 1e-6);
  });

  it("falls back to beside the others when the canvas hasn't been measured", () => {
    const show = demoShow();
    const prop = newProp("line", show);
    expect(placedInView(prop, show, [], null)).toEqual(besideOthers(prop, show));
  });

  it("works out the part of the layout a view shows", () => {
    expect(visibleBox({ cx: 10, cy: 5, zoom: 20 }, { width: 400, height: 200 })).toEqual({ minX: 0, minY: 0, maxX: 20, maxY: 10 });
    expect(visibleBox({ cx: 10, cy: 5, zoom: 20 }, { width: 0, height: 0 })).toBeNull();
  });

  it("goes by the engine's positions for the others where it has them", () => {
    const show = demoShow();
    // The engine draws the first prop far out to the right (say, an imported shape).
    const preview = [{ prop: show.props[0].id, frameOffset: 0, channelsPerPixel: 3, points: [40, 0, 50, 2] }];
    const placed = besideOthers(newProp("line", show), show, preview);
    expect(placed.transform.position.x).toBeCloseTo(53.5, 1);
  });

  it("builds a prop's update from the show as it is when sent", () => {
    const show = demoShow();
    const id = show.props[1].id;
    const rename = updateEdits(id, (p) => ({ ...p, name: "Renamed" }));
    const moved = structuredClone(show);
    moved.props[1].transform.position.x = 42;
    const [edit] = rename(moved);
    expect(edit).toMatchObject({ type: "updateProp", prop: { name: "Renamed", transform: { position: { x: 42 } } } });
    expect(rename({ ...show, props: [] })).toEqual([]);
  });

  it("says where a prop is wired", () => {
    const show = demoShow();
    expect(wiringOf(show, show.props[0].id)).toEqual(["Port 1 on Main FPP"]);
    expect(wiringOf(show, show.props[3].id)).toEqual([]);
  });
});
