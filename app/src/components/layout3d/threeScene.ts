// The 3D view drawn with three.js: a night scene (sky, ground), the photo as an upright
// backdrop, an optional house model, and every pixel as a bulb — all pixels in one draw call
// (one Points object whose positions and colors are updated in place). Lit bulbs glow as much
// as the viewer's Glow setting says, none at 0: each has a halo like the 2D previews' and the
// video export's, and a bloom pass spreads the light further. Only the pixels bloom (drawn
// alone, with the photo and model in black so they still hide what's behind them), never the
// photo's bright spots. Selection outlines and the move gizmo are drawn over the top, unbloomed.

import {
  AdditiveBlending,
  BackSide,
  Box3 as ThreeBox3,
  Box3Helper,
  BufferAttribute,
  BufferGeometry,
  CanvasTexture,
  Color,
  ConeGeometry,
  DirectionalLight,
  DoubleSide,
  DynamicDrawUsage,
  Euler,
  Fog,
  GridHelper,
  Group,
  HemisphereLight,
  Line,
  LineBasicMaterial,
  type Material,
  Mesh,
  MeshBasicMaterial,
  type Object3D,
  PerspectiveCamera,
  PlaneGeometry,
  Points,
  Raycaster,
  SRGBColorSpace,
  Scene,
  ShaderMaterial,
  SphereGeometry,
  Texture,
  Vector2,
  Vector3,
  WebGLRenderer,
} from "three";
import { EffectComposer } from "three/addons/postprocessing/EffectComposer.js";
import { OutputPass } from "three/addons/postprocessing/OutputPass.js";
import { RenderPass } from "three/addons/postprocessing/RenderPass.js";
import { ShaderPass } from "three/addons/postprocessing/ShaderPass.js";
import { UnrealBloomPass } from "three/addons/postprocessing/UnrealBloomPass.js";
import { type Box3, type GizmoHandle, type V3, FOV_DEG, PLANE_AT, PLANE_SIZE, bloomStrength, clipRange, orbitEye } from "../../lib/layout3d";
import type { Scene3d } from "./scene";

const SKY_TOP = new Color("#04060f");
const SKY_HORIZON = new Color("#141b33");
const GROUND = new Color("#070a09");
const ACCENT = new Color("#a78bfa");
const HIGHLIGHT = new Color("#facc15");
const AXIS_COLORS: Record<"x" | "y" | "z", Color> = { x: new Color("#ef4444"), y: new Color("#22c55e"), z: new Color("#3b82f6") };

/** A bulb's sprite is this many times its core's size: the rest is room for its halo. */
const SPRITE_PER_BULB = 5;

const PIXEL_VERTEX = /* glsl */ `
  attribute vec3 tint;
  uniform float bulb;
  uniform float scale;
  uniform float minPx;
  uniform float maxPx;
  varying vec3 vColor;
  varying float vCore;
  varying float vDim;
  void main() {
    vec4 mv = modelViewMatrix * vec4(position, 1.0);
    gl_Position = projectionMatrix * mv;
    float core = bulb * scale / max(-mv.z, 1e-3);
    float sprite = core * ${SPRITE_PER_BULB.toFixed(1)};
    gl_PointSize = clamp(sprite, minPx, maxPx);
    // The core is at least a pixel and a bit across, so far-off bulbs don't vanish; ones that
    // would be smaller than that are dimmed by how much smaller (by area: the light a pixel
    // gets), so a distant crowd of bulbs adds up to a glow instead of a white blot.
    vCore = max(core, 1.4) / gl_PointSize;
    float shrink = core / 1.4;
    vDim = clamp(shrink * shrink, 0.12, 1.0);
    vColor = pow(tint, vec3(2.2));
  }
`;

