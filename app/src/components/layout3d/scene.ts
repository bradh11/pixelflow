// What the 3D view asks of its renderer. The view works out everything (camera, picking,
// gizmo, colors) in plain TypeScript (lib/layout3d.ts) and hands the results to a `Scene3d`
// to draw, so the view's behavior is tested without WebGL. The real one is three.js
// (threeScene.ts), loaded only when a 3D view first opens.

import type { Backend } from "../../api/backend";
import type { Size } from "../../lib/layoutMath";
import type { Box3, FloorGrid, GizmoHandle, LightLook, Orbit, Ray, V3 } from "../../lib/layout3d";
import type { PreviewProp3d } from "../../api/types";
import { ARM_LENGTH, GRAZING, beamLight, beamWidth, groundReach, headDirection, servoDirection, spread } from "../../lib/beams";

type Rgb = readonly [number, number, number];

/**
 * What the 3D view draws for moving heads and servos, in layout units: a lit head's beam (from
 * `origin`, pointing `dir`, `length` long, `angleDeg` wide, in its hue `color` as bright as
 * `level`; `lands`: it stops at the ground, at full), the pool of light where a beam meets the
 * ground (centred `at`, `radius` across and `stretch` along `heading`, the beam's way across the
 * ground), the dashed outline of where a selected head points while nothing plays (`aim`, never
 * light), or a servo's arm (from `origin`, pointing `dir`, `length` long).
 */
export type Motion3d =
  | { kind: "beam"; origin: V3; dir: V3; length: number; angleDeg: number; color: Rgb; level: number; lands: boolean }
  | { kind: "pool"; at: V3; heading: { x: number; z: number }; radius: number; stretch: number; color: Rgb; level: number }
  | { kind: "aim"; origin: V3; dir: V3; length: number; angleDeg: number; color: Rgb }
  | { kind: "arm"; origin: V3; dir: V3; length: number };

/**
 * The props' beams, pools and arms as they point with `frame`, from their cells in 3D. While
 * nothing plays (`frame` null) no light shows: a head in `aiming` (selected on the Layout screen)
 * shows its aim instead, in `aimColor`.
 */
export function motionOf(props: PreviewProp3d[], frame: Uint8Array | null, aiming: ReadonlySet<string> = new Set(), aimColor: Rgb = [167, 139, 250]): Motion3d[] {
  const out: Motion3d[] = [];
  for (const p of props) {
    const motion = p.motion;
    const cells = Math.floor(p.xyz.length / 3);
    if (!motion || cells === 0) continue;
    const at = (k: number) => ({ x: p.xyz[3 * k], y: p.xyz[3 * k + 1], z: p.xyz[3 * k + 2] });
    const beam = motion.beam;
    const o = p.frameOffset;
    const light = frame && o + 2 < frame.length ? beamLight(frame[o], frame[o + 1], frame[o + 2]) : null;
    if (beam && (light || (!frame && aiming.has(p.prop)))) {
      const d = headDirection(motion, frame);
      const dir = { x: d[0], y: d[1], z: d[2] };
      const origin = at(0);
      const angleDeg = beamWidth(motion, frame);
      const landed = groundReach(origin.y, d, beam.length);
      const length = landed ?? beam.length;
      if (!light) {
        out.push({ kind: "aim", origin, dir, length, angleDeg, color: aimColor });
      } else {
        const color: Rgb = [light.rgb & 255, (light.rgb >> 8) & 255, (light.rgb >> 16) & 255];
        out.push({ kind: "beam", origin, dir, length, angleDeg, color, level: light.level, lands: landed !== null });
        if (landed !== null) {
          const radius = Math.max(length * spread(angleDeg), 0.02);
          const h = Math.sqrt(d[0] * d[0] + d[2] * d[2]);
          const heading = h > 0 ? { x: d[0] / h, z: d[2] / h } : { x: 1, z: 0 };
          const ground = { x: origin.x + d[0] * length, y: 0, z: origin.z + d[2] * length };
          out.push({ kind: "pool", at: ground, heading, radius, stretch: radius / Math.max(-d[1], GRAZING), color, level: light.level });
        }
      }
    }
    (motion.servos ?? []).forEach((_, k) => {
      const [x, y, z] = servoDirection(motion, k, frame);
      out.push({ kind: "arm", origin: at(Math.min(k, cells - 1)), dir: { x, y, z }, length: ARM_LENGTH });
    });
  }
  return out;
}

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
  /**
   * Every pixel's position (x, y, z triples), all drawn at once, and how each one's prop is
   * spread out, which sizes its dot (see `packSpread`).
   */
  setPixels(xyz: Float32Array, spread: Float32Array): void;
  /** Which pixels (1 each, in `setPixels`' order) are DMX floods' cells, drawn with a wash of their color round them (after `setPixels`). */
  setFloods(floods: Uint8Array): void;
  /** Moving heads' beams and servos' arms, as they point now (see lib/beams.ts); none to clear them. */
  setMotion(motion: Motion3d[]): void;
  /** Changes the positions of `xyz.length / 3` pixels from pixel `start` on (after `setPixels`). */
  updatePixels(start: number, xyz: Float32Array): void;
  /**
   * Every pixel's own color (RGB bytes): black for one with none, which is drawn in the look's
   * quiet color, under the others. `lit` is true for live colors, which glow as much as the
   * options say.
   */
  setColors(rgb: Uint8Array, lit: boolean): void;
  /** How big lights are drawn, and the color of the ones with none of their own. */
  setLook(look: LightLook): void;
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
  /**
   * How much lit pixels glow (0 for none, to 1), the ground and its grid, and how big dots are
   * against the usual (the viewer's Dot size, `DOT_SCALES`).
   */
  setOptions(options: { glow: number; ground: boolean; grid: boolean; dotScale: number }): void;
  /** How far apart the grid's lines are, and where it fades out (see `floorGrid`). */
  setFloor(grid: FloorGrid): void;
  /**
   * Draws a frame from the camera. `moving`: the camera is on its way somewhere, and more frames
   * follow: small dots are drawn soft wherever they fall, so they glide. Once it's still they sit
   * on whole device pixels, crisp, as the flat previews draw them.
   */
  render(orbit: Orbit, moving?: boolean): void;
  dispose(): void;
}

/** Makes the renderer for a canvas. */
export type SceneFactory = (canvas: HTMLCanvasElement) => Promise<Scene3d>;

/** A model file just read to measure it, kept for the 3D view so it isn't read again. */
let handedOver: { path: string; bytes: Uint8Array } | null = null;

/** The box around the model in a file, as the file has it (loads three.js if needed). */
export async function measureModel(bytes: Uint8Array, path: string): Promise<Box3 | null> {
  const three = await import("./threeScene");
  const box = await three.measureModel(bytes, path);
  handedOver = { path, bytes };
  return box;
}

/** The house model file's bytes: the ones just measured if they're this file's, otherwise read now. */
export async function readModel(backend: Pick<Backend, "readHouseModel">, path: string): Promise<Uint8Array> {
  const kept = handedOver;
  handedOver = null;
  return kept?.path === path ? kept.bytes : backend.readHouseModel(path);
}

/** The three.js renderer, loaded the first time it's needed (it's large, and 2D-only users never need it). */
export const loadThreeScene: SceneFactory = async (canvas) => {
  const { createThreeScene } = await import("./threeScene");
  return createThreeScene(canvas);
};
