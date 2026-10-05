import { useEffect, useRef, useState } from "react";
import type { PreviewProp, Show } from "../../api/types";
import { type Pt, type Size, backgroundBox, boxOfPoints, fitView, toScreen, unionBox } from "../../lib/layoutMath";
import { batchPixels, drawBatches } from "../../lib/pixelBatches";
import { type PortRef, type SlotRef, wiringPath } from "../../lib/wiringMath";
import { useWiring } from "../../state/wiring";
import { useBackgroundImage } from "../layout/useLayoutData";

// Dark in both themes, like the layout canvas: lights are judged against a night sky.
const BACKDROP = "#0a0a0c";
const ACCENT = "#a78bfa";
const START = "#34d399";
const WIRE = "rgba(255, 255, 255, 0.75)";
const HALO = "rgba(0, 0, 0, 0.7)";
const PIXELS = { unlit: "rgba(200, 200, 200, 0.28)", selected: ACCENT, dark: "rgba(90, 90, 90, 0.6)" };

/** Arrows along a prop: about one every this many screen pixels. */
const ARROW_EVERY_PX = 70;

function arrow(ctx: CanvasRenderingContext2D, at: Pt, toward: Pt) {
  const angle = Math.atan2(toward.y - at.y, toward.x - at.x);
  ctx.save();
  ctx.translate(at.x, at.y);
  ctx.rotate(angle);
  ctx.beginPath();
  ctx.moveTo(5, 0);
  ctx.lineTo(-4, -4);
  ctx.lineTo(-4, 4);
  ctx.closePath();
  ctx.fill();
  ctx.restore();
}

function draw(canvas: HTMLCanvasElement, show: Show, props: PreviewProp[], photo: ReturnType<typeof useBackgroundImage>, focus: PortRef | null, selected: SlotRef | null) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const ratio = window.devicePixelRatio || 1;
  const size: Size = { width: canvas.clientWidth, height: canvas.clientHeight };
  canvas.width = Math.round(size.width * ratio);
  canvas.height = Math.round(size.height * ratio);
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  ctx.fillStyle = BACKDROP;
  ctx.fillRect(0, 0, size.width, size.height);

  const port = focus && show.controllers.find((c) => c.id === focus.controller)?.ports.find((p) => p.number === focus.port);
  const path = port ? wiringPath(port, props) : null;
  const bg = show.background ?? null;
  const box = unionBox([...props.map((p) => boxOfPoints(p.points)), bg ? backgroundBox(bg, photo.aspect) : null, path?.start ? { minX: path.start.x, maxX: path.start.x, minY: path.start.y, maxY: path.start.y } : null]);
  const view = fitView(box, size, 18);
  const at = (p: Pt) => toScreen(view, size, p);

  if (bg && photo.image) {
    const b = backgroundBox(bg, photo.aspect);
    const [tl, br] = [at({ x: b.minX, y: b.maxY }), at({ x: b.maxX, y: b.minY })];
    ctx.globalAlpha = bg.opacity * 0.45;
    ctx.drawImage(photo.image, tl.x, tl.y, br.x - tl.x, br.y - tl.y);
    ctx.globalAlpha = 1;
  }

  const onPort = new Set(port?.slots.map((s) => s.prop) ?? []);
  const radius = Math.min(3, Math.max(1, view.zoom * 0.04));
  drawBatches(ctx, batchPixels(props, null, view, size, onPort, PIXELS, radius), radius, ratio);
  if (!path || !path.start || !path.firstPixel) return;

  // The wire between props: dashed.
  const stroke = (width: number, color: string, dash: number[], line: () => void) => {
    ctx.setLineDash(dash);
    ctx.strokeStyle = HALO;
    ctx.lineWidth = width + 2;
    line();
    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    line();
    ctx.setLineDash([]);
  };
  for (const jump of path.jumps) {
    const [a, b] = [at(jump.from), at(jump.to)];
    stroke(1.5, WIRE, [5, 4], () => {
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
    });
  }
  // Along each prop, first pixel to last, with arrows for the direction the data runs.
  const selectedProp = selected && port && selected.controller === focus?.controller && selected.port === focus.port ? port.slots[selected.index]?.prop : null;
  for (const run of path.runs) {
    const pts = run.points.map(at);
    stroke(run.prop === selectedProp ? 3 : 2, ACCENT, [], () => {
      ctx.beginPath();
      pts.forEach((p, j) => (j === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y)));
      ctx.stroke();
    });
    ctx.fillStyle = "#fff";
    let travelled = 0;
    for (let j = 1; j < pts.length; j++) {
      travelled += Math.hypot(pts[j].x - pts[j - 1].x, pts[j].y - pts[j - 1].y);
      if (travelled >= ARROW_EVERY_PX || (pts.length === 2 && j === 1)) {
        travelled = 0;
        const mid = { x: (pts[j].x + pts[j - 1].x) / 2, y: (pts[j].y + pts[j - 1].y) / 2 };
        arrow(ctx, mid, pts[j]);
      }
    }
  }

  // The controller, and the first pixel its data reaches.
  const c = at(path.start);
  ctx.fillStyle = "#1f2937";
  ctx.strokeStyle = WIRE;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.roundRect?.(c.x - 14, c.y - 9, 28, 18, 4);
  ctx.fill();
  ctx.stroke();
  ctx.fillStyle = "#fff";
  ctx.font = "600 10px system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(`P${port!.number}`, c.x, c.y + 0.5);
  const s = at(path.firstPixel);
  ctx.fillStyle = START;
  ctx.strokeStyle = HALO;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.arc(s.x, s.y, 5.5, 0, Math.PI * 2);
  ctx.stroke();
  ctx.fill();
}

/**
 * The layout, small, with the hovered or selected port's props lit and its wiring drawn: from
 * the controller through each prop's first pixel to its last, a green dot where the data
 * enters, and arrows for the way it runs.
 */
export function WiringPreview({ show, props }: { show: Show; props: PreviewProp[] }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const selected = useWiring((s) => s.selected);
  const hovered = useWiring((s) => s.hovered);
  const focus = selected ?? hovered;
  const photo = useBackgroundImage(show.background?.path);
  const [size, setSize] = useState(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver(() => setSize(canvas.clientWidth));
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (canvasRef.current) draw(canvasRef.current, show, props, photo, focus, selected);
  });

  const port = focus && show.controllers.find((c) => c.id === focus.controller)?.ports.find((p) => p.number === focus.port);
  const controller = focus && show.controllers.find((c) => c.id === focus.controller);
  const names = port?.slots.map((s) => show.props.find((p) => p.id === s.prop)?.name ?? "Missing prop") ?? [];
  return (
    <section aria-label="Wiring preview" className="overflow-hidden rounded-lg border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900">
      <canvas ref={canvasRef} data-size={size} className="block h-56 w-full" aria-hidden />
      <p className="px-3 py-2 text-xs text-neutral-600 dark:text-neutral-400" aria-live="polite" data-testid="wiring-preview-caption">
        {!port || !controller
          ? "Point at a port to see its wiring here."
          : names.length === 0
            ? `Port ${port.number} on ${controller.name} has nothing wired yet.`
            : `Port ${port.number} on ${controller.name}: ${names.join(" → ")}. The data enters at the green dot and runs the way the arrows point.`}
      </p>
    </section>
  );
}