const PIXEL_FRAGMENT = /* glsl */ `
  uniform float glow;
  varying vec3 vColor;
  varying float vCore;
  varying float vDim;
  void main() {
    vec2 d = gl_PointCoord * 2.0 - 1.0;
    float r = length(d);
    if (r > 1.0) discard;
    float core = 1.0 - smoothstep(vCore * 0.6, vCore, r);
    // The halo the 2D previews and the video export draw at this glow level: a bell from the
    // bulb's edge out, 1.2 to 2.4 bulb radii wide and up to 0.7 of the color strong. That's a
    // share of the color as seen, so it's raised to 2.2 here, where colors add as light. It
    // fades out by the sprite's edge.
    float bell = vCore * (1.2 + 1.2 * glow);
    float seen = (1.0 - core) * 0.7 * glow * exp(-(r * r) / (bell * bell));
    float halo = pow(seen, 2.2) * (1.0 - smoothstep(0.85, 1.0, r));
    vec3 c = vColor * (core * 1.6 + halo) * vDim;
    gl_FragColor = vec4(c, 1.0);
    #include <colorspace_fragment>
  }
`;

const SKY_VERTEX = /* glsl */ `
  varying vec3 vDir;
  void main() {
    vDir = normalize(position);
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const MIX_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const MIX_FRAGMENT = /* glsl */ `
  uniform sampler2D baseTexture;
  uniform sampler2D bloomTexture;
  varying vec2 vUv;
  void main() {
    gl_FragColor = texture2D(baseTexture, vUv) + texture2D(bloomTexture, vUv);
  }
`;

const SKY_FRAGMENT = /* glsl */ `
  uniform vec3 top;
  uniform vec3 horizon;
  varying vec3 vDir;
  void main() {
    float t = pow(clamp(vDir.y, 0.0, 1.0), 0.45);
    gl_FragColor = vec4(mix(horizon, top, t), 1.0);
    #include <colorspace_fragment>
  }
`;

function disposeTree(root: Object3D) {
  root.traverse((o) => {
    const mesh = o as Mesh;
    mesh.geometry?.dispose();
    const materials = Array.isArray(mesh.material) ? mesh.material : mesh.material ? [mesh.material] : [];
    for (const m of materials) {
      for (const value of Object.values(m)) if (value instanceof Texture) value.dispose();
      m.dispose();
    }
  });
}

const toBox = (b: ThreeBox3): Box3 | null =>
  b.isEmpty() ? null : { min: { x: b.min.x, y: b.min.y, z: b.min.z }, max: { x: b.max.x, y: b.max.y, z: b.max.z } };

/** What to tell the user when a model file can't be shown (the loader's own words go to the console). */
export function plainModelError(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  if (/failed to load (buffer|texture)|couldn't load texture|failed to fetch|load failed/i.test(raw)) {
    return "This model needs files next to it that PixelFlow can't read. Export it as a single GLB file, or a glTF with everything embedded, and choose that.";
  }
  return "PixelFlow couldn't read this model. It may be damaged, or saved in a form PixelFlow doesn't support. Try exporting it as a GLB file.";
}

/** A model file's contents as three.js objects: glTF (binary or self-contained) or OBJ. */
async function parseModel(bytes: Uint8Array, name: string): Promise<Object3D> {
  try {
    if (name.split(".").pop()?.toLowerCase() === "obj") {
      const { OBJLoader } = await import("three/addons/loaders/OBJLoader.js");
      return new OBJLoader().parse(new TextDecoder().decode(bytes));
    }
    const { GLTFLoader } = await import("three/addons/loaders/GLTFLoader.js");
    // parseAsync reads the bytes where they are: a whole buffer needs no copy.
    const whole = bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength;
    const buffer = (whole ? bytes.buffer : bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)) as ArrayBuffer;
    return (await new GLTFLoader().parseAsync(buffer, "")).scene;
  } catch (error) {
    console.error(`The house model ${name} couldn't be read:`, error);
    throw new Error(plainModelError(error));
  }
}

