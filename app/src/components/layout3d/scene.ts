// What the 3D view asks of its renderer. The view works out everything (camera, picking,
// gizmo, colors) in plain TypeScript (lib/layout3d.ts) and hands the results to a `Scene3d`
// to draw, so the view's behavior is tested without WebGL. The real one is three.js
// (threeScene.ts), loaded only when a 3D view first opens.

import type { Size } from "../../lib/layoutMath";
import type { Box3, GizmoHandle, Orbit, Ray, V3 } from "../../lib/layout3d";

/** The house model's placement (layout units and degrees). */
export interface ModelPlacement {
  position: V3;
  rotationDeg: V3;
  scale: number;
  opacity: number;
}

export interface Scene3d {
  /** Sets the drawing size (CSS pixels) and device pixels per CSS pixel. */
  resize(size: Size, ratio: number): void;
  /** Every pixel's position (x, y, z triples), all drawn at once. */
  setPixels(xyz: Float32Array): void;
  /** Changes the positions of `xyz.length / 3` pixels from pixel `start` on (after `setPixels`). */
  updatePixels(start: number, xyz: Float32Array): void;
  /** Every pixel's color (RGB bytes); `lit` is true for live colors, which glow. */
  setColors(rgb: Uint8Array, lit: boolean): void;
  /** How big a bulb is, in layout units. */
  setBulbSize(size: number): void;
  /** The photo as an upright picture filling `box` (a flat box at its depth), or none. */
  setBackdrop(backdrop: { image: TexImageSource; box: Box3; opacity: number } | null): void;
  /** Loads a house model from a file's bytes, replacing any other; resolves with its box as the file has it. */
  setModel(model: { bytes: Uint8Array; name: string } | null): Promise<Box3 | null>;
  /** Places the loaded house model; resolves to its new box. */
  placeModel(placement: ModelPlacement): Box3 | null;
  /** Where a ray from the camera first meets the house model, if it does. */
  surfaceAt(ray: Ray): V3 | null;
  /** Outlines a box around the selection, or nothing. */
  setSelectionBox(box: Box3 | null): void;
  /** The move gizmo at `origin`, arrows `length` long, with a handle highlighted; or none. */
  setGizmo(gizmo: { origin: V3; length: number; highlight: GizmoHandle | null } | null): void;
  /** Glow around lit pixels, and the ground. */
  setOptions(options: { bloom: boolean; ground: boolean }): void;
  /** Draws a frame from the camera. */
  render(orbit: Orbit): void;
  dispose(): void;
}

/** Makes the renderer for a canvas. */
export type SceneFactory = (canvas: HTMLCanvasElement) => Promise<Scene3d>;

/** The box around the model in a file, as the file has it (loads three.js if needed). */
export async function measureModel(bytes: Uint8Array, name: string): Promise<Box3 | null> {
  const three = await import("./threeScene");
  return three.measureModel(bytes, name);
}

/** The three.js renderer, loaded the first time it's needed (it's large, and 2D-only users never need it). */
export const loadThreeScene: SceneFactory = async (canvas) => {
  const { createThreeScene } = await import("./threeScene");
  return createThreeScene(canvas);
};
