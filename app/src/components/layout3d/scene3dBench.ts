// Dev-only timing of the 3D view's renderer on a synthetic show (not part of the app): orbiting
// around 200,000 pixels with new live colors every frame. Start `pnpm dev`, open the app, and in
// the browser console run
//
//   (await import("/src/components/layout3d/scene3dBench.ts")).runScene3dBench()
//
// It reports frames per second as the browser paints them (capped by the display), the time to
// draw one frame when the GPU is made to finish it (the real cost, uncapped) with new colors
// every frame and with the camera moving only, and the time to fill and send the colors.

import type { PreviewProp3d } from "../../api/types";
import { type Orbit, boundsOfXyz, fillColors, fitOrbit, packPositions, pickPixel, typicalSpacing, viewProjection } from "../../lib/layout3d";
import { createThreeScene } from "./threeScene";

/** Mega trees of 16 strings × 100 pixels in rows across a yard, `pixels` in all. */
export function syntheticShow3d(pixels = 200_000): { props: PreviewProp3d[]; frameLength: number } {
  const perTree = 1600;
  const props: PreviewProp3d[] = [];
  for (let t = 0; t * perTree < pixels; t++) {
    const xyz = new Float32Array(perTree * 3);
    const [cx, cz] = [(t % 25) * 4 - 50, -Math.floor(t / 25) * 4];
    for (let i = 0; i < perTree; i++) {
      const [string, along] = [Math.floor(i / 100), (i % 100) / 99];
      const angle = (string / 16) * Math.PI * 2;
      const r = 1.5 * (1 - along) + 0.1;
      xyz.set([cx + r * Math.sin(angle), along * 5, cz + r * Math.cos(angle)], i * 3);
    }
    props.push({ prop: `tree-${t}`, frameOffset: t * perTree * 3, channelsPerPixel: 3, xyz });
  }
  return { props, frameLength: props.length * perTree * 3 };
}

/** How long picking the pixel under a click takes (the median of a few). */
function pickTime(props: PreviewProp3d[], orbit: Orbit, size: { width: number; height: number }): number {
  const m = viewProjection(orbit, size);
  const times: number[] = [];
  for (let i = 0; i < 9; i++) {
    const t0 = performance.now();
    pickPixel(props, m, size, { x: size.width / 2 + i * 10, y: size.height / 2 }, 8);
    times.push(performance.now() - t0);
  }
  return times.sort((a, b) => a - b)[4];
}

function rainbow(frame: Uint8Array, shift: number) {
  for (let i = 0; i + 2 < frame.length; i += 3) {
    const hue = ((i / 3) * 0.7 + shift) % 360;
    for (let c = 0; c < 3; c++) frame[i + c] = Math.round(127 + 127 * Math.cos(((hue - c * 120) * Math.PI) / 180));
  }
}

/** `keep` leaves the last frame on screen (for a screenshot) instead of removing the canvas. */
export async function runScene3dBench({ pixels = 200_000, frames = 300, width = 1280, height = 720, bloom = true, keep = false } = {}) {
  const canvas = document.createElement("canvas");
  Object.assign(canvas.style, { position: "fixed", left: "0", top: "0", width: `${width}px`, height: `${height}px`, zIndex: "9999" });
  document.body.append(canvas);
  const scene = createThreeScene(canvas);
  scene.resize({ width, height }, window.devicePixelRatio || 1);
  scene.setOptions({ bloom, ground: true });
  const { props, frameLength } = syntheticShow3d(pixels);
  const packed = packPositions(props);
  scene.setPixels(packed.xyz);
  scene.setBulbSize((typicalSpacing(props) ?? 0.1) * 0.42);
  const colors = new Uint8Array(packed.xyz.length);
  const start = fitOrbit(boundsOfXyz(props.map((p) => p.xyz)), { width, height });
  const orbitAt = (i: number): Orbit => ({ ...start, yaw: start.yaw + i * 0.01 });
  const none = new Set<string>();

  // Live frames, made ahead so making them isn't timed.
  const liveFrames = [0, 1, 2].map((k) => {
    const f = new Uint8Array(frameLength);
    rainbow(f, k * 40);
    return f;
  });
  let fillMs = 0;
  /** One frame: new colors (from the next live frame) when `recolor`, then the camera a step round. */
  const drawFrame = (i: number, recolor = true) => {
    if (recolor) {
      const t0 = performance.now();
      fillColors(props, liveFrames[i % 3], none, { unlit: [96, 96, 104], selected: [167, 139, 250] }, colors);
      scene.setColors(colors, true);
      fillMs += performance.now() - t0;
    }
    scene.render(orbitAt(i));
  };

  // Warm up (shader compiles, buffer uploads).
  for (let i = 0; i < 10; i++) drawFrame(i);

  // As painted: one frame per animation frame.
  fillMs = 0;
  const painted = await new Promise<number>((resolve) => {
    let i = 0;
    let first = 0;
    const step = (now: number) => {
      if (i === 0) first = now;
      drawFrame(i);
      if (++i < frames) requestAnimationFrame(step);
      else resolve((frames - 1) / ((now - first) / 1000));
    };
    requestAnimationFrame(step);
  });
  const fillPerFrame = fillMs / frames;

  // Uncapped: each frame drawn and finished by the GPU before the next.
  const gl = (canvas.getContext("webgl2") ?? canvas.getContext("webgl")) as WebGLRenderingContext | null;
  const syncFrames = Math.min(60, frames);
  const finished = (recolor: boolean) => {
    const t0 = performance.now();
    for (let i = 0; i < syncFrames; i++) {
      drawFrame(i, recolor);
      // Reading a pixel waits for the GPU to finish the frame.
      gl?.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(4));
    }
    return (performance.now() - t0) / syncFrames;
  };
  const msPerFrame = finished(true);
  const msPerOrbitFrame = finished(false);

  const renderer = gl?.getExtension("WEBGL_debug_renderer_info");
  const result = {
    pixels: packed.xyz.length / 3,
    size: `${width}×${height} @${window.devicePixelRatio || 1}x`,
    bloom,
    gpu: renderer && gl ? String(gl.getParameter(renderer.UNMASKED_RENDERER_WEBGL)) : "unknown",
    paintedFps: Math.round(painted * 10) / 10,
    msPerFrameFinished: Math.round(msPerFrame * 100) / 100,
    uncappedFps: Math.round((1000 / msPerFrame) * 10) / 10,
    msPerFrameOrbitOnly: Math.round(msPerOrbitFrame * 100) / 100,
    colorFillAndUploadMs: Math.round(fillPerFrame * 100) / 100,
    pickMs: Math.round(pickTime(props, start, { width, height }) * 100) / 100,
  };
  if (keep) return result;
  scene.dispose();
  canvas.remove();
  return result;
}
