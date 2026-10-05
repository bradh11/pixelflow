import { type PointerEvent as ReactPointerEvent, type Ref, useCallback, useEffect, useId, useImperativeHandle, useRef, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { PreviewProp3d, PreviewSet3d, Show } from "../../api/types";
import {
  type Box3,
  type GizmoHandle,
  type Orbit,
  type PixelPalette,
  type Ray,
  type V3,
  backdropBox,
  boundsOfXyz,
  boxCenter3,
  composeGestures3d,
  dollyAt,
  dragDelta,
  fillColors,
  fitOrbit,
  focusOrbit,
  freeDragHandle,
  gizmoHit,
  gizmoLength,
  moveGesture3,
  orbitBy,
  packPositions,
  panBy3,
  pickPixel,
  presetOrbit,
  propsInRect,
  screenRay,
  stepOrbit,
  sub,
  typicalSpacing,
  unionBox3,
  v3,
  viewProjection,
} from "../../lib/layout3d";
import { type Gesture, type Pt, type Size, pinchFactor, wheelIntent, wheelZoomFactor } from "../../lib/layoutMath";
import { useLayoutEditor } from "../../state/layoutEditor";
import { commitGesture, settlePending, unsettled } from "../../state/layoutGestures";
import { useApp } from "../../state/store";
import { type CameraAction, loadShowView, saveShowView, useView3d } from "../../state/view3d";
import { type LayoutCanvasHandle, SelectionAnnouncer } from "../layout/LayoutCanvas";
import { type PhotoImage, useLiveFrame } from "../layout/useLayoutData";
import { type Scene3d, type SceneFactory, loadThreeScene, readModel } from "./scene";
import { View3dControls } from "./View3dControls";

/** How close (screen pixels) a click must be to a pixel to pick its prop. */
const HIT_PX = 8;
/** Drags shorter than this (screen pixels) count as clicks. */
const CLICK_PX = 4;
/** Bulbs are this fraction of the usual gap between pixels across. */
const BULB_PER_SPACING = 0.42;
const PALETTE: PixelPalette = { unlit: [96, 96, 104], selected: [167, 139, 250] };
/** The renderer draws at most this many device pixels per CSS pixel (sharper costs more than it shows). */
const MAX_RATIO = 2;

type Drag =
  | { kind: "orbit"; last: Pt; from: Pt; moved: boolean; clear: boolean }
  | { kind: "pan"; last: Pt }
  /**
   * Moving the selection: by a gizmo handle, or grabbed by a pixel (`grab`, dragged over the
   * ground or up the house, or onto the house model's surface).
   */
  | {
      kind: "move";
      ids: string[];
      handle: GizmoHandle;
      origin: V3;
      start: Ray;
      from: Pt;
      moved: boolean;
      delta: V3;
      grab: V3 | null;
      narrowTo: string | null;
      deselect: string | null;
    }
  | { kind: "marquee"; from: Pt; to: Pt; additive: string[] };

/**
 * Renderers by canvas, with how many views use each. A renderer takes its canvas's one WebGL
 * context, so a canvas only ever gets one: React's development check mounts a view twice on the
 * same canvas, and a second renderer would share the context that letting go of the first one
 * loses. A renderer is let go of once no view has used it for a moment.
 */
const renderers = new WeakMap<HTMLCanvasElement, { scene: Promise<Scene3d>; users: number }>();

function takeRenderer(canvas: HTMLCanvasElement, make: SceneFactory): Promise<Scene3d> {
  let entry = renderers.get(canvas);
  if (!entry) {
    entry = { scene: make(canvas), users: 0 };
    renderers.set(canvas, entry);
  }
  entry.users++;
  return entry.scene;
}

function releaseRenderer(canvas: HTMLCanvasElement) {
  const entry = renderers.get(canvas);
  if (!entry || --entry.users > 0) return;
  // Mounted again straight away (the development check): keep it.
  queueMicrotask(() => {
    if (entry.users > 0 || renderers.get(canvas) !== entry) return;
    renderers.delete(canvas);
    entry.scene.then(
      (scene) => scene.dispose(),
      () => {},
    );
  });
}

/** WebKit's pinch events (Safari and the macOS app). */
type PinchEvent = Event & { scale: number; clientX: number; clientY: number };

interface Layout3dViewProps {
  preview: PreviewSet3d;
  show: Show;
  photo: PhotoImage;
  /** Remembers the camera for this show on this computer (see `showViewKey`). */
  storageKey: string;
  /** Select and move props; otherwise the view is only looked around. */
  editable?: boolean;
  /** Live colors given by the screen; without them the view fetches its own. */
  frame?: Uint8Array | null;
  /** Makes the renderer (tests give a stand-in). */
  sceneFactory?: SceneFactory;
  ref?: Ref<LayoutCanvasHandle>;
}

/**
 * The display in 3D, lit with live colors: orbit, pan, and zoom around it, pick a view, and
 * (when editable) select props and move them in depth with the gizmo or by dragging them.
 * Like the 2D canvas, every finished drag is one batch of edits (one undo step), and camera
 * moves, selection, and live colors only redraw: they never re-render React.
 */
export function Layout3dView({ preview, show, photo, storageKey, editable = false, frame, sceneFactory = loadThreeScene, ref }: Layout3dViewProps) {
  const helpId = useId();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const marqueeRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<Scene3d | null>(null);
  const [ready, setReady] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [modelProblem, setModelProblem] = useState<string | null>(null);
  const backend = useApp((s) => s.backend);
  /** The house model's box where it's placed, once loaded. */
  const modelBox = useRef<Box3 | null>(null);
  const camera = useRef<{ current: Orbit; goal: Orbit } | null>(null);
  const drag = useRef<Drag | null>(null);
  const liveFrame = useRef<Uint8Array | null>(null);
  const hover = useRef<GizmoHandle | null>(null);
  const spaceHeld = useRef(false);
  const tick = useRef<{ request: number | null; last: number }>({ request: null, last: 0 });
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  /** A drag sticking to the house model: the latest ray, raycast against the model once a frame. */
  const surfaceDrag = useRef<{ ray: Ray; request: number | null } | null>(null);
  /** The key the view's settings were last read from (see the `storageKey` effect). */
  const shownKey = useRef(storageKey);
  /** The props as `effective` last worked them out, and from what. */
  const effectiveMemo = useRef<{ from: unknown[]; props: PreviewProp3d[] } | null>(null);
  const selectedMemo = useRef<{ props: PreviewProp3d[]; selected: string[]; box: Box3 | null } | null>(null);
  /** What the renderer has: the preview it was given, each prop's uploaded pixels, and where they start. */
  const uploaded = useRef<{ preview: PreviewSet3d | null; xyz: Map<string, Float32Array>; starts: Map<string, { start: number; count: number }>; colors: Uint8Array }>({
    preview: null,
    xyz: new Map(),
    starts: new Map(),
    colors: new Uint8Array(0),
  });

  const latest = useRef({ preview, show, photo, editable, frame, storageKey });
  latest.current = { preview, show, photo, editable, frame, storageKey };

  const size = (): Size => {
    const canvas = canvasRef.current;
    return { width: canvas?.clientWidth ?? 0, height: canvas?.clientHeight ?? 0 };
  };

  /**
   * The props as they should look now: with gestures still on their way, held arrow keys, and
   * any drag. Worked out again only when one of those changes (a drag asks several times a move).
   */
  const effective = (): PreviewProp3d[] => {
    const { preview } = latest.current;
    const st = useLayoutEditor.getState();
    const d = drag.current;
    const moving = d?.kind === "move" && d.moved ? d : null;
    const from = [preview, st.pending, st.nudge, moving?.ids, moving?.delta];
    const memo = effectiveMemo.current;
    if (memo && memo.from.every((v, i) => v === from[i])) return memo.props;
    const layers: { ids: string[]; gesture: Gesture }[] = unsettled(st.pending, preview.revision);
    if (st.nudge) layers.push({ ids: st.nudge.ids, gesture: { kind: "move", dx: st.nudge.dx, dy: st.nudge.dy } });
    if (moving) layers.push({ ids: moving.ids, gesture: moveGesture3(moving.delta) });
    const props = composeGestures3d(preview.props, layers);
    effectiveMemo.current = { from, props };
    return props;
  };

  const backdrop = (): Box3 | null => {
    const { show, photo } = latest.current;
    const bg = show.background;
    return bg && photo.image ? backdropBox(bg, photo.aspect, -useView3d.getState().photoDepth) : null;
  };

  /** Everything worth showing: the props, the photo, and the house model. */
  const contentBox = (props = effective()): Box3 | null => unionBox3([boundsOfXyz(props.map((p) => p.xyz)), backdrop(), modelBox.current]);

  const selectedBox = (props: PreviewProp3d[]): Box3 | null => {
    const { selected } = useLayoutEditor.getState();
    const memo = selectedMemo.current;
    if (memo && memo.props === props && memo.selected === selected) return memo.box;
    const ids = new Set(selected);
    const box = boundsOfXyz(props.filter((p) => ids.has(p.prop)).map((p) => p.xyz));
    selectedMemo.current = { props, selected, box };
    return box;
  };

  /** The camera as it's drawn (null until there's a size and something to show). */
  const view = (): Orbit | null => camera.current?.current ?? null;

  const saveCamera = () => {
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      if (camera.current) saveShowView(latest.current.storageKey, { orbit: camera.current.goal });
    }, 400);
  };

  /** Draws on the next animation frame, gliding the camera toward where it's headed. */
  const invalidate = useCallback(() => {
    if (tick.current.request !== null) return;
    const step = (now: number) => {
      tick.current.request = null;
      const scene = sceneRef.current;
      const cam = camera.current;
      if (!scene || !cam) return;
      const dt = tick.current.last ? Math.min(0.1, (now - tick.current.last) / 1000) : 1 / 60;
      tick.current.last = now;
      cam.current = stepOrbit(cam.current, cam.goal, dt);
      drawSelection(scene, cam.current);
      scene.render(cam.current);
      if (cam.current !== cam.goal) tick.current.request = requestAnimationFrame(step);
      else {
        tick.current.last = 0;
        saveCamera();
      }
    };
    if (typeof requestAnimationFrame === "function") tick.current.request = requestAnimationFrame(step);
    // Every helper used here reads `latest`, the stores, or refs, so the function never needs to change.
  }, []);

  /** Moves the camera to `goal`, gliding there; `now` jumps instead. */
  const moveCamera = (goal: Orbit, now = false) => {
    if (!camera.current) camera.current = { current: goal, goal };
    else camera.current = { current: now ? goal : camera.current.current, goal };
    invalidate();
  };

  /** The selection outline and the gizmo, sized for the camera `o`. */
  function drawSelection(scene: Scene3d, o: Orbit) {
    const { editable } = latest.current;
    // Only the editor shows what's selected.
    const box = editable ? selectedBox(effective()) : null;
    scene.setSelectionBox(box);
    if (!box) return scene.setGizmo(null);
    const d = drag.current;
    const origin = d?.kind === "move" && !d.grab ? v3(d.origin.x + d.delta.x, d.origin.y + d.delta.y, d.origin.z + d.delta.z) : boxCenter3(box);
    const active = d?.kind === "move" && !d.grab ? d.handle : hover.current;
    scene.setGizmo({ origin, length: gizmoLength(o, size(), origin), highlight: active });
  }

  /** Sends the props' positions to the renderer: all of them for new positions, otherwise only props that moved. */
  const syncPositions = () => {
    const scene = sceneRef.current;
    if (!scene) return;
    const props = effective();
    const up = uploaded.current;
    if (up.preview !== latest.current.preview) {
      const packed = packPositions(props);
      scene.setPixels(packed.xyz);
      up.preview = latest.current.preview;
      up.starts = packed.starts;
      up.xyz = new Map(props.map((p) => [p.prop, p.xyz]));
      up.colors = new Uint8Array(packed.xyz.length);
      const spacing = typicalSpacing(props);
      scene.setBulbSize(spacing ? spacing * BULB_PER_SPACING : 0.04);
      syncColors();
      return;
    }
    for (const p of props) {
      if (up.xyz.get(p.prop) === p.xyz) continue;
      const at = up.starts.get(p.prop);
      if (at) scene.updatePixels(at.start, p.xyz.subarray(0, at.count * 3));
      up.xyz.set(p.prop, p.xyz);
    }
  };

  /** Colors every pixel: live colors while something plays, otherwise unlit (selected props tinted). */
  const syncColors = () => {
    const scene = sceneRef.current;
    if (!scene) return;
    const up = uploaded.current;
    const f = latest.current.frame !== undefined ? latest.current.frame : liveFrame.current;
    const selected = new Set(latest.current.editable ? useLayoutEditor.getState().selected : []);
    fillColors(latest.current.preview.props, f ?? null, selected, PALETTE, up.colors);
    scene.setColors(up.colors, !!f);
  };

  const syncBackdrop = () => {
    const scene = sceneRef.current;
    if (!scene) return;
    const { show, photo } = latest.current;
    const box = backdrop();
    scene.setBackdrop(box && photo.image && show.background ? { image: photo.image, box, opacity: show.background.opacity } : null);
  };

  /** Sets the camera once there's a size and the props have arrived: as remembered for this show, or fitted. */
  const placeCameraIfNeeded = () => {
    if (camera.current || !sceneRef.current) return;
    const s = size();
    const { show, preview } = latest.current;
    if (s.width <= 0 || s.height <= 0 || (show.props.length > 0 && preview.props.length === 0)) return;
    const saved = loadShowView(latest.current.storageKey).orbit;
    moveCamera(saved ?? fitOrbit(contentBox(), s), true);
  };

  const runCamera = (action: CameraAction) => {
    const cam = camera.current;
    if (!cam) return;
    const s = size();
    if (action.kind === "fit") moveCamera(fitOrbit(contentBox(), s, cam.goal.yaw, cam.goal.pitch));
    else if (action.kind === "preset") moveCamera(presetOrbit(action.preset, contentBox(), s));
    else moveCamera(dollyAt(cam.goal, s, null, 1 / action.factor));
  };

  // The renderer: made once, on first showing.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let cancelled = false;
    takeRenderer(canvas, sceneFactory).then(
      (scene) => {
        if (cancelled) return;
        sceneRef.current = scene;
        const { bloom, ground } = useView3d.getState();
        scene.setOptions({ bloom, ground });
        const s = size();
        scene.resize(s, Math.min(MAX_RATIO, window.devicePixelRatio || 1));
        setReady(true);
      },
      () => !cancelled && setProblem("The 3D view needs WebGL, which isn't available here. The 2D view still works."),
    );
    return () => {
      cancelled = true;
      if (tick.current.request !== null && typeof cancelAnimationFrame === "function") cancelAnimationFrame(tick.current.request);
      tick.current.request = null;
      dropSurfaceDrag();
      clearTimeout(saveTimer.current);
      if (camera.current) saveShowView(latest.current.storageKey, { orbit: camera.current.goal });
      releaseRenderer(canvas);
      sceneRef.current = null;
      // Taken again (the development check), the renderer gets everything afresh.
      uploaded.current.preview = null;
      setReady(false);
    };
  }, [sceneFactory]);

  // New positions (or the renderer just made): send them, then the camera if it isn't set yet.
  useEffect(() => {
    if (!ready) return;
    settlePending(preview.revision);
    syncPositions();
    placeCameraIfNeeded();
    invalidate();
  }, [ready, preview, invalidate]);

  // Another show: its own remembered camera and photo depth. The same show saved under a new
  // name keeps the camera it has (its settings were carried to the new key when it was saved).
  useEffect(() => useView3d.getState().openShow(storageKey), [storageKey]);
  useEffect(() => {
    if (!ready) return;
    const { carried } = useView3d.getState();
    const saved = carried?.from === shownKey.current && carried.to === storageKey;
    shownKey.current = storageKey;
    if (saved && camera.current) {
      saveShowView(storageKey, { orbit: camera.current.goal });
      return;
    }
    camera.current = null;
    placeCameraIfNeeded();
  }, [ready, storageKey]);

  useEffect(() => {
    if (!ready) return;
    syncBackdrop();
    invalidate();
  }, [ready, photo.image, photo.aspect, show.background, invalidate]);

  // The house model: loaded when it changes, placed whenever it moves.
  const modelPath = show.houseModel?.path ?? null;
  const placeModel = () => {
    const m = latest.current.show.houseModel;
    if (m && sceneRef.current) modelBox.current = sceneRef.current.placeModel(m);
  };
  useEffect(() => {
    const scene = sceneRef.current;
    if (!ready || !scene) return;
    let cancelled = false;
    modelBox.current = null;
    setModelProblem(null);
    const forget = () => useView3d.setState({ loadedModel: null });
    if (!modelPath || !backend) {
      void scene.setModel(null).then(invalidate);
      return forget;
    }
    void (async () => {
      try {
        const bytes = await readModel(backend, modelPath);
        if (cancelled) return;
        const natural = await scene.setModel({ bytes, name: modelPath });
        if (cancelled) return;
        useView3d.setState({ loadedModel: natural ? { path: modelPath, natural } : null });
        placeModel();
        invalidate();
      } catch (e) {
        // Messages from reading and parsing the file are already in plain words.
        if (!cancelled) setModelProblem(`The house model can't be shown. ${errorMessage(e)}`);
      }
    })();
    return () => {
      cancelled = true;
      forget();
    };
  }, [ready, modelPath, backend, invalidate]);
  useEffect(() => {
    if (!ready) return;
    placeModel();
    invalidate();
  }, [ready, show.houseModel, invalidate]);

  // Colors handed in by the screen.
  useEffect(() => {
    if (!ready || frame === undefined) return;
    syncColors();
    invalidate();
  }, [ready, frame, invalidate]);

  // Colors fetched here, when the screen doesn't hand them in.
  useLiveFrame(
    useCallback(
      (f: Uint8Array | null) => {
        if (f === null && liveFrame.current === null) return;
        liveFrame.current = f;
        syncColors();
        invalidate();
      },
      [invalidate],
    ),
    frame === undefined,
  );

  // Selection, tool, and gestures on their way only need a redraw.
  useEffect(
    () =>
      useLayoutEditor.subscribe((st, prev) => {
        if (st.pending !== prev.pending || st.nudge !== prev.nudge) syncPositions();
        if (st.selected !== prev.selected) syncColors();
        invalidate();
      }),
    [invalidate],
  );

  // View settings and camera requests.
  useEffect(
    () =>
      useView3d.subscribe((st, prev) => {
        const scene = sceneRef.current;
        if (!scene) return;
        if (st.bloom !== prev.bloom || st.ground !== prev.ground) scene.setOptions({ bloom: st.bloom, ground: st.ground });
        if (st.photoDepth !== prev.photoDepth) syncBackdrop();
        if (st.command && st.command !== prev.command) runCamera(st.command.action);
        invalidate();
      }),
    [invalidate],
  );

  // The renderer follows the canvas's size.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !ready) return;
    const observer = new ResizeObserver(() => {
      sceneRef.current?.resize(size(), Math.min(MAX_RATIO, window.devicePixelRatio || 1));
      placeCameraIfNeeded();
      invalidate();
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [ready, invalidate]);

  const point = (e: { clientX: number; clientY: number }): Pt => {
    const rect = canvasRef.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  // Wheel and pinch: zoom toward the pointer; two-finger scrolls orbit (with Shift, pan).
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let pinching = false;
    let lastScale = 1;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const cam = camera.current;
      if (!cam || (pinching && e.ctrlKey)) return;
      const s = size();
      if (wheelIntent(e) === "zoom") moveCamera(dollyAt(cam.goal, s, point(e), 1 / wheelZoomFactor(e)));
      else if (e.shiftKey) moveCamera(panBy3(cam.goal, s, -e.deltaX, -e.deltaY));
      else moveCamera(orbitBy(cam.goal, -e.deltaX * 0.5, -e.deltaY * 0.5));
    };
    const onPinchStart = (e: Event) => {
      e.preventDefault();
      pinching = true;
      lastScale = 1;
    };
    const onPinch = (e: Event) => {
      e.preventDefault();
      const cam = camera.current;
      if (!cam) return;
      const { scale, clientX, clientY } = e as PinchEvent;
      const factor = pinchFactor(lastScale, scale);
      if (scale > 0) lastScale = scale;
      const at = Number.isFinite(clientX) && Number.isFinite(clientY) ? point({ clientX, clientY }) : null;
      moveCamera(dollyAt(cam.goal, size(), at, 1 / factor));
    };
    const onPinchEnd = (e: Event) => {
      e.preventDefault();
      pinching = false;
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("gesturestart", onPinchStart);
    canvas.addEventListener("gesturechange", onPinch);
    canvas.addEventListener("gestureend", onPinchEnd);
    return () => {
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("gesturestart", onPinchStart);
      canvas.removeEventListener("gesturechange", onPinch);
      canvas.removeEventListener("gestureend", onPinchEnd);
    };
  }, []);

  // Space held: drag to pan (in the editor; a look-only view leaves Space to its screen, which may
  // play and pause with it).
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (!latest.current.editable) return;
      if (e.key === " " && (e.target === canvasRef.current || e.target === document.body) && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        spaceHeld.current = true;
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === " ") spaceHeld.current = false;
    };
    const release = () => (spaceHeld.current = false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", release);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", release);
    };
  }, []);

  const showMarquee = (d: Extract<Drag, { kind: "marquee" }> | null) => {
    const el = marqueeRef.current;
    if (!el) return;
    el.style.display = d ? "block" : "none";
    if (!d) return;
    Object.assign(el.style, {
      left: `${Math.min(d.from.x, d.to.x)}px`,
      top: `${Math.min(d.from.y, d.to.y)}px`,
      width: `${Math.abs(d.to.x - d.from.x)}px`,
      height: `${Math.abs(d.to.y - d.from.y)}px`,
    });
  };

  const setCursor = (cursor: string) => {
    if (canvasRef.current) canvasRef.current.style.cursor = cursor;
  };

  /** Stops waiting to snap a drag to the house model. */
  const dropSurfaceDrag = () => {
    const pending = surfaceDrag.current;
    if (pending?.request != null && typeof cancelAnimationFrame === "function") cancelAnimationFrame(pending.request);
    surfaceDrag.current = null;
  };

  /** Moves the dragged selection to where the camera ray `now` points: onto the house model's surface, if `snap` and it's hit. */
  const followRay = (d: Extract<Drag, { kind: "move" }>, now: Ray, snap: boolean) => {
    const st = useLayoutEditor.getState();
    const grid = st.snap ? st.grid : null;
    const surface = snap && d.grab ? sceneRef.current?.surfaceAt(now) : null;
    const delta = surface && d.grab ? sub(surface, d.grab) : dragDelta(d.handle, d.origin, d.start, now, grid);
    if (!delta) return;
    d.delta = delta;
    syncPositions();
    invalidate();
  };

  /** Snaps the drag to the house model for the latest ray, now. */
  const flushSurfaceDrag = () => {
    const pending = surfaceDrag.current;
    dropSurfaceDrag();
    const d = drag.current;
    if (pending && d?.kind === "move") followRay(d, pending.ray, true);
  };

  const cancel = () => {
    const d = drag.current;
    if (!d) return false;
    dropSurfaceDrag();
    drag.current = null;
    showMarquee(null);
    syncPositions();
    invalidate();
    return true;
  };
  useImperativeHandle(ref, () => ({ cancel }));

  const onPointerDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const cam = view();
    if (!cam || (e.button !== 0 && e.button !== 1 && e.button !== 2)) return;
    const canvas = canvasRef.current!;
    canvas.focus({ preventScroll: true });
    try {
      canvas.setPointerCapture?.(e.pointerId);
    } catch {
      // A pointer the browser no longer tracks can't be captured; the drag still works inside the canvas.
    }
    const s = point(e);
    const st = useLayoutEditor.getState();
    const { editable, show } = latest.current;
    const sz = size();
    if (e.button !== 0 || spaceHeld.current) {
      drag.current = { kind: "pan", last: s };
      setCursor("grabbing");
      return;
    }
    if (!editable) {
      drag.current = { kind: "orbit", last: s, from: s, moved: false, clear: false };
      return;
    }
    const props = effective();
    const box = selectedBox(props);
    if (box) {
      const origin = boxCenter3(box);
      const handle = gizmoHit(cam, sz, origin, s);
      if (handle) {
        drag.current = { kind: "move", ids: st.selected, handle, origin, start: screenRay(cam, sz, s), from: s, moved: false, delta: v3(0, 0, 0), grab: null, narrowTo: null, deselect: null };
        return;
      }
    }
    const hit = pickPixel(props, viewProjection(cam, sz), sz, s, HIT_PX);
    if (hit) {
      const startMove = (ids: string[], narrowTo: string | null = null, deselect: string | null = null) => {
        drag.current = {
          kind: "move",
          ids: ids.filter((id) => show.props.some((p) => p.id === id)),
          handle: freeDragHandle(cam),
          origin: hit.point,
          start: screenRay(cam, sz, s),
          from: s,
          moved: false,
          delta: v3(0, 0, 0),
          grab: hit.point,
          narrowTo,
          deselect,
        };
      };
      if (e.shiftKey) {
        if (st.selected.includes(hit.prop)) startMove(st.selected, null, hit.prop);
        else {
          st.toggle(hit.prop);
          startMove(useLayoutEditor.getState().selected);
        }
      } else if (!st.selected.includes(hit.prop)) {
        st.select([hit.prop]);
        startMove([hit.prop]);
      } else startMove(st.selected, st.selected.length > 1 ? hit.prop : null);
      return;
    }
    if (e.shiftKey) {
      drag.current = { kind: "marquee", from: s, to: s, additive: st.selected };
      return;
    }
    drag.current = { kind: "orbit", last: s, from: s, moved: false, clear: true };
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const cam = view();
    if (!cam) return;
    const s = point(e);
    const d = drag.current;
    const sz = size();
    if (!d) {
      // Hovering: light up the gizmo handle under the pointer.
      if (!latest.current.editable) return;
      const box = selectedBox(effective());
      const handle = box ? gizmoHit(cam, sz, boxCenter3(box), s) : null;
      if (handle !== hover.current) {
        hover.current = handle;
        setCursor(handle ? "pointer" : "default");
        invalidate();
      }
      return;
    }
    if (d.kind === "orbit" || d.kind === "pan") {
      const [dx, dy] = [s.x - d.last.x, s.y - d.last.y];
      d.last = s;
      if (d.kind === "orbit") {
        if (!d.moved && Math.hypot(s.x - d.from.x, s.y - d.from.y) < CLICK_PX) return;
        d.moved = true;
        setCursor("grabbing");
        moveCamera(orbitBy(camera.current!.goal, dx, dy));
      } else moveCamera(panBy3(camera.current!.goal, sz, dx, dy));
      return;
    }
    if (d.kind === "marquee") {
      d.to = s;
      showMarquee(d);
      return;
    }
    if (!d.moved && Math.hypot(s.x - d.from.x, s.y - d.from.y) < CLICK_PX) return;
    d.moved = true;
    const now = screenRay(cam, sz, s);
    // Grabbed by a pixel with a house model: the pixel sticks to the house's surface (Alt: don't).
    // Finding the surface can take a while on a big model, so it's done once a frame, for the latest ray.
    if (d.grab && !e.altKey && latest.current.show.houseModel && typeof requestAnimationFrame === "function") {
      if (surfaceDrag.current) surfaceDrag.current.ray = now;
      else surfaceDrag.current = { ray: now, request: requestAnimationFrame(flushSurfaceDrag) };
      return;
    }
    dropSurfaceDrag();
    followRay(d, now, false);
  };

  const finish = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    // A move still waiting to snap to the house lands where the pointer was let go.
    flushSurfaceDrag();
    const d = drag.current;
    drag.current = null;
    try {
      canvasRef.current?.releasePointerCapture?.(e.pointerId);
    } catch {
      // Already released.
    }
    setCursor("default");
    if (!d) return;
    const st = useLayoutEditor.getState();
    const cam = view();
    if (d.kind === "orbit") {
      if (!d.moved && d.clear) st.clear();
      return;
    }
    if (d.kind === "marquee") {
      showMarquee(null);
      if (cam) {
        const sz = size();
        const inside = propsInRect(effective(), viewProjection(cam, sz), sz, d.from, d.to);
        st.select([...d.additive, ...inside]);
      }
      return;
    }
    if (d.kind !== "move") return;
    if (!d.moved) {
      if (d.deselect) st.toggle(d.deselect);
      else if (d.narrowTo) st.select([d.narrowTo]);
      return;
    }
    // The move stays drawn while it's on its way (see layoutGestures).
    void commitGesture(d.ids, moveGesture3(d.delta));
    syncPositions();
    invalidate();
  };

  const onDoubleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const cam = camera.current;
    if (!cam) return;
    const s = point(e);
    const sz = size();
    const props = effective();
    const hit = pickPixel(props, viewProjection(cam.current, sz), sz, s, HIT_PX);
    const box = hit ? boundsOfXyz(props.filter((p) => p.prop === hit.prop).map((p) => p.xyz)) : null;
    if (box) moveCamera(focusOrbit(cam.goal, box, sz));
    else moveCamera(fitOrbit(contentBox(props), sz, cam.goal.yaw, cam.goal.pitch));
  };

  return (
    <div className="relative h-full w-full overflow-hidden rounded-lg bg-[#04060f]">
      <canvas
        ref={canvasRef}
        role="application"
        aria-label={editable ? "3D layout" : "3D preview"}
        aria-roledescription="3D view"
        aria-describedby={helpId}
        tabIndex={0}
        className="block h-full w-full touch-none outline-none"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={finish}
        onPointerCancel={() => cancel()}
        onDoubleClick={onDoubleClick}
        onContextMenu={(e) => e.preventDefault()}
      />
      <p id={helpId} className="sr-only">
        {editable
          ? "Drag to orbit around the display, right-drag to pan, scroll to zoom. Keys 1 to 5 pick the front, top, left, right, and street views; F fits everything in. Click a prop to select it, then drag the gizmo's arrows to move it, or set its position, depth, and rotation in the properties panel. V switches to the 2D view."
          : "Drag to orbit around the display, right-drag to pan, scroll to zoom."}
      </p>
      {editable && <SelectionAnnouncer show={show} />}
      <div ref={marqueeRef} aria-hidden className="pointer-events-none absolute hidden border border-accent-400 bg-accent-400/10" />
      <View3dControls />
      {modelProblem && <p className="absolute bottom-2 left-2 max-w-md rounded-md bg-black/60 px-3 py-2 text-xs text-amber-300">{modelProblem}</p>}
      {problem && (
        <div className="absolute inset-0 flex items-center justify-center p-6 text-center text-sm text-neutral-300">
          <p className="max-w-sm rounded-lg bg-black/60 px-4 py-3">{problem}</p>
        </div>
      )}
    </div>
  );
}
