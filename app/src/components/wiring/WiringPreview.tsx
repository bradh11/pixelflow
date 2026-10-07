import { type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent, useEffect, useMemo, useRef, useState } from "react";
import type { Port, PreviewProp, Show } from "../../api/types";
import { type Box, type Pt, type Size, type View, backgroundBox, boxOfPoints, fitView, hitTest, toScreen, toWorld, unionBox } from "../../lib/layoutMath";
import { batchPixels, drawBatches } from "../../lib/pixelBatches";
import { type PortRef, findPort, wiringPath } from "../../lib/wiringMath";
import { useWiring } from "../../state/wiring";
import { useBackgroundImage } from "../layout/useLayoutData";

// Dark in both themes, like the layout canvas: lights are judged against a night sky.
const BACKDROP = "#0a0a0c";
const ACCENT = "#a78bfa";
const LIT = "#fde047";
const START = "#34d399";
const WIRE = "rgba(255, 255, 255, 0.75)";
const HALO = "rgba(0, 0, 0, 0.7)";
const PIXELS = { unlit: "rgba(200, 200, 200, 0.28)", selected: ACCENT, dark: "rgba(90, 90, 90, 0.6)" };

/** Arrows along a prop: about one every this many screen pixels. */
const ARROW_EVERY_PX = 70;
/** A click this close (screen pixels) to a prop's pixel picks it. */
const PICK_PX = 10;
const PAD = 18;

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
 * What the canvas shows: every prop and the photo, with room below for the controller box (the
 * path draws it below the lowest pixel of the port's props, at most 15% of the layout's height
 * lower), so it never changes with the port shown and the drawn layout can be kept.
 */
export function wiringBox(props: PreviewProp[], bg: Show["background"], aspect: number): Box | null {
  const propsBox = unionBox(props.map((p) => boxOfPoints(p.points)));
  const room = propsBox ? { ...propsBox, minY: propsBox.minY - Math.max(1, (propsBox.maxY - propsBox.minY) * 0.15) } : null;
  return unionBox([room, bg ? backgroundBox(bg, aspect) : null]);
}

export const wiringView = (box: Box | null, size: Size): View => fitView(box, size, PAD);

