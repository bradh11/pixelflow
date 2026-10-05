import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Edit, Prop } from "../api/types";
import { alignEdits, besideOthers, distributeEdits, duplicateEdits, gestureEdits, removeEdits, wiringOf } from "./layoutEdits";
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

  it("says where a prop is wired", () => {
    const show = demoShow();
    expect(wiringOf(show, show.props[0].id)).toEqual(["Port 1 on Main FPP"]);
    expect(wiringOf(show, show.props[3].id)).toEqual([]);
  });
});
