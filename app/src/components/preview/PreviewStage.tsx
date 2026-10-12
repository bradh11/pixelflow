import { useEffect, useMemo, useRef, useState } from "react";
import type { SequenceTarget } from "../../api/sequence";
import type { PreviewProp, PreviewSet3d, Show } from "../../api/types";
import { BACKDROP, COLORS, fitDisplay } from "../../lib/flatPreview";
import { backgroundBox, boxOfPoints, toScreen, unionBox } from "../../lib/layoutMath";
import { drawPixels } from "../../lib/pixelBatches";
import { targetNodes, targetPreview } from "../../lib/submodels";
import { useApp } from "../../state/store";
import { type LayoutMode, showViewKey, useView3d } from "../../state/view3d";
import { type PhotoImage, useBackgroundImage, usePreviewProps, usePreviewProps3d } from "../layout/useLayoutData";
import { Layout3dView } from "../layout3d/Layout3dView";

const NONE: ReadonlySet<string> = new Set();

/**
 * The display with its lights in `frame`'s colors, filling the room it's given: flat like the
 * Layout screen (the photo behind, dimmed) or in 3D (look only, with the show's camera from the
 * Layout screen). Every preview draws with it: the Sequence screen's, the Play screen's, and the
 * preview's own window. `only`: just the pixels of one sequence row's target (only a submodel's
 * own pixels, for a row on a submodel), without the photo.
 */
export function PreviewStage({
  frame,
  mode,
  only = null,
  label = "Preview of the show at the playhead",
}: {
  frame: Uint8Array | null;
  mode: LayoutMode;
  only?: SequenceTarget | null;
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
        <Layout3dView preview={shown3d} show={snapshot.show} photo={photo3d} storageKey={showViewKey(snapshot.path, snapshot.show.name)} frame={frame} />
      ) : (
        <FlatPreview props={props} frame={frame} show={show} photo={photo} onlyRow={only !== null} label={label} />
      )}
      {preview.props.length === 0 && (
        <p className="pointer-events-none absolute inset-0 flex items-center justify-center p-4 text-center text-sm text-neutral-400">Add props on the Layout screen to see them here.</p>
      )}
    </div>
  );
}

/** The props drawn flat, front on, as on the Layout screen (with the photo behind, dimmed). */
function FlatPreview({
  props,
  frame,
  show,
  photo,
  onlyRow,
  label,
}: {
  props: PreviewProp[];
  frame: Uint8Array | null;
  show: Show | undefined;
  photo: PhotoImage;
  onlyRow: boolean;
  label: string;
}) {
  const glow = useView3d((s) => s.glow);
  const canvasRef = useRef<HTMLCanvasElement>(null);
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
    const { view, radius } = fitDisplay(box, size);
    // The night sky only behind the display itself (around it, the panel shows through).
    ctx.fillStyle = BACKDROP;
    if (box) {
      const a = toScreen(view, size, { x: box.minX, y: box.maxY });
      const b = toScreen(view, size, { x: box.maxX, y: box.minY });
      const pad = 8;
      ctx.beginPath();
      ctx.roundRect(a.x - pad, a.y - pad, b.x - a.x + 2 * pad, b.y - a.y + 2 * pad, 6);
      ctx.fill();
    } else {
      ctx.fillRect(0, 0, size.width, size.height);
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
    drawPixels(ctx, props, frame, view, size, NONE, COLORS, radius, ratio, glow);
  }, [props, frame, size, show?.background, photo, onlyRow, glow]);

  return <canvas ref={canvasRef} role="img" aria-label={label} className="h-full w-full rounded-md" />;
}
