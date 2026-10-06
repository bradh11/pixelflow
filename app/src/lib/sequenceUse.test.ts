import { describe, expect, it } from "vitest";
import { newEffect, newRow, type Sequence } from "../api/sequence";
import { deleteUseWarning, rowsUsing } from "./sequenceUse";

function doc(rows: Sequence["rows"]): Sequence {
  return { schemaVersion: 1, name: "Christmas Medley", audio: null, durationMs: 60_000, frameMs: 25, timingTracks: [], rows };
}

function rowWith(target: Parameters<typeof newRow>[0], effects: number) {
  const row = newRow(target);
  row.layers[0].effects = Array.from({ length: effects }, (_, i) => newEffect("on", i * 100, i * 100 + 50));
  return row;
}

describe("rowsUsing", () => {
  const seq = doc([rowWith({ group: "g" }, 2), rowWith({ group: "g" }, 1), rowWith({ prop: "a" }, 3), rowWith({ region: { prop: "a", region: "r" } }, 4), rowWith({ prop: "b" }, 5)]);

  it("counts a group's rows and their effects", () => {
    expect(rowsUsing(seq, { group: "g" })).toEqual({ rows: 2, effects: 3 });
    expect(rowsUsing(seq, { group: "other" })).toEqual({ rows: 0, effects: 0 });
  });

  it("counts props' rows, their submodels' rows too", () => {
    expect(rowsUsing(seq, { props: ["a"] })).toEqual({ rows: 2, effects: 7 });
    expect(rowsUsing(seq, { props: ["a", "b"] })).toEqual({ rows: 3, effects: 12 });
  });

  it("is nothing without an open sequence", () => {
    expect(rowsUsing(null, { group: "g" })).toEqual({ rows: 0, effects: 0 });
  });
});

describe("deleteUseWarning", () => {
  it("names what's used, by how many rows and effects, in which sequence", () => {
    const seq = doc([rowWith({ group: "g" }, 2), rowWith({ group: "g" }, 141)]);
    expect(deleteUseWarning(seq, "Group “All Arches”", { group: "g" })).toBe(
      "Group “All Arches” lights 2 rows with 143 effects in Christmas Medley. Delete it anyway? Those rows will show nothing until you point them at something else.",
    );
    expect(deleteUseWarning(seq, "x", { group: "none" })).toBeNull();
  });
});