/** The box around a model as its file has it (not placed). */
function naturalBox(object: Object3D): Box3 | null {
  object.updateMatrixWorld(true);
  return toBox(new ThreeBox3().setFromObject(object));
}

/** The model `measureModel` read last, kept so a view showing the same bytes next doesn't parse them again. */
let measured: { bytes: Uint8Array; object: Object3D } | null = null;

/** The kept model if it was read from `bytes`; any other kept model is freed. */
function takeMeasured(bytes: Uint8Array | null): Object3D | null {
  const kept = measured;
  measured = null;
  if (kept && kept.bytes === bytes) return kept.object;
  if (kept) disposeTree(kept.object);
  return null;
}

/** The box around the model in a file, as the file has it; null when it's empty. */
export async function measureModel(bytes: Uint8Array, name: string): Promise<Box3 | null> {
  const object = await parseModel(bytes, name);
  takeMeasured(null);
  measured = { bytes, object };
  return naturalBox(object);
}

/** How the house model is turned: about X, then Y, then Z (layout axes), as props are ("ZYX" to three.js). */
export function modelEuler(rotationDeg: V3): Euler {
  const r = Math.PI / 180;
  return new Euler(rotationDeg.x * r, rotationDeg.y * r, rotationDeg.z * r, "ZYX");
}

/** The move gizmo, one unit long: three arrows and three plane squares. */
function buildGizmo() {
  const group = new Group();
  const parts = new Map<GizmoHandle, { material: MeshBasicMaterial | LineBasicMaterial; color: Color }[]>();
  const overlay = (m: MeshBasicMaterial | LineBasicMaterial) => {
    m.depthTest = false;
    m.depthWrite = false;
    m.transparent = true;
    return m;
  };
  const dirs = { x: new Vector3(1, 0, 0), y: new Vector3(0, 1, 0), z: new Vector3(0, 0, 1) };
  for (const axis of ["x", "y", "z"] as const) {
    const color = AXIS_COLORS[axis];
    const lineMaterial = overlay(new LineBasicMaterial({ color }));
    const line = new Line(new BufferGeometry().setFromPoints([new Vector3(), dirs[axis].clone().multiplyScalar(0.84)]), lineMaterial);
    const coneMaterial = overlay(new MeshBasicMaterial({ color }));
    const cone = new Mesh(new ConeGeometry(0.05, 0.18, 16), coneMaterial);
    cone.position.copy(dirs[axis]).multiplyScalar(0.91);
    cone.quaternion.setFromUnitVectors(new Vector3(0, 1, 0), dirs[axis]);
    group.add(line, cone);
    parts.set(axis, [
      { material: lineMaterial, color },
      { material: coneMaterial, color },
    ]);
  }
  for (const plane of ["xy", "xz", "yz"] as const) {
    // A plane's square takes the color of the axis it doesn't move along.
    const other = (["x", "y", "z"] as const).find((a) => !plane.includes(a))!;
    const color = AXIS_COLORS[other];
    const material = overlay(new MeshBasicMaterial({ color, opacity: 0.45, side: DoubleSide }));
    const square = new Mesh(new PlaneGeometry(PLANE_SIZE, PLANE_SIZE), material);
    const mid = PLANE_AT + PLANE_SIZE / 2;
    const [a, b] = [dirs[plane[0] as "x" | "y" | "z"], dirs[plane[1] as "x" | "y" | "z"]];
    square.position.copy(a).multiplyScalar(mid).addScaledVector(b, mid);
    if (plane === "xz") square.rotation.x = -Math.PI / 2;
    if (plane === "yz") square.rotation.y = Math.PI / 2;
    group.add(square);
    parts.set(plane, [{ material, color }]);
  }
  group.renderOrder = 1000;
  group.traverse((o) => (o.renderOrder = 1000));
  const highlight = (handle: GizmoHandle | null) => {
    for (const [key, list] of parts) for (const { material, color } of list) material.color.copy(key === handle ? HIGHLIGHT : color);
  };
  return { group, highlight };
}

