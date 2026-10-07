import { describe, expect, it } from "vitest";
import { fieldValue, parseNumbers, withField } from "./shapeFields";

describe("shape fields", () => {
  it("reads and sets nested settings, starting a missing one from its defaults", () => {
    const shape = { type: "matrix", columns: 4 };
    expect(fieldValue({ wiring: { start: "topLeft" } }, "wiring.start")).toBe("topLeft");
    expect(fieldValue(shape, "wiring.start")).toBeUndefined();
    const defaults = { wiring: { start: "bottomLeft", serpentine: true } };
    expect(withField(shape, "wiring.serpentine", false, defaults)).toEqual({ type: "matrix", columns: 4, wiring: { start: "bottomLeft", serpentine: false } });
    expect(withField(shape, "columns", 8)).toEqual({ type: "matrix", columns: 8 });
  });

  it("reads comma lists of whole numbers within bounds", () => {
    expect(parseNumbers("3, 4,5,4", 1)).toEqual([3, 4, 5, 4]);
    expect(parseNumbers("3,,4", 1)).toBeNull();
    expect(parseNumbers("3,0", 1)).toBeNull();
    expect(parseNumbers("2.5", 1)).toBeNull();
    expect(parseNumbers("120", 1, 100)).toBeNull();
    // Zeros leave gaps, but a list of only gaps has nothing in it.
    expect(parseNumbers("0,3,0", 0)).toEqual([0, 3, 0]);
    expect(parseNumbers("0, 0", 0)).toBeNull();
    // A list that may be empty (an arch's layers) can be cleared, but not left half typed.
    expect(parseNumbers(" ", 1, 100, true)).toEqual([]);
    expect(parseNumbers("", 1)).toBeNull();
    expect(parseNumbers("5,", 1, 100, true)).toBeNull();
  });
});
