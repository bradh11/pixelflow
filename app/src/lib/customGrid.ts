// Editing a custom grid's cells: row-major from the top row, 0 for an empty cell and n for
// pixel n (1-based). The pixel count is the highest number, so gaps are allowed.

/** The cells with cell `index` numbered next (one more than the highest), or cleared if it had a number. */
export function toggleCell(cells: number[], index: number): number[] {
  const next = cells.reduce((m, c) => Math.max(m, c), 0) + 1;
  return cells.map((c, i) => (i === index ? (c === 0 ? next : 0) : c));
}

/** The grid resized, keeping the cells that still fit (anchored at the top left). */
export function resizeGrid(cells: number[], columns: number, newColumns: number, newRows: number): number[] {
  return Array.from({ length: newColumns * newRows }, (_, i) => {
    const [row, col] = [Math.floor(i / newColumns), i % newColumns];
    return col < columns ? (cells[row * columns + col] ?? 0) : 0;
  });
}

/** Every cell emptied. */
export function clearGrid(cells: number[]): number[] {
  return cells.map(() => 0);
}

/** Largest grid edited cell by cell in the properties panel. */
export const MAX_EDITED_CELLS = 100 * 100;
