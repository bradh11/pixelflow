import { BufferAttribute, type Object3D, Points, type Scene, Vector3, WebGLRenderTarget, type WebGLRenderer } from "three";
import { OutputPass } from "three/addons/postprocessing/OutputPass.js";
import { Pass } from "three/addons/postprocessing/Pass.js";
import { ShaderPass } from "three/addons/postprocessing/ShaderPass.js";
import { UnrealBloomPass } from "three/addons/postprocessing/UnrealBloomPass.js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { modelPoint, v3 } from "../../lib/layout3d";
import { createThreeScene, modelEuler } from "./threeScene";

/** Enough of a WebGLRenderer for the scene to be built, drawn, and taken down without WebGL. */
function fakeRenderer() {
  const calls: string[] = [];
  const drawn: Scene[] = [];
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
    clearDepth: () => {},
    render: (scene: Scene) => drawn.push(scene),
    dispose: () => calls.push("dispose"),
    forceContextLoss: () => calls.push("forceContextLoss"),
  };
  return renderer;
}

const ORBIT = { target: v3(0, 0, 0), yaw: 0, pitch: 0.2, distance: 10 };

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

  it("frees everything it made when it's taken down, the bloom passes and the WebGL context too", () => {
    const renderer = fakeRenderer();
    const targets = vi.spyOn(WebGLRenderTarget.prototype, "dispose");
    const bloom = vi.spyOn(UnrealBloomPass.prototype, "dispose");
    const output = vi.spyOn(OutputPass.prototype, "dispose");
    const shaders = vi.spyOn(ShaderPass.prototype, "dispose");
    const passes = vi.spyOn(Pass.prototype, "dispose");
    const scene = createThreeScene({} as HTMLCanvasElement, () => renderer as unknown as WebGLRenderer);
    scene.setPixels(new Float32Array([0, 0, 0, 1, 1, 1]));
    scene.dispose();
    expect(bloom).toHaveBeenCalledOnce();
    // The bloom's 11 targets, and both composers' 2 each.
    expect(targets.mock.calls.length).toBeGreaterThanOrEqual(15);
    expect(output).toHaveBeenCalledOnce();
    // The mix pass, and each composer's copy pass.
    expect(shaders).toHaveBeenCalledTimes(3);
    // The two render passes.
    expect(passes).toHaveBeenCalledTimes(2);
    expect(renderer.calls).toEqual(["renderLists.dispose", "dispose", "forceContextLoss"]);
  });

  it("frees the mix pass's material and every geometry and material in its scenes", () => {
    const renderer = fakeRenderer();
    const scene = createThreeScene({} as HTMLCanvasElement, () => renderer as unknown as WebGLRenderer);
    scene.setOptions({ bloom: false, ground: true });
    scene.setPixels(new Float32Array([0, 0, 0]));
    scene.render(ORBIT);
    const freed = new Set<unknown>();
    const watch = (root: Object3D) =>
      root.traverse((o) => {
        const mesh = o as Object3D & { geometry?: { addEventListener: Function }; material?: { addEventListener: Function } };
        for (const thing of [mesh.geometry, mesh.material]) thing?.addEventListener("dispose", () => freed.add(thing));
      });
    const [main] = renderer.drawn;
    watch(main);
    const things = new Set<unknown>();
    main.traverse((o) => {
      const mesh = o as Object3D & { geometry?: unknown; material?: unknown };
      if (mesh.geometry) things.add(mesh.geometry);
      if (mesh.material) things.add(mesh.material);
    });
    scene.dispose();
    for (const thing of things) expect(freed.has(thing)).toBe(true);
  });

  it("uploads every position after a full update, even when a partial one follows before the next frame", () => {
    const renderer = fakeRenderer();
    const scene = createThreeScene({} as HTMLCanvasElement, () => renderer as unknown as WebGLRenderer);
    scene.setOptions({ bloom: false, ground: true });
    scene.setPixels(new Float32Array(9));
    scene.render(ORBIT);
    const points = () => {
      let found: Points | null = null;
      renderer.drawn.at(-1)!.traverse((o) => {
        if (o instanceof Points) found = o;
      });
      return found! as Points;
    };
    const position = points().geometry.getAttribute("position") as BufferAttribute;
    position.clearUpdateRanges();
    // New positions for every pixel (an undo, say), then a drag moves the last pixel in the same frame.
    scene.setPixels(new Float32Array([1, 1, 1, 2, 2, 2, 3, 3, 3]));
    scene.updatePixels(2, new Float32Array([4, 4, 4]));
    scene.render(ORBIT);
    // No ranges: three uploads the whole buffer.
    expect(position.updateRanges).toEqual([]);
    expect(Array.from(position.array)).toEqual([1, 1, 1, 2, 2, 2, 4, 4, 4]);
    // Afterwards, partial updates upload only what changed again.
    scene.updatePixels(0, new Float32Array([5, 5, 5]));
    expect(position.updateRanges).toEqual([{ start: 0, count: 3 }]);
  });
});
