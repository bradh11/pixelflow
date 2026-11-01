import { useEffect, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import type { SequenceTarget } from "../../api/sequence";
import type { PreviewProp, PreviewSet3d, Show } from "../../api/types";
import { BACKDROP, COLORS, fitDisplay } from "../../lib/flatPreview";
import { backgroundBox, boxOfPoints, toScreen, unionBox } from "../../lib/layoutMath";
import { type PixelColors, drawPixels } from "../../lib/pixelBatches";
import { drawLabels, flatSpots } from "../../lib/propLabels";
import { targetNodes, targetPreview } from "../../lib/submodels";
import { DOT_SCALES, boostFrame, hidesUnlit } from "../../lib/viewOptions";
import { useApp } from "../../state/store";
import { type LayoutMode, showViewKey, useView3d } from "../../state/view3d";
import { type PhotoImage, useBackgroundImage, usePreviewProps, usePreviewProps3d } from "../layout/useLayoutData";
import { Layout3dView } from "../layout3d/Layout3dView";
import { FlatViewControls } from "../layout3d/View3dControls";

const NONE: ReadonlySet<string> = new Set();
/** The previews' colors with pixels that are off left out (the viewer's Unlit pixels: Hidden). */
const ONLY_LIT: PixelColors = { unlit: COLORS.unlit, selected: COLORS.selected };

/**
 * The display with its lights in `frame`'s colors, filling the room it's given: flat like the
 * Layout screen (the photo behind, dimmed) or in 3D (look only, with the show's camera from the
 * Layout screen). Every preview draws with it: the Sequence screen's, the Play screen's, and the
 * preview's own window. `only`: just the pixels of one sequence row's target (only a submodel's
 * own pixels, for a row on a submodel), without the photo. `playing`: the show's time is moving
 * (with the viewer's Unlit pixels on Auto, pixels that are off are hidden then).
 */
export function PreviewStage({
  frame,
  mode,
  only = null,
  playing = false,
  label = "Preview of the show at the playhead",
}: {
  frame: Uint8Array | null;
  mode: LayoutMode;
  only?: SequenceTarget | null;
  playing?: boolean;
  /** The flat picture's name. */
  label?: string;
}) {
  const snapshot = useApp((s) => s.snapshot);
  const show = snapshot?.show;
  const preview = usePreviewProps();
  const photo = useBackgroundImage(show?.background?.path);
  const in3d = mode === "3d";
  const preview3d = usePreviewProps3d(in3d);

  // The row's pixels, by prop: whole props, a submodel's pixels, or a group's members.
  const segments = useMemo(() => {
    if (!only || !show) return null;
    const flat = new Map(preview.props.map((p) => [p.prop, p.points]));
    return targetNodes(
      show,
      only,
      (prop) => (flat.get(prop.id)?.length ?? 0) >> 1,
      (prop) => flat.get(prop.id) ?? [],
    );
  }, [only, show, preview.props]);
  const props: PreviewProp[] = useMemo(() => (segments ? targetPreview(preview.props, segments, "2d") : preview.props), [segments, preview.props]);
  const shown3d: PreviewSet3d = useMemo(
    () => (segments ? { revision: preview3d.revision, props: targetPreview(preview3d.props, segments, "3d") } : preview3d),
    [segments, preview3d],
  );
  // Just the row: without the photo, as in 2D.
  const photo3d: PhotoImage = useMemo(() => (only ? { ...photo, image: null } : photo), [only, photo]);

  return (
    <div className="relative h-full w-full">
      {in3d && snapshot ? (
        <Layout3dView preview={shown3d} show={snapshot.show} photo={photo3d} storageKey={showViewKey(snapshot.path, snapshot.show.name)} frame={frame} playing={playing} />
      ) : (
        <FlatPreview props={props} frame={frame} playing={playing} show={show} photo={photo} onlyRow={only !== null} label={label} />
      )}
      {preview.props.length === 0 && (
        <p className="pointer-events-none absolute inset-0 flex items-center justify-center p-4 text-center text-sm text-neutral-400">Add props on the Layout screen to see them here.</p>
      )}
    </div>
  );
}

/**
 * The props drawn flat, front on, as on the Layout screen (with the photo behind, dimmed), as the
 * viewer likes to look at them (glow and the View options), with the View options over its corner.
 */
function FlatPreview({
  props,
  frame,
  playing,
  show,
  photo,
  onlyRow,
  label,
}: {
  props: PreviewProp[];
  frame: Uint8Array | null;
  playing: boolean;
  show: Show | undefined;
  photo: PhotoImage;
  onlyRow: boolean;
  label: string;
}) {
  const { glow, names, unlit, dotSize, boost } = useView3d(useShallow((s) => ({ glow: s.glow, names: s.names, unlit: s.unlit, dotSize: s.dotSize, boost: s.boost })));
  const hideOff = hidesUnlit(unlit, playing);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  /** The top-left corner of the night sky behind the display, where the View options sit. */
  const [corner, setCorner] = useState<{ x: number; y: number } | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const measure = () => setSize({ width: canvas.clientWidth, height: canvas.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx || size.width === 0) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(size.width * ratio);
    canvas.height = Math.round(size.height * ratio);
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, size.width, size.height);
    const bg = show?.background ?? null;
    const box = unionBox([...props.map((p) => boxOfPoints(p.points)), bg && !onlyRow ? backgroundBox(bg, photo.aspect) : null]);
    const fitted = fitDisplay(box, size);
    const view = fitted.view;
    const dotScale = DOT_SCALES[dotSize];
    // The night sky only behind the display itself (around it, the panel shows through), and the
    // lights inside it: a beam reaching past it is cut off at its edge.
    ctx.fillStyle = BACKDROP;
    ctx.save();
    if (box) {
      const a = toScreen(view, size, { x: box.minX, y: box.maxY });
      const b = toScreen(view, size, { x: box.maxX, y: box.minY });
      const pad = 8;
      ctx.beginPath();
      ctx.roundRect(a.x - pad, a.y - pad, b.x - a.x + 2 * pad, b.y - a.y + 2 * pad, 6);
      ctx.fill();
      ctx.clip();
      const at = { x: Math.max(0, Math.round(a.x - pad)), y: Math.max(0, Math.round(a.y - pad)) };
      setCorner((was) => (was?.x === at.x && was.y === at.y ? was : at));
    } else {
      ctx.fillRect(0, 0, size.width, size.height);
      setCorner(null);
    }
    if (bg && photo.image && !onlyRow) {
      const b = backgroundBox(bg, photo.aspect);
      const tl = toScreen(view, size, { x: b.minX, y: b.maxY });
      const br = toScreen(view, size, { x: b.maxX, y: b.minY });
      // Dimmed, so the lights stand out as they would at night.
      ctx.globalAlpha = bg.opacity * 0.5;
      ctx.drawImage(photo.image, tl.x, tl.y, br.x - tl.x, br.y - tl.y);
      ctx.globalAlpha = 1;
    }
    // Dim levels lifted when the viewer boosts them: only what's drawn, never the frame.
    const shown = frame && boost > 0 ? boostFrame(frame, props, boost) : frame;
    drawPixels(ctx, props, shown, view, size, NONE, hideOff ? ONLY_LIT : COLORS, fitted.radius * dotScale, ratio, glow, dotScale);
    ctx.restore();
    if (names && show) drawLabels(ctx, flatSpots(props, new Map(show.props.map((p) => [p.id, p.name])), view, size), size);
  }, [props, frame, size, show, photo, onlyRow, glow, names, hideOff, dotSize, boost]);

  return (
    <>
      <canvas ref={canvasRef} role="img" aria-label={label} className="h-full w-full rounded-md" />
      <FlatViewControls at={corner} />
    </>
  );
}
