import { useEffect, useMemo, useRef, useState } from "react";
import type { Port, PreviewProp, Show } from "../../api/types";
import { type Pt, type Size, type View, backgroundBox, boxOfPoints, fitView, toScreen, unionBox } from "../../lib/layoutMath";
import { batchPixels, drawBatches } from "../../lib/pixelBatches";
import { type PortRef, findPort, wiringPath } from "../../lib/wiringMath";
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

/** Sizes the canvas to its box (in device pixels) and returns its context, scaled to CSS pixels. */
function prepare(canvas: HTMLCanvasElement, size: Size): CanvasRenderingContext2D | null {
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.round(size.width * ratio);
  canvas.height = Math.round(size.height * ratio);
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  return ctx;
}

/**
 * The view of the layout: every prop and the photo, with room below for the controller box (the
 * path draws it below the lowest pixel of the port's props, at most 15% of the layout's height
 * lower), so it never changes with the port shown and the drawn layout can be kept.
 */
function layoutView(props: PreviewProp[], bg: Show["background"], aspect: number, size: Size): View {
  const propsBox = unionBox(props.map((p) => boxOfPoints(p.points)));
  const room = propsBox ? { ...propsBox, minY: propsBox.minY - Math.max(1, (propsBox.maxY - propsBox.minY) * 0.15) } : null;
  return fitView(unionBox([room, bg ? backgroundBox(bg, aspect) : null]), size, 18);
}

const NONE: ReadonlySet<string> = new Set();

/** The still part: backdrop, photo, and every prop dimmed. Drawn again only when they change. */
function drawLayout(canvas: HTMLCanvasElement, size: Size, view: View, props: PreviewProp[], bg: Show["background"], photo: { image: CanvasImageSource | null; aspect: number }) {
  const ctx = prepare(canvas, size);
  if (!ctx) return;
  const at = (p: Pt) => toScreen(view, size, p);
  ctx.fillStyle = BACKDROP;
  ctx.fillRect(0, 0, size.width, size.height);
  if (bg && photo.image) {
    const b = backgroundBox(bg, photo.aspect);
    const [tl, br] = [at({ x: b.minX, y: b.maxY }), at({ x: b.maxX, y: b.minY })];
    ctx.globalAlpha = bg.opacity * 0.45;
    ctx.drawImage(photo.image, tl.x, tl.y, br.x - tl.x, br.y - tl.y);
    ctx.globalAlpha = 1;
  }
  const radius = Math.min(3, Math.max(1, view.zoom * 0.04));
  drawBatches(ctx, batchPixels(props, null, view, size, NONE, PIXELS, radius), radius, window.devicePixelRatio || 1);
}