/** A WebGL renderer for the canvas. */
function webglRenderer(canvas: HTMLCanvasElement): WebGLRenderer {
  // No multisampling: hundreds of thousands of overlapping bulb sprites cost far more with it
  // (the bloom path renders without it anyway), and round sprites don't need it.
  return new WebGLRenderer({ canvas, antialias: false, powerPreference: "high-performance" });
}

/** The renderer for one canvas (`makeRenderer` lets tests run it without WebGL). */
export function createThreeScene(canvas: HTMLCanvasElement, makeRenderer: (canvas: HTMLCanvasElement) => WebGLRenderer = webglRenderer): Scene3d {
  const renderer = makeRenderer(canvas);
  renderer.outputColorSpace = SRGBColorSpace;
  renderer.setClearColor(SKY_TOP);

  const scene = new Scene();
  // The far ground fades into the night instead of shimmering at the horizon.
  scene.fog = new Fog(SKY_HORIZON, 80, 320);
  const overlayScene = new Scene();
  const camera = new PerspectiveCamera(FOV_DEG, 1, 0.1, 1000);

  const sky = new Mesh(
    new SphereGeometry(1, 32, 16),
    new ShaderMaterial({
      uniforms: { top: { value: SKY_TOP }, horizon: { value: SKY_HORIZON } },
      vertexShader: SKY_VERTEX,
      fragmentShader: SKY_FRAGMENT,
      side: BackSide,
      depthWrite: false,
    }),
  );
  sky.renderOrder = -10;
  scene.add(sky);

  const ground = new Group();
  const groundPlane = new Mesh(new PlaneGeometry(1, 1), new MeshBasicMaterial({ color: GROUND }));
  groundPlane.rotation.x = -Math.PI / 2;
  groundPlane.scale.setScalar(4000);
  groundPlane.position.y = -0.02;
  const grid = new GridHelper(200, 200, 0x23302a, 0x141c18);
  (grid.material as Material).transparent = true;
  (grid.material as Material).opacity = 0.55;
  ground.add(groundPlane, grid);
  scene.add(ground);

  const moon = new DirectionalLight(0xa9b8ff, 0.45);
  moon.position.set(-30, 60, 40);
  scene.add(new HemisphereLight(0x8a9bc4, 0x0b0d10, 0.55), moon);

  // Every pixel: one Points object, positions and colors updated in place.
  const pixelMaterial = new ShaderMaterial({
    uniforms: {
      bulb: { value: 0.04 },
      scale: { value: 600 },
      minPx: { value: 2.5 },
      maxPx: { value: 96 },
      glow: { value: 0 },
    },
    vertexShader: PIXEL_VERTEX,
    fragmentShader: PIXEL_FRAGMENT,
    transparent: true,
    depthWrite: false,
    blending: AdditiveBlending,
  });
  let pixelGeometry = new BufferGeometry();
  /** Every position must reach the GPU at the next frame, so partial updates add no ranges until then. */
  let fullUpload = false;
  const pixels = new Points(pixelGeometry, pixelMaterial);
  pixels.frustumCulled = false;
  pixels.renderOrder = 10;
  scene.add(pixels);

  // What blooms: the pixels alone, with the photo and model as black shapes in front of the
  // pixels behind them. The objects share their geometry with the main scene's.
  const bloomScene = new Scene();
  bloomScene.background = new Color(0x000000);
  const bloomPixels = new Points(pixelGeometry, pixelMaterial);
  bloomPixels.frustumCulled = false;
  bloomScene.add(bloomPixels);
  const occluder = new MeshBasicMaterial({ color: 0x000000 });
  let backdropShadow: Mesh | null = null;
  const modelShadow = new Group();
  bloomScene.add(modelShadow);

  let backdrop: Mesh<PlaneGeometry, MeshBasicMaterial> | null = null;
  let backdropImage: TexImageSource | null = null;
  const model = new Group();
  scene.add(model);
  /** The model's meshes, which props snap to (its lines and points are left out). */
  let modelMeshes: Object3D[] = [];
  let modelOpacity = 1;
  /** Counts model loads, so a slow one finishing after a newer one is dropped. */
  let modelLoads = 0;

  const selectionBox = new Box3Helper(new ThreeBox3(), ACCENT);
  (selectionBox.material as Material).depthTest = false;
  (selectionBox.material as Material).transparent = true;
  selectionBox.visible = false;
  overlayScene.add(selectionBox);
  const gizmo = buildGizmo();
  gizmo.group.visible = false;
  overlayScene.add(gizmo.group);

  const bloomComposer = new EffectComposer(renderer);
  bloomComposer.renderToScreen = false;
  const bloomRender = new RenderPass(bloomScene, camera);
  bloomComposer.addPass(bloomRender);
  // Its strength follows the glow level (see `setOptions`).
  const bloom = new UnrealBloomPass(new Vector2(256, 256), 0, 0.45, 0);
  bloomComposer.addPass(bloom);
  const composer = new EffectComposer(renderer);
  const mainRender = new RenderPass(scene, camera);
  composer.addPass(mainRender);
  const mix = new ShaderPass(
    new ShaderMaterial({
      uniforms: { baseTexture: { value: null }, bloomTexture: { value: bloomComposer.renderTarget2.texture } },
      vertexShader: MIX_VERTEX,
      fragmentShader: MIX_FRAGMENT,
    }),
    "baseTexture",
  );
  mix.needsSwap = true;
  composer.addPass(mix);
  const output = new OutputPass();
  composer.addPass(output);
  /** How much lit pixels glow (0–1), and whether the colors now are live ones. */
  let glow = 0;
  let lit = false;
  let size = { width: 1, height: 1, ratio: 1 };

  const raycaster = new Raycaster();

  const applyModelOpacity = () => {
    model.traverse((o) => {
      const mesh = o as Mesh;
      const materials = Array.isArray(mesh.material) ? mesh.material : mesh.material ? [mesh.material] : [];
      for (const m of materials) {
        m.transparent = modelOpacity < 1;
        m.opacity = modelOpacity;
        m.depthWrite = modelOpacity >= 0.5;
      }
    });
    // A see-through model doesn't hide the lights behind it, so it doesn't hide their glow either.
    modelShadow.visible = modelOpacity >= 0.5;
  };

  const placeShadow = () => {
    modelShadow.position.copy(model.position);
    modelShadow.quaternion.copy(model.quaternion);
    modelShadow.scale.copy(model.scale);
  };

  /** The model as black shapes for the bloom scene, placed like the model. */
  const shadowModel = () => {
    modelShadow.clear();
    for (const child of model.children) {
      const copy = child.clone(true);
      copy.traverse((o) => {
        if ((o as Mesh).isMesh) (o as Mesh).material = occluder;
      });
      modelShadow.add(copy);
    }
    placeShadow();
  };

  return {
    resize(next, ratio) {
      size = { width: Math.max(1, next.width), height: Math.max(1, next.height), ratio };
      renderer.setPixelRatio(ratio);
      renderer.setSize(size.width, size.height, false);
      composer.setPixelRatio(ratio);
      composer.setSize(size.width, size.height);
      bloomComposer.setPixelRatio(ratio);
      // The bloom pass sizes its own targets from this, at half resolution and smaller.
      bloomComposer.setSize(size.width, size.height);
      camera.aspect = size.width / size.height;
    },

    setPixels(xyz) {
      const count = xyz.length / 3;
      const current = pixelGeometry.getAttribute("position") as BufferAttribute | undefined;
      if (current && current.count === count) {
        (current.array as Float32Array).set(xyz);
        current.clearUpdateRanges();
        current.needsUpdate = true;
        fullUpload = true;
        return;
      }
      fullUpload = true;
      pixelGeometry.dispose();
      pixelGeometry = new BufferGeometry();
      const position = new BufferAttribute(new Float32Array(xyz), 3);
      position.setUsage(DynamicDrawUsage);
      const tint = new BufferAttribute(new Uint8Array(count * 3), 3, true);
      tint.setUsage(DynamicDrawUsage);
      pixelGeometry.setAttribute("position", position);
      pixelGeometry.setAttribute("tint", tint);
      pixels.geometry = pixelGeometry;
      bloomPixels.geometry = pixelGeometry;
    },

    updatePixels(start, xyz) {
      const position = pixelGeometry.getAttribute("position") as BufferAttribute | undefined;
      if (!position || start * 3 + xyz.length > position.array.length) return;
      (position.array as Float32Array).set(xyz, start * 3);
      if (!fullUpload) position.addUpdateRange(start * 3, xyz.length);
      position.needsUpdate = true;
    },

    setColors(rgb, live) {
      const tint = pixelGeometry.getAttribute("tint") as BufferAttribute | undefined;
      if (!tint) return;
      const array = tint.array as Uint8Array;
      array.set(rgb.length > array.length ? rgb.subarray(0, array.length) : rgb);
      tint.clearUpdateRanges();
      tint.needsUpdate = true;
      lit = live;
      // Only lit pixels glow: unlit bulbs are just small grey dots.
      pixelMaterial.uniforms.glow.value = lit ? glow : 0;
    },

    setBulbSize(bulb) {
      pixelMaterial.uniforms.bulb.value = bulb;
    },

    setBackdrop(next) {
      const remove = () => {
        if (backdrop) {
          scene.remove(backdrop);
          disposeTree(backdrop);
        }
        if (backdropShadow) bloomScene.remove(backdropShadow);
        backdrop = backdropShadow = null;
        backdropImage = null;
      };
      if (!next) return remove();
      if (!backdrop || backdropImage !== next.image) {
        remove();
        let texture: Texture;
        if (typeof ImageBitmap !== "undefined" && next.image instanceof ImageBitmap) {
          // Bitmaps ignore flipY in WebGL: draw it on a canvas, which doesn't.
          const c = document.createElement("canvas");
          [c.width, c.height] = [next.image.width, next.image.height];
          c.getContext("2d")?.drawImage(next.image, 0, 0);
          texture = new CanvasTexture(c);
        } else {
          texture = new Texture(next.image as HTMLImageElement);
          texture.needsUpdate = true;
        }
        texture.colorSpace = SRGBColorSpace;
        texture.anisotropy = renderer.capabilities.getMaxAnisotropy();
        // The photo is dimmed a little: it's a night scene.
        backdrop = new Mesh(new PlaneGeometry(1, 1), new MeshBasicMaterial({ map: texture, transparent: true, color: new Color(0.8, 0.8, 0.8), side: DoubleSide }));
        backdropImage = next.image;
        scene.add(backdrop);
        backdropShadow = new Mesh(backdrop.geometry, occluder);
        bloomScene.add(backdropShadow);
      }
      const { min, max } = next.box;
      backdrop.scale.set(max.x - min.x, max.y - min.y, 1);
      backdrop.position.set((min.x + max.x) / 2, (min.y + max.y) / 2, min.z);
      backdrop.material.opacity = next.opacity;
      backdrop.material.depthWrite = next.opacity > 0.5;
      backdropShadow!.scale.copy(backdrop.scale);
      backdropShadow!.position.copy(backdrop.position);
      backdropShadow!.visible = next.opacity > 0.5;
    },

    async setModel(next) {
      const load = ++modelLoads;
      for (const child of [...model.children]) {
        model.remove(child);
        disposeTree(child);
      }
      modelShadow.clear();
      modelMeshes = [];
      const kept = takeMeasured(next?.bytes ?? null);
      if (!next) return null;
      const loaded = kept ?? (await parseModel(next.bytes, next.name));
      if (load !== modelLoads) {
        disposeTree(loaded);
        return null;
      }
      const box = naturalBox(loaded);
      model.add(loaded);
      loaded.traverse((o) => {
        if ((o as Mesh).isMesh) modelMeshes.push(o);
      });
      shadowModel();
      applyModelOpacity();
      return box;
    },

    placeModel(p) {
      model.position.set(p.position.x, p.position.y, p.position.z);
      model.setRotationFromEuler(modelEuler(p.rotationDeg));
      model.scale.setScalar(p.scale);
      modelOpacity = p.opacity;
      applyModelOpacity();
      placeShadow();
      model.updateMatrixWorld(true);
      return model.children.length ? toBox(new ThreeBox3().setFromObject(model)) : null;
    },

    surfaceAt(ray) {
      if (modelMeshes.length === 0) return null;
      raycaster.set(new Vector3(ray.origin.x, ray.origin.y, ray.origin.z), new Vector3(ray.dir.x, ray.dir.y, ray.dir.z));
      const hit = raycaster.intersectObjects(modelMeshes, false)[0];
      return hit ? { x: hit.point.x, y: hit.point.y, z: hit.point.z } : null;
    },

    setSelectionBox(box) {
      selectionBox.visible = !!box;
      if (box) selectionBox.box.set(new Vector3(box.min.x, box.min.y, box.min.z), new Vector3(box.max.x, box.max.y, box.max.z));
    },

    setGizmo(next) {
      gizmo.group.visible = !!next;
      if (!next) return;
      gizmo.group.position.set(next.origin.x, next.origin.y, next.origin.z);
      gizmo.group.scale.setScalar(next.length);
      gizmo.highlight(next.highlight);
    },

    setOptions(options) {
      glow = Number.isFinite(options.glow) ? Math.min(1, Math.max(0, options.glow)) : 0;
      pixelMaterial.uniforms.glow.value = lit ? glow : 0;
      bloom.strength = bloomStrength(glow);
      ground.visible = options.ground;
    },

    render(orbit) {
      const eye = orbitEye(orbit);
      const { near, far } = clipRange(orbit);
      camera.near = near;
      camera.far = far;
      camera.position.set(eye.x, eye.y, eye.z);
      camera.lookAt(orbit.target.x, orbit.target.y, orbit.target.z);
      camera.updateProjectionMatrix();
      sky.position.copy(camera.position);
      sky.scale.setScalar(far * 0.9);
      pixelMaterial.uniforms.scale.value = (size.height * size.ratio) / (2 * Math.tan(((FOV_DEG / 2) * Math.PI) / 180));
      pixelMaterial.uniforms.maxPx.value = 96 * size.ratio;
      pixelMaterial.uniforms.minPx.value = 2.5 * size.ratio;
      renderer.autoClear = true;
      // With nothing glowing there's nothing to bloom: the scene is drawn straight to the screen.
      if (pixelMaterial.uniforms.glow.value > 0) {
        bloomComposer.render();
        composer.render();
      } else renderer.render(scene, camera);
      fullUpload = false;
      if (selectionBox.visible || gizmo.group.visible) {
        renderer.autoClear = false;
        renderer.clearDepth();
        renderer.render(overlayScene, camera);
      }
    },

    dispose() {
      modelLoads++;
      disposeTree(scene);
      disposeTree(overlayScene);
      disposeTree(bloomScene);
      occluder.dispose();
      // A composer frees only its own targets and copy pass: every other pass is freed here.
      for (const pass of [bloomRender, bloom, mainRender, mix, output]) pass.dispose();
      bloomComposer.dispose();
      composer.dispose();
      renderer.renderLists.dispose();
      renderer.dispose();
      // Let go of the WebGL context now rather than whenever it's collected: browsers allow only
      // a few at once, and each switch to 3D makes a new one.
      renderer.forceContextLoss();
    },
  };
}
