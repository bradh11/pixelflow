// Draws the smart guides on the layout canvas while a prop is dragged: lines where edges and
// centers meet, arrows with their length across equal gaps, and "same width" / "same height"
// beside boxes of the same size. Thin lines in the accent color over a dark outline, so they
// read over a bright photo too.

import type { Box, Pt } from "../../lib/layoutMath";
import { type Marks, formatGap } from "../../lib/smartGuides";

interface Colors {
  accent: string;
  halo: string;
  /** Text on an accent label. */
  ink: string;
}

const ARROW = 4;
/** How far outside a box its size mark sits, in screen pixels. */
const SIZE_OFFSET = 10;

/** Draws `marks` (in world units) with `at` turning world points into screen points. */
export function drawGuideMarks(ctx: CanvasRenderingContext2D, marks: Marks, at: (p: Pt) => Pt, colors: Colors) {
  const crisp = (v: number) => Math.round(v) + 0.5;
  const stroke = (path: () => void) => {
    ctx.setLineDash([]);
    ctx.lineWidth = 3;
    ctx.strokeStyle = colors.halo;
    path();
    ctx.lineWidth = 1;
    ctx.strokeStyle = colors.accent;
    path();
  };

  for (const g of marks.guides) {
    const [a, b] = g.axis === "x" ? [at({ x: g.at, y: g.from }), at({ x: g.at, y: g.to })] : [at({ x: g.from, y: g.at }), at({ x: g.to, y: g.at })];
    stroke(() => {
      ctx.beginPath();
      if (g.axis === "x") {
        ctx.moveTo(crisp(a.x), a.y);
        ctx.lineTo(crisp(b.x), b.y);
      } else {
        ctx.moveTo(a.x, crisp(a.y));
        ctx.lineTo(b.x, crisp(b.y));
      }
      ctx.stroke();
    });
  }

  for (const g of marks.gaps) {
    const [a, b] = g.axis === "x" ? [at({ x: g.from, y: g.at }), at({ x: g.to, y: g.at })] : [at({ x: g.at, y: g.from }), at({ x: g.at, y: g.to })];
    arrow(ctx, a, b, stroke);
    label(ctx, formatGap(g.to - g.from), { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }, colors);
  }

  for (const s of marks.sizes) {
    const [a, b] = sizeLine(s.box, s.dim, at);
    arrow(ctx, a, b, stroke);
    label(ctx, s.dim === "width" ? "same width" : "same height", { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }, colors);
  }
}

/** Where a size mark goes: just below the box for its width, just left of it for its height. */
function sizeLine(box: Box, dim: "width" | "height", at: (p: Pt) => Pt): [Pt, Pt] {
  const [tl, br] = [at({ x: box.minX, y: box.maxY }), at({ x: box.maxX, y: box.minY })];
  if (dim === "width") return [{ x: tl.x, y: br.y + SIZE_OFFSET }, { x: br.x, y: br.y + SIZE_OFFSET }];
  return [{ x: tl.x - SIZE_OFFSET, y: tl.y }, { x: tl.x - SIZE_OFFSET, y: br.y }];
}

/** A line from `a` to `b` with an arrowhead at each end. */
function arrow(ctx: CanvasRenderingContext2D, a: Pt, b: Pt, stroke: (path: () => void) => void) {
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  if (len < 1) return;
  const [ux, uy] = [(b.x - a.x) / len, (b.y - a.y) / len];
  const head = Math.min(ARROW, len / 3);
  const tip = (p: Pt, dir: number) => {
    ctx.moveTo(p.x + dir * (ux * head - uy * head), p.y + dir * (uy * head + ux * head));
    ctx.lineTo(p.x, p.y);
    ctx.lineTo(p.x + dir * (ux * head + uy * head), p.y + dir * (uy * head - ux * head));
  };
  stroke(() => {
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    tip(a, 1);
    tip(b, -1);
    ctx.stroke();
  });
}

/** A small accent pill with `text`, centered on `p`. */
function label(ctx: CanvasRenderingContext2D, text: string, p: Pt, colors: Colors) {
  ctx.font = "600 10px system-ui, -apple-system, sans-serif";
  const w = ctx.measureText(text).width + 8;
  const h = 14;
  ctx.fillStyle = colors.accent;
  ctx.beginPath();
  if (typeof ctx.roundRect === "function") ctx.roundRect(p.x - w / 2, p.y - h / 2, w, h, 3);
  else ctx.rect(p.x - w / 2, p.y - h / 2, w, h);
  ctx.fill();
  ctx.fillStyle = colors.ink;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(text, p.x, p.y + 0.5);
}