/** The part that follows the port in focus: its props lit, and its wiring path. */
function drawPath(canvas: HTMLCanvasElement, size: Size, view: View, props: PreviewProp[], port: Port | null, selectedProp: string | null) {
  const ctx = prepare(canvas, size);
  if (!ctx || !port) return;
  const at = (p: Pt) => toScreen(view, size, p);
  const onPort = new Set(port.slots.map((s) => s.prop));
  const radius = Math.min(3, Math.max(1, view.zoom * 0.04));
  const lit = props.filter((p) => onPort.has(p.prop));
  drawBatches(ctx, batchPixels(lit, null, view, size, onPort, PIXELS, radius), radius, window.devicePixelRatio || 1);
  const path = wiringPath(port, props);
  if (!path.start || !path.firstPixel) return;

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
  for (const run of path.runs) {
    const pts = run.points.map(at);
    stroke(run.prop === selectedProp ? 3 : 2, ACCENT, [], () => {
      ctx.beginPath();
      pts.forEach((p, j) => (j === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y)));
      ctx.stroke();
    });
    ctx.fillStyle = "#fff";
    let travelled = 0;
    let drawn = 0;
    for (let j = 1; j < pts.length; j++) {
      travelled += Math.hypot(pts[j].x - pts[j - 1].x, pts[j].y - pts[j - 1].y);
      if (travelled >= ARROW_EVERY_PX) {
        travelled = 0;
        drawn++;
        arrow(ctx, { x: (pts[j].x + pts[j - 1].x) / 2, y: (pts[j].y + pts[j - 1].y) / 2 }, pts[j]);
      }
    }
    // A short prop still shows which way it runs: one arrow halfway along.
    if (drawn === 0 && pts.length >= 2) {
      const j = Math.max(1, Math.floor(pts.length / 2));
      arrow(ctx, { x: (pts[j].x + pts[j - 1].x) / 2, y: (pts[j].y + pts[j - 1].y) / 2 }, pts[j]);
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
  ctx.fillText(`P${port.number}`, c.x, c.y + 0.5);
  const s = at(path.firstPixel);
  ctx.fillStyle = START;
  ctx.strokeStyle = HALO;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.arc(s.x, s.y, 5.5, 0, Math.PI * 2);
  ctx.stroke();
  ctx.fill();
}

const portKey = (r: PortRef | null) => (r ? `${r.controller}\u0000${r.port}\u0000${r.at ?? ""}` : null);

/**
 * The layout, small, with the hovered or selected port's props lit and its wiring drawn: from
 * the controller through each prop's first pixel to its last, a green dot where the data
 * enters, and arrows for the way it runs. The layout is drawn once and kept; only the path layer
 * is drawn again, at most once a frame, when the port in focus changes.
 */
export function WiringPreview({ show, props }: { show: Show; props: PreviewProp[] }) {
  const baseRef = useRef<HTMLCanvasElement>(null);
  const pathRef = useRef<HTMLCanvasElement>(null);
  // Strings, so a pointer move over the same port re-renders nothing.
  const dragKey = useWiring((s) => (s.drag?.over?.kind === "port" ? portKey(s.drag.over) : null));
  const selectedKey = useWiring((s) => portKey(s.selected));
  const selectedProp = useWiring((s) => s.selected?.prop ?? null);
  const hoveredKey = useWiring((s) => portKey(s.hovered));
  // While dragging, the port under the pointer; otherwise the selected chip's port, or the hovered one.
  const focusKey = dragKey ?? selectedKey ?? hoveredKey;
  const photo = useBackgroundImage(show.background?.path);
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });

  useEffect(() => {
    const canvas = baseRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver(() => setSize({ width: canvas.clientWidth, height: canvas.clientHeight }));
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  const focus = useMemo<PortRef | null>(() => {
    if (!focusKey) return null;
    const [controller, port, at] = focusKey.split("\u0000");
    return { controller, port: Number(port), at: at === "" ? undefined : Number(at) };
  }, [focusKey]);
  const port = focus ? findPort(show, focus) : null;
  // Kept while only the wiring changes (each edit brings a new show object).
  const bgKey = JSON.stringify(show.background ?? null);
  const bg = useMemo<Show["background"]>(() => JSON.parse(bgKey), [bgKey]);
  const { image, aspect } = photo;
  const view = useMemo(() => layoutView(props, bg, aspect, size), [props, bg, aspect, size]);

  useEffect(() => {
    if (baseRef.current) drawLayout(baseRef.current, size, view, props, bg, { image, aspect });
  }, [size, view, props, bg, image, aspect]);

  // At most once a frame, however fast the pointer moves.
  useEffect(() => {
    const canvas = pathRef.current;
    if (!canvas) return;
    const lit = port?.slots.some((s) => s.prop === selectedProp) ? selectedProp : null;
    const frame = requestAnimationFrame(() => drawPath(canvas, size, view, props, port, lit));
    return () => cancelAnimationFrame(frame);
  }, [size, view, props, port, selectedProp]);

  const controller = focus && show.controllers.find((c) => c.id === focus.controller);
  const names = port?.slots.map((s) => show.props.find((p) => p.id === s.prop)?.name ?? "Missing prop") ?? [];
  return (
    <section aria-label="Wiring preview" className="overflow-hidden rounded-lg border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900">
      <div className="relative h-56 w-full">
        <canvas ref={baseRef} data-size={size.width} className="absolute inset-0 block h-full w-full" aria-hidden />
        <canvas ref={pathRef} className="absolute inset-0 block h-full w-full" aria-hidden />
      </div>
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