/** The prop under a point on the canvas (CSS pixels from its top left), if any. */
export function pickAt(props: PreviewProp[], box: Box | null, size: Size, at: Pt): string | null {
  const view = wiringView(box, size);
  return hitTest(props, toWorld(view, size, at), PICK_PX / view.zoom);
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

/**
 * The part that follows the port in focus: its props lit, and its wiring path, with `highlight`'s
 * stretch of it drawn brighter (or, when it isn't on the port, a box around it). With `badges`,
 * each prop's place on the port is numbered at its first pixel.
 */
function drawPath(canvas: HTMLCanvasElement, size: Size, view: View, props: PreviewProp[], port: Port | null, highlight: string | null, badges: boolean) {
  const ctx = prepare(canvas, size);
  if (!ctx) return;
  const at = (p: Pt) => toScreen(view, size, p);
  const onPort = new Set(port?.slots.map((s) => s.prop) ?? []);
  const radius = Math.min(3, Math.max(1, view.zoom * 0.04));
  if (highlight && !onPort.has(highlight)) {
    const box = boxOfPoints(props.find((p) => p.prop === highlight)?.points ?? []);
    if (box) {
      const [tl, br] = [at({ x: box.minX, y: box.maxY }), at({ x: box.maxX, y: box.minY })];
      ctx.setLineDash([4, 3]);
      ctx.strokeStyle = LIT;
      ctx.lineWidth = 1.5;
      ctx.strokeRect(tl.x - 6, tl.y - 6, br.x - tl.x + 12, br.y - tl.y + 12);
      ctx.setLineDash([]);
    }
  }
  if (!port) return;
  const lit = props.filter((p) => onPort.has(p.prop));
  drawBatches(ctx, batchPixels(lit, null, view, size, onPort, PIXELS, radius), radius, window.devicePixelRatio || 1);
  const path = wiringPath(port, props);
  if (!path.start || !path.firstPixel) return;

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
  // The wire between props: dashed. The wire into the highlighted prop is drawn brighter.
  path.jumps.forEach((jump, i) => {
    const [a, b] = [at(jump.from), at(jump.to)];
    const into = path.runs[i]?.prop === highlight;
    stroke(into ? 2.5 : 1.5, into ? LIT : WIRE, [5, 4], () => {
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
    });
  });
  // Along each prop, first pixel to last, with arrows for the direction the data runs.
  for (const run of path.runs) {
    const pts = run.points.map(at);
    const bright = run.prop === highlight;
    stroke(bright ? 4 : 2, bright ? LIT : ACCENT, [], () => {
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

  if (!badges) return;
  // Numbered just above each prop's first pixel, in wiring order.
  ctx.font = "700 11px system-ui, sans-serif";
  for (const run of path.runs) {
    const p = at(run.points[0]);
    const [x, y] = [p.x, p.y - 15];
    ctx.fillStyle = run.prop === highlight ? LIT : ACCENT;
    ctx.strokeStyle = HALO;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(x, y, 9, 0, Math.PI * 2);
    ctx.stroke();
    ctx.fill();
    ctx.fillStyle = "#111";
    ctx.fillText(String(run.slot + 1), x, y + 0.5);
  }
}

export interface WiringCanvasProps {
  show: Show;
  props: PreviewProp[];
  /** The port whose wiring is drawn. */
  port: Port | null;
  highlight: string | null;
  badges?: boolean;
  /** Tallest the canvas grows (CSS length); it keeps the layout's shape, so no band is left empty. */
  maxHeight: string;
  /** Makes props clickable: called with the prop clicked. */
  onPick?: (prop: string) => void;
  /** What clicking the prop under the pointer would do. */
  pickLabel?: (prop: string) => string;
}

/**
 * The layout with one port's wiring drawn on it: from the controller through each prop's first
 * pixel to its last, a green dot where the data enters, and arrows for the way it runs. The
 * layout is drawn once and kept; only the path layer is drawn again, at most once a frame.
 */
export function WiringCanvas({ show, props, port, highlight, badges = false, maxHeight, onPick, pickLabel }: WiringCanvasProps) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const baseRef = useRef<HTMLCanvasElement>(null);
  const pathRef = useRef<HTMLCanvasElement>(null);
  const photo = useBackgroundImage(show.background?.path);
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  const [hover, setHover] = useState<{ prop: string; x: number; y: number } | null>(null);
  const frame = useRef(0);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setSize({ width: el.clientWidth, height: el.clientHeight }));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  // Kept while only the wiring changes (each edit brings a new show object).
  const bgKey = JSON.stringify(show.background ?? null);
  const bg = useMemo<Show["background"]>(() => JSON.parse(bgKey), [bgKey]);
  const { image, aspect } = photo;
  const box = useMemo(() => wiringBox(props, bg, aspect), [props, bg, aspect]);
  const view = useMemo(() => wiringView(box, size), [box, size]);
  const shape = box ? Math.min(4, Math.max(0.6, (box.maxX - box.minX) / Math.max(1e-6, box.maxY - box.minY))) : 16 / 9;

  useEffect(() => {
    if (baseRef.current) drawLayout(baseRef.current, size, view, props, bg, { image, aspect });
  }, [size, view, props, bg, image, aspect]);

  const lit = hover?.prop ?? highlight;
  useEffect(() => {
    const canvas = pathRef.current;
    if (!canvas) return;
    const id = requestAnimationFrame(() => drawPath(canvas, size, view, props, port, lit, badges));
    return () => cancelAnimationFrame(id);
  }, [size, view, props, port, lit, badges]);

  const propAt = (e: { clientX: number; clientY: number; currentTarget: HTMLElement }) => {
    const r = e.currentTarget.getBoundingClientRect();
    const at = { x: e.clientX - r.left, y: e.clientY - r.top };
    return { prop: pickAt(props, box, { width: r.width, height: r.height }, at), ...at };
  };
  const picking = onPick
    ? {
        "data-wire-canvas": "",
        onClick: (e: ReactMouseEvent<HTMLDivElement>) => {
          const { prop } = propAt(e);
          if (prop) onPick(prop);
        },
        onPointerMove: (e: ReactPointerEvent<HTMLDivElement>) => {
          const { clientX, clientY, currentTarget } = e;
          cancelAnimationFrame(frame.current);
          frame.current = requestAnimationFrame(() => {
            const { prop, x, y } = propAt({ clientX, clientY, currentTarget });
            setHover(prop ? { prop, x, y } : null);
          });
        },
        onPointerLeave: () => {
          cancelAnimationFrame(frame.current);
          setHover(null);
        },
      }
    : {};

  return (
    <div
      ref={wrapRef}
      className={`relative mx-auto max-w-full overflow-hidden ${onPick ? (hover ? "cursor-pointer" : "cursor-crosshair") : ""}`}
      style={{ aspectRatio: String(shape), width: `min(100%, calc(${maxHeight} * ${shape}))` }}
      {...picking}
    >
      <canvas ref={baseRef} data-size={size.width} className="absolute inset-0 block h-full w-full" aria-hidden />
      <canvas ref={pathRef} className="absolute inset-0 block h-full w-full" aria-hidden />
      {hover && pickLabel && (
        <div
          aria-hidden
          className="pointer-events-none absolute z-10 max-w-64 rounded bg-neutral-900/90 px-2 py-1 text-xs text-white shadow"
          style={{ left: Math.min(hover.x + 12, Math.max(0, size.width - 200)), top: hover.y + 14 }}
        >
          {pickLabel(hover.prop)}
        </div>
      )}
    </div>
  );
}

const portKeyOf = (r: PortRef | null) => (r ? `${r.controller}\u0000${r.port}\u0000${r.at ?? ""}` : null);

/**
 * The layout with the hovered or selected port's props lit and its wiring drawn; pointing at a
 * prop's table row lights its stretch of the wire.
 */
export function WiringPreview({ show, props, maxHeight }: { show: Show; props: PreviewProp[]; maxHeight: string }) {
  // Strings, so a pointer move over the same port re-renders nothing.
  const dragKey = useWiring((s) => (s.drag?.over?.kind === "port" ? portKeyOf(s.drag.over) : null));
  const selectedKey = useWiring((s) => portKeyOf(s.selected));
  const selectedProp = useWiring((s) => s.selected?.prop ?? null);
  const hoveredKey = useWiring((s) => portKeyOf(s.hovered));
  const hoveredProp = useWiring((s) => s.hoveredProp);
  // While dragging, the port under the pointer; otherwise the hovered port, or the selected row's.
  const focusKey = dragKey ?? hoveredKey ?? selectedKey;
  const focus = useMemo<PortRef | null>(() => {
    if (!focusKey) return null;
    const [controller, port, at] = focusKey.split("\u0000");
    return { controller, port: Number(port), at: at === "" ? undefined : Number(at) };
  }, [focusKey]);
  const port = focus ? findPort(show, focus) : null;
  const highlight = hoveredProp ?? selectedProp;
  const controller = focus && show.controllers.find((c) => c.id === focus.controller);
  const names = port?.slots.map((s) => show.props.find((p) => p.id === s.prop)?.name ?? "Missing prop") ?? [];
  return (
    <section aria-label="Wiring preview" className="overflow-hidden rounded-lg border border-neutral-200 bg-neutral-950 dark:border-neutral-800">
      <WiringCanvas show={show} props={props} port={port} highlight={port?.slots.some((s) => s.prop === highlight) ? highlight : null} maxHeight={maxHeight} />
      <p className="bg-white px-3 py-2 text-xs text-neutral-600 dark:bg-neutral-900 dark:text-neutral-400" aria-live="polite" data-testid="wiring-preview-caption">
        {!port || !controller
          ? "Point at a port to see its wiring here."
          : names.length === 0
            ? `Port ${port.number} on ${controller.name} has nothing wired yet.`
            : `Port ${port.number} on ${controller.name}: ${names.join(" → ")}. The data enters at the green dot and runs the way the arrows point.`}
      </p>
    </section>
  );
}
