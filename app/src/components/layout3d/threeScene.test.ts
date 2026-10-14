import {
  AdditiveBlending,
  BufferAttribute,
  CustomBlending,
  DepthTexture,
  EqualDepth,
  type InterleavedBufferAttribute,
  LessDepth,
  Mesh,
  NotEqualStencilFunc,
  type Object3D,
  OneFactor,
  OneMinusSrcAlphaFactor,
  Points,
  ReplaceStencilOp,
  Scene,
  type ShaderMaterial,
  type Vector2,
  type Vector4,
  Vector3,
  WebGLRenderTarget,
  type WebGLRenderer,
} from "three";
import { FullScreenQuad } from "three/addons/postprocessing/Pass.js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type LightLook, SPREAD_SIZE, modelPoint, pixelsPerUnit, v3 } from "../../lib/layout3d";
import { FAINT } from "../../lib/pixelBatches";
import { createThreeScene, modelEuler, plainModelError } from "./threeScene";

/** What a fake renderer was asked to draw: into which picture (null: the screen), and whether what was there was cleared first. */
interface Drawn {
  what: Object3D;
  into: WebGLRenderTarget | null;
  cleared: boolean;
  /** Whether the depth buffer was emptied since the draw before. */
  freshDepth: boolean;
}

/** Enough of a WebGLRenderer for the scene to be built, drawn, and taken down without WebGL. */
function fakeRenderer() {
  const calls: string[] = [];
  const drawn: Drawn[] = [];
  let target: WebGLRenderTarget | null = null;
  let freshDepth = false;
  const renderer = {
    calls,
    drawn,
    outputColorSpace: "",
    autoClear: true,
    capabilities: { getMaxAnisotropy: () => 1 },
    renderLists: { dispose: () => calls.push("renderLists.dispose") },
    setClearColor: () => {},
    setPixelRatio: () => {},
    getPixelRatio: () => 1,
    setSize: () => {},
    getSize: (v: { set(x: number, y: number): unknown }) => v.set(100, 100),
    setRenderTarget: (next: WebGLRenderTarget | null) => void (target = next),
    clearDepth: () => void (freshDepth = true),
    render: (what: Object3D) => {
      drawn.push({ what, into: target, cleared: renderer.autoClear, freshDepth });
      freshDepth = false;
    },
    dispose: () => calls.push("dispose"),
    forceContextLoss: () => calls.push("forceContextLoss"),
  };
  return renderer;
}

const ORBIT = { target: v3(0, 0, 0), yaw: 0, pitch: 0.2, distance: 10 };
const LOOK: LightLook = { radius: [1.2, 4], unlit: [200, 100, 50, 0.5], dark: [70, 70, 70, 0.4] };
/** A spread for `count` pixels (see `packSpread`). */
const spreadFor = (count: number) => new Float32Array(count * SPREAD_SIZE).fill(0.1);
const pointsIn = (root: Object3D) => {
  const found: Points[] = [];
  root.traverse((o) => void (o instanceof Points && found.push(o)));
  return found.sort((a, b) => a.renderOrder - b.renderOrder);
};
/** A photo to stand behind the lights. */
const photo = (opacity: number) => ({ image: { width: 4, height: 3 } as unknown as TexImageSource, box: { min: v3(-5, 0, -1), max: v3(5, 6, -1) }, opacity });

/** A scene on a fake renderer, with what it drew. */
function made() {
  const renderer = fakeRenderer();
  const scene = createThreeScene({} as HTMLCanvasElement, () => renderer as unknown as WebGLRenderer);
  scene.setLook(LOOK);
  /** What was drawn to the screen: the scene, the lights over it, and anything after. */
  const onScreen = () => renderer.drawn.filter((d) => d.into === null);
  /** The lights' two passes, in the order they're drawn. */
  const passes = () => pointsIn(onScreen()[1].what).map((points) => ({ points, material: points.material as ShaderMaterial }));
  return { renderer, scene, onScreen, passes };
}

afterEach(() => vi.restoreAllMocks());

