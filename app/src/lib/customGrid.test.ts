import { describe, expect, it } from "vitest";
import { clearGrid, resizeGrid, toggleCell } from "./customGrid";

describe("custom grid editing", () => {
  it("numbers empty cells in order and clears numbered ones, leaving a gap", () => {
    let cells = [0, 0, 0, 0];
    cells = toggleCell(cells, 2);
    cells = toggleCell(cells, 0);
    cells = toggleCell(cells, 3);
    expect(cells).toEqual([2, 0, 1, 3]);
    cells = toggleCell(cells, 0);
    expect(cells).toEqual([0, 0, 1, 3]);
    expect(toggleCell(cells, 1)).toEqual([0, 4, 1, 3]);
    expect(clearGrid(cells)).toEqual([0, 0, 0, 0]);
  });

  it("resizes keeping the top-left cells", () => {
    // 3 × 2:  1 2 3 / 4 5 6
    const cells = [1, 2, 3, 4, 5, 6];
    expect(resizeGrid(cells, 3, 2, 2)).toEqual([1, 2, 4, 5]);
    expect(resizeGrid(cells, 3, 4, 3)).toEqual([1, 2, 3, 0, 4, 5, 6, 0, 0, 0, 0, 0]);
    expect(resizeGrid(cells, 3, 3, 1)).toEqual([1, 2, 3]);
  });
});
