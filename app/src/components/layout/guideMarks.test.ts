import { describe, expect, it } from "vitest";
import { GUIDE_COLORS, drawGuideMarks } from "./guideMarks";

/** The selection's accent on the canvas. */
const SELECTION = "#a78bfa";

/** A 2D context that remembers every stroke and fill color set, and the text drawn. */
function recorder() {
  const styles: string[] = [];
  const texts: string[] = [];
  const ctx = new Proxy({} as Record<string | symbol, unknown>, {
    get: (target, key) => {
      if (key in target) return target[key];
      if (key === "fillText") return (text: string) => texts.push(text);
      if (key === "measureText") return (text: string) => ({ width: text.length * 6 });
      return () => {};
    },
    set: (target, key, value) => {
      if (key === "strokeStyle" || key === "fillStyle") styles.push(value as string);
      target[key] = value;
      return true;
    },
  });
  return { ctx: ctx as unknown as CanvasRenderingContext2D, styles, texts };
}

describe("smart guide colors", () => {
  it("stand apart from the selection's color in both themes", () => {
    for (const theme of ["dark", "light"] as const) {
      expect(GUIDE_COLORS[theme].line.toLowerCase()).not.toBe(SELECTION);
      expect(GUIDE_COLORS[theme].line).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it("are what guides, gaps, and size marks are drawn in", () => {
    const { ctx, styles, texts } = recorder();
    const marks = {
      guides: [{ axis: "x" as const, at: 0, from: 0, to: 5 }],
      gaps: [{ axis: "x" as const, from: 2, to: 4, at: 0.5 }],
      sizes: [{ dim: "width" as const, box: { minX: 0, minY: 0, maxX: 4, maxY: 2 }, moving: true }],
    };
    drawGuideMarks(ctx, marks, (p) => ({ x: p.x * 10, y: -p.y * 10 }), GUIDE_COLORS.light);
    expect(styles).toContain(GUIDE_COLORS.light.line);
    expect(styles).not.toContain(SELECTION);
    expect(texts).toEqual(["2", "same width"]);
  });
});