describe("the three.js scene", () => {
  it("turns the house model in the same order as props (X, then Y, then Z, about the layout's axes)", () => {
    const rotationDeg = v3(-90, 30, 0);
    // A model made Z-up, stood up with a tilt of -90 and turned 30: its up stays up.
    const up = new Vector3(0, 0, 1).applyEuler(modelEuler(rotationDeg));
    expect(up.x).toBeCloseTo(0);
    expect(up.y).toBeCloseTo(1);
    expect(up.z).toBeCloseTo(0);
    // Any turn agrees with how props' pixels are placed.
    for (const r of [v3(-90, 30, 0), v3(20, -45, 70), v3(180, 90, -30)]) {
      const p = new Vector3(1.5, -2, 0.75).applyEuler(modelEuler(r));
      const q = modelPoint(v3(1.5, -2, 0.75), { position: v3(0, 0, 0), rotationDeg: r, scale: 1 });
      expect([p.x, p.y, p.z].map((n) => n.toFixed(6))).toEqual([q.x, q.y, q.z].map((n) => n.toFixed(6)));
    }
  });

  it("frees everything it made when it's taken down, its two pictures and the WebGL context too", () => {
    const targets = vi.spyOn(WebGLRenderTarget.prototype, "dispose");
    const quads = vi.spyOn(FullScreenQuad.prototype, "dispose");
    const { renderer, scene } = made();
    scene.setPixels(new Float32Array([0, 0, 0, 1, 1, 1]), spreadFor(2));
    scene.dispose();
    // The halos' picture, and the depth picture of what hides lights.
    expect(targets).toHaveBeenCalledTimes(2);
    expect(quads).toHaveBeenCalledOnce();
    expect(renderer.calls).toEqual(["renderLists.dispose", "dispose", "forceContextLoss"]);
  });

  it("frees every geometry and material in its scenes, and the material the halos are laid over with", () => {
    const { renderer, scene } = made();
    scene.setOptions({ glow: 0.5, ground: true });
    scene.setBackdrop(photo(1));
    scene.setPixels(new Float32Array([0, 0, 0]), spreadFor(1));
    scene.setColors(new Uint8Array([255, 0, 0]), true);
    scene.render(ORBIT);
    // What hides lights, the halos, the scene, the lights, and the halos laid over.
    expect(renderer.drawn).toHaveLength(5);
    const freed = new Set<unknown>();
    const things = new Set<unknown>();
    for (const { what } of renderer.drawn) {
      what.traverse((o) => {
        const mesh = o as Object3D & { geometry?: { addEventListener: Function }; material?: { addEventListener: Function } };
        for (const thing of [mesh.geometry, mesh.material]) {
          // The quad the halos are laid over with keeps its geometry for every scene.
          if (!thing || (what instanceof Mesh && thing === mesh.geometry)) continue;
          things.add(thing);
          thing.addEventListener("dispose", () => freed.add(thing));
        }
      });
    }
    expect(things.size).toBeGreaterThan(8);
    scene.dispose();
    for (const thing of things) expect(freed.has(thing)).toBe(true);
  });

  it("uploads every position after a full update, even when a partial one follows before the next frame", () => {
    const { scene, passes } = made();
    scene.setOptions({ glow: 0, ground: true });
    scene.setPixels(new Float32Array(9), spreadFor(3));
    scene.render(ORBIT);
    const position = passes()[0].points.geometry.getAttribute("position") as BufferAttribute;
    position.clearUpdateRanges();
    // New positions for every pixel (an undo, say), then a drag moves the last pixel in the same frame.
    scene.setPixels(new Float32Array([1, 1, 1, 2, 2, 2, 3, 3, 3]), spreadFor(3));
    scene.updatePixels(2, new Float32Array([4, 4, 4]));
    scene.render(ORBIT);
    // No ranges: three uploads the whole buffer.
    expect(position.updateRanges).toEqual([]);
    expect(Array.from(position.array)).toEqual([1, 1, 1, 2, 2, 2, 4, 4, 4]);
    // Afterwards, partial updates upload only what changed again.
    scene.updatePixels(0, new Float32Array([5, 5, 5]));
    expect(position.updateRanges).toEqual([{ start: 0, count: 3 }]);
  });

  it("draws the lights over the finished scene with the depth buffer to themselves: every dot into it alone, then each place's kept dot in color", () => {
    const { scene, onScreen, passes } = made();
    scene.setPixels(new Float32Array(6), spreadFor(2));
    scene.render(ORBIT);
    const [main, lights] = onScreen();
    expect(onScreen()).toHaveLength(2);
    // The scene clears what was on screen; the lights are drawn over it, once the scene's depths are gone.
    expect([main.what, main.cleared]).toEqual([expect.any(Scene), true]);
    expect(pointsIn(main.what)).toHaveLength(0);
    expect([lights.cleared, lights.freshDepth]).toEqual([false, true]);
    const [settling, color] = passes();
    expect(passes()).toHaveLength(2);
    // The same points, with the same numbers, both times.
    expect(color.points.geometry).toBe(settling.points.geometry);
    expect(color.material.uniforms).toBe(settling.material.uniforms);
    expect([color.material.vertexShader, color.material.fragmentShader]).toEqual([settling.material.vertexShader, settling.material.fragmentShader]);
    // First which dot each place keeps: the first in the order the dots' depths put them in.
    expect(settling.material.colorWrite).toBe(false);
    expect(settling.material.depthWrite).toBe(true);
    expect(settling.material.depthFunc).toBe(LessDepth);
    // Then that dot alone, leaving the depth buffer be.
    expect(color.material.colorWrite).toBe(true);
    expect(color.material.depthFunc).toBe(EqualDepth);
    expect(color.material.depthWrite).toBe(false);
  });

  it("puts a light's color over what's behind it, never adding to it, and only one light at a place", () => {
    const { scene, passes } = made();
    scene.setPixels(new Float32Array(6), spreadFor(2));
    scene.render(ORBIT);
    const [settling, color] = passes().map((p) => p.material);
    expect(settling.blending).not.toBe(AdditiveBlending);
    // The color, already multiplied by how solid it is, over as much of what's there as it lets through.
    expect([color.blending, color.blendSrc, color.blendDst]).toEqual([CustomBlending, OneFactor, OneMinusSrcAlphaFactor]);
    // Where two dots tie for a place, the first marks it and the other isn't drawn.
    expect(color.stencilWrite).toBe(true);
    expect([color.stencilFunc, color.stencilRef, color.stencilZPass]).toEqual([NotEqualStencilFunc, 1, ReplaceStencilOp]);
  });

  it("hands every light its prop's spread, and its own color beside the colors of the lights wired before and after it", () => {
    const { scene, passes } = made();
    const spread = new Float32Array([0.5, 0.25, 0, 1, 0.5, 0.25, 0, 0, 0.5, 0.25, 0, 2]);
    scene.setPixels(new Float32Array(9), spread);
    scene.setColors(new Uint8Array([10, 11, 12, 20, 21, 22, 30, 31, 32]), true);
    scene.render(ORBIT);
    const { geometry } = passes()[0].points;
    const spreads = geometry.getAttribute("spread") as BufferAttribute;
    expect(spreads.itemSize).toBe(SPREAD_SIZE);
    expect(Array.from(spreads.array)).toEqual(Array.from(spread));
    const [before, own, after] = ["tintBefore", "tint", "tintAfter"].map((name) => geometry.getAttribute(name) as InterleavedBufferAttribute);
    // One list of colors, read a light apart.
    expect(before.data).toBe(own.data);
    expect(after.data).toBe(own.data);
    expect([before.offset, own.offset, after.offset]).toEqual([0, 3, 6]);
    expect(own.normalized).toBe(true);
    expect([own.getX(1) * 255, own.getY(1) * 255, own.getZ(1) * 255].map(Math.round)).toEqual([20, 21, 22]);
    expect([before.getX(1) * 255, after.getX(1) * 255].map(Math.round)).toEqual([10, 30]);
    // Nothing is wired before the first light or after the last: black.
    expect([before.getX(0), after.getX(2)]).toEqual([0, 0]);
    // Every light is drawn: three, though the list of colors is longer.
    expect((geometry.getAttribute("position") as BufferAttribute).count).toBe(3);
    // The same number of pixels again: its spread and colors are kept up.
    scene.setPixels(new Float32Array(9), spread.map((v) => v * 2));
    expect(Array.from(spreads.array)).toEqual(Array.from(spread.map((v) => v * 2)));
  });

  it("draws quiet lights in the look's unlit color until something plays, then in its dark color, with faint lights fading into it", () => {
    const { scene, passes } = made();
    scene.setPixels(new Float32Array(6), spreadFor(2));
    scene.setColors(new Uint8Array(6), false);
    scene.render(ORBIT);
    const { uniforms } = passes()[1].material;
    const quiet = () => (uniforms.quiet.value as Vector4).toArray().map((v) => Math.round(v * 1000) / 1000);
    // As light: each color times how solid it is.
    expect(quiet()).toEqual([0.392, 0.196, 0.098, 0.5]);
    expect(uniforms.faint.value).toBe(0);
    expect((uniforms.radius.value as Vector2).toArray()).toEqual([1.2, 4]);
    scene.setColors(new Uint8Array([255, 0, 0, 0, 0, 0]), true);
    expect(quiet()).toEqual([0.11, 0.11, 0.11, 0.4]);
    expect(uniforms.faint.value).toBeCloseTo(FAINT / 255);
    // Another look: its sizes and colors.
    scene.setLook({ radius: [1.3, 4.5], unlit: [0, 0, 0, 1], dark: [255, 255, 255, 1] });
    expect(quiet()).toEqual([1, 1, 1, 1]);
    expect((uniforms.radius.value as Vector2).toArray()).toEqual([1.3, 4.5]);
  });

  it("tells the lights how big a layout unit is on screen, and whether the camera is still", () => {
    const { scene, passes } = made();
    scene.resize({ width: 800, height: 600 }, 2);
    scene.setPixels(new Float32Array(6), spreadFor(2));
    scene.render(ORBIT, true);
    const { uniforms } = passes()[0].material;
    expect(uniforms.scale.value).toBeCloseTo(pixelsPerUnit(600, 2));
    expect(uniforms.ratio.value).toBe(2);
    expect((uniforms.viewport.value as Vector2).toArray()).toEqual([1600, 1200]);
    // On its way somewhere: dots glide. Still: small ones sit on whole device pixels.
    expect(uniforms.still.value).toBe(0);
    scene.render(ORBIT);
    expect(uniforms.still.value).toBe(1);
  });

  it("has each light look in a depth picture of the photo and the house model for whether it's hidden, when there's anything to hide it", () => {
    const { renderer, scene, passes } = made();
    scene.resize({ width: 800, height: 600 }, 2);
    scene.setPixels(new Float32Array(6), spreadFor(2));
    scene.render(ORBIT);
    const { uniforms } = passes()[0].material;
    const hiding = () => (uniforms.hiding.value as Vector3).toArray();
    // Nothing that could hide a light: nothing is drawn for it, and no light looks.
    expect(renderer.drawn.every((d) => d.into === null)).toBe(true);
    expect(hiding()[0]).toBe(0);

    // A photo that hides what's behind it: drawn first, depths only, into a picture a screen pixel to each of its own.
    renderer.drawn.length = 0;
    scene.setBackdrop(photo(0.9));
    scene.render(ORBIT);
    const [first, main] = renderer.drawn;
    expect(renderer.drawn).toHaveLength(3);
    expect(first.into).toBeInstanceOf(WebGLRenderTarget);
    expect([first.into!.width, first.into!.height]).toEqual([800, 600]);
    expect(first.into!.depthTexture).toBeInstanceOf(DepthTexture);
    expect(uniforms.hides.value).toBe(first.into!.depthTexture);
    expect(hiding()).toEqual([1, 1 / 800, 1 / 600]);
    expect(main.into).toBeNull();
    const shapes: Mesh[] = [];
    first.what.traverse((o) => void (o instanceof Mesh && shapes.push(o)));
    expect(shapes).toHaveLength(1);
    expect((shapes[0].material as ShaderMaterial).colorWrite).toBe(false);
    // The lights themselves aren't in it.
    expect(pointsIn(first.what)).toHaveLength(0);

    // A photo faint enough to see through hides nothing.
    renderer.drawn.length = 0;
    scene.setBackdrop(photo(0.4));
    scene.render(ORBIT);
    expect(renderer.drawn.every((d) => d.into === null)).toBe(true);
    expect(hiding()[0]).toBe(0);
  });

  it("washes DMX floods' cells over the finished scene and under the dots, as the flat previews do, and only when there are floods", () => {
    const { renderer, scene } = made();
    scene.setPixels(new Float32Array(9), spreadFor(3));
    scene.setFloods(new Uint8Array([0, 1, 0]));
    scene.setColors(new Uint8Array([255, 0, 0, 255, 128, 0, 0, 0, 255]), true);
    scene.render(ORBIT);
    const onScreen = renderer.drawn.filter((d) => d.into === null);
    // The scene, the floods' washes, then the lights.
    expect(onScreen).toHaveLength(3);
    const [washes] = pointsIn(onScreen[1].what);
    const material = washes.material as ShaderMaterial;
    expect(material.depthWrite).toBe(false);
    expect(material.blending).not.toBe(AdditiveBlending);
    expect(material.uniforms.lit.value).toBe(1);
    expect(Array.from(washes.geometry.getAttribute("flood").array)).toEqual([0, 1, 0]);
    expect(material.fragmentShader).toContain("t * t");

    // Another set of pixels forgets the floods until they are said again.
    renderer.drawn.length = 0;
    scene.setPixels(new Float32Array(12), spreadFor(4));
    scene.render(ORBIT);
    expect(renderer.drawn.filter((d) => d.into === null)).toHaveLength(2);
  });

  it("with no glow draws the scene and the lights straight to the screen: nothing is added", () => {
    const { renderer, scene } = made();
    scene.setPixels(new Float32Array(6), spreadFor(2));
    scene.setOptions({ glow: 0, ground: true });
    scene.setColors(new Uint8Array([255, 0, 0, 0, 255, 0]), true);
    scene.render(ORBIT);
    expect(renderer.drawn).toHaveLength(2);
    let adding = 0;
    for (const { what, into } of renderer.drawn) {
      expect(into).toBeNull();
      what.traverse((o) => {
        if (((o as Mesh).material as ShaderMaterial | undefined)?.blending === AdditiveBlending) adding++;
      });
    }
    expect(adding).toBe(0);
  });

  it("glows as much as the level says: the lit lights' halos drawn into a picture of their own, and added over the finished lights", () => {
    const { renderer, scene, passes } = made();
    scene.resize({ width: 800, height: 600 }, 2);
    scene.setPixels(new Float32Array(6), spreadFor(2));
    const colors = new Uint8Array([255, 0, 0, 0, 255, 0]);
    scene.setOptions({ glow: 0.5, ground: true });
    scene.setColors(colors, true);
    scene.render(ORBIT);
    const [halos, main, lights, over] = renderer.drawn;
    expect(renderer.drawn).toHaveLength(4);
    // The halos: the same points, alone, adding up in a picture a screen pixel to each of its own.
    expect(halos.into).toBeInstanceOf(WebGLRenderTarget);
    expect([halos.into!.width, halos.into!.height]).toEqual([800, 600]);
    const haloPoints = pointsIn(halos.what);
    expect(haloPoints).toHaveLength(1);
    const halo = haloPoints[0].material as ShaderMaterial;
    expect(halo.blending).toBe(AdditiveBlending);
    expect(haloPoints[0].geometry).toBe(passes()[0].points.geometry);
    expect(halo.uniforms.cell.value).toBe(2);
    // A hidden light has no halo: the halos look in the same depth picture as the dots.
    expect(halo.uniforms.hides).toBe(passes()[0].material.uniforms.hides);
    expect(halo.uniforms.hiding).toBe(passes()[0].material.uniforms.hiding);
    // Then the scene and the lights, to the screen, the dots as crisp as without glow.
    expect([main.into, main.cleared, lights.into, lights.cleared]).toEqual([null, true, null, false]);
    // Then the halos' picture over them, adding, without clearing what's there.
    expect(over.what).toBeInstanceOf(Mesh);
    expect([over.into, over.cleared]).toEqual([null, false]);
    const laid = (over.what as Mesh).material as ShaderMaterial;
    expect(laid.blending).toBe(AdditiveBlending);
    expect(laid.uniforms.halos.value).toBe(halos.into!.texture);
    // The level reaches the halos and the dots (which dim by what their halo adds back).
    const level = () => [halo.uniforms.glow.value, passes()[1].material.uniforms.glow.value];
    expect(level()).toEqual([0.5, 0.5]);
    scene.setOptions({ glow: 1, ground: true });
    expect(level()).toEqual([1, 1]);
    // It's drawn at the step the flat previews draw it at; a level out of range is kept in range.
    scene.setOptions({ glow: 0.52, ground: true });
    expect(level()).toEqual([0.5, 0.5]);
    scene.setOptions({ glow: 9, ground: true });
    expect(level()).toEqual([1, 1]);
    scene.setOptions({ glow: NaN, ground: true });
    expect(level()).toEqual([0, 0]);

    // Lights that aren't lit (nothing is playing) don't glow, whatever the level.
    scene.setOptions({ glow: 1, ground: true });
    scene.setColors(colors, false);
    renderer.drawn.length = 0;
    scene.render(ORBIT);
    expect(renderer.drawn).toHaveLength(2);
    expect(level()).toEqual([0, 0]);
    // Lit again, they glow at the level set meanwhile.
    scene.setOptions({ glow: 0.25, ground: true });
    scene.setColors(colors, true);
    expect(level()).toEqual([0.25, 0.25]);
  });

  it("explains a model it can't read in plain words", () => {
    expect(plainModelError(new Error('THREE.GLTFLoader: Failed to load buffer "house.bin".'))).toMatch(/files next to it.*single GLB/);
    expect(plainModelError(new SyntaxError("Unexpected token < in JSON"))).toMatch(/couldn't read this model/);
  });
});
