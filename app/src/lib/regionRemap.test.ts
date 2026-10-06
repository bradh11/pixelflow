import { describe, expect, it } from "vitest";
import type { Region } from "../api/types";
import { moveRegion } from "./regionRemap";

const nodes = (lines: Region extends never ? never : ({ first: number; last: number } | null)[][]): Region => ({
  id: "r",
  name: "Part",
  kind: "nodes",
  lines,
  layout: "horizontal",
  buffer: "default",
});

describe("moving a submodel's pixels", () => {
  it("shifts pixels along when the line now starts further on", () => {
    const moved = moveRegion(nodes([[{ first: 0, last: 3 }, null, { first: 9, last: 5 }]]), { from: 0, to: 10, offset: 4, reverse: false });
    expect(moved).toMatchObject({ lines: [[{ first: 4, last: 7 }, null, { first: 13, last: 9 }]] });
  });

  it("runs pixels backwards for a line that's turned round, keeping each run's direction along the line", () => {
    // Ten pixels turned round and put after four others: pixel i goes to 4 + 9 - i.
    const moved = moveRegion(nodes([[{ first: 0, last: 3 }]]), { from: 0, to: 10, offset: 4, reverse: true });
    expect(moved).toMatchObject({ lines: [[{ first: 13, last: 10 }]] });
  });

  it("keeps only the pixels in the part, and drops a submodel left with none", () => {
    const r = nodes([[{ first: 2, last: 7 }], [{ first: 8, last: 9 }]]);
    expect(moveRegion(r, { from: 0, to: 5, offset: 0, reverse: false })).toMatchObject({ lines: [[{ first: 2, last: 4 }], []] });
    expect(moveRegion(r, { from: 5, to: 10, offset: -5, reverse: false })).toMatchObject({ lines: [[{ first: 0, last: 2 }], [{ first: 3, last: 4 }]] });
    expect(moveRegion(nodes([[{ first: 8, last: 9 }]]), { from: 0, to: 5, offset: 0, reverse: false })).toBeNull();
  });

  it("moves a face's ranges, and keeps a rectangle only where the pixels stay put", () => {
    const face: Region = { id: "f", name: "Face", kind: "face", mouths: { O: [{ start: 1, end: 3 }] }, eyesOpen: [{ start: 0, end: 1 }], eyesClosed: [], outline: [] };
    expect(moveRegion(face, { from: 0, to: 5, offset: 2, reverse: true })).toMatchObject({ mouths: { O: [{ start: 4, end: 6 }] }, eyesOpen: [{ start: 6, end: 7 }] });
    const box: Region = { id: "b", name: "Top", kind: "subBuffer", x1: 0, y1: 50, x2: 100, y2: 100 };
    expect(moveRegion(box, { from: 0, to: 5, offset: 0, reverse: false })).toEqual(box);
    expect(moveRegion(box, { from: 0, to: 5, offset: 3, reverse: false })).toBeNull();
  });
});
