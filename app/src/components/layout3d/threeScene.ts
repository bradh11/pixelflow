// The 3D view drawn with three.js: a night scene (sky, ground), the photo as an upright
// backdrop, an optional house model, and every pixel as a crisp dot, by the flat previews' rules
// (lib/pixelBatches.ts) worked out in perspective. All pixels are one set of points whose
// positions and colors are updated in place; their shader sizes each dot from how far apart its
// prop's pixels are and how far it is from the camera (see `lightWidth` in lib/layout3d.ts).
//
// A dot is drawn in exactly its color, over what's behind it: nothing adds up. (Only where a
// prop's pixels are closer together than a screen pixel, and can't each show, does a light take
// on some of its wire neighbors' color.) Each place on the screen shows one light. Lights with a
// color of their own (lit, or selected) come before the quiet ones (off, or not selected), so a
// light that's off never covers one that's lit, wherever they stand; a dot's solid part comes
// before any soft edge, so the edge of a near dot never shows through to the dark where a
// farther dot is solid (a string seen end on stays a solid line); among solid parts it's the
// nearest to the camera, each dot a shallow cone pointing at it, so pixels too close together
// for dots of their own run into a clean line, cut where one pixel's light ends and the next
// begins; and among edges, the fullest. The depth buffer settles it: once the scene is drawn
// it's emptied, and every dot is drawn into it alone, at a depth that says where it comes in
// that order; then in color, each place's first dot only.
//
// What hides a light (the photo, the house model) is drawn into a small depth picture first, and
// each light looks there for whether anything stands in front of it.
//
// Lit pixels glow as much as the viewer's Glow setting says, none at 0: each light that isn't
// hidden has the flat previews' halo (lib/pixelGlow.ts), drawn into a coarse picture of its own
// and added over the dots. Selection outlines and the move gizmo are drawn over the top.

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
  CustomBlending,
  DepthTexture,
  DirectionalLight,
  DoubleSide,
  DynamicDrawUsage,
  EqualDepth,
  Euler,
  Fog,
  GridHelper,
  Group,
  HalfFloatType,
  HemisphereLight,
  InterleavedBuffer,
  InterleavedBufferAttribute,
  LessDepth,
  Line,
  LineSegments,
  LineBasicMaterial,
  type Material,
  Mesh,
  MeshBasicMaterial,
  NoBlending,
  NotEqualStencilFunc,
  type Object3D,
  OneFactor,
  OneMinusSrcAlphaFactor,
  PerspectiveCamera,
  PlaneGeometry,
  Points,
  Raycaster,
  ReplaceStencilOp,
  SRGBColorSpace,
  Scene,
  ShaderMaterial,
  SphereGeometry,
  Texture,
  Vector2,
  Vector3,
  Vector4,
  WebGLRenderTarget,
  WebGLRenderer,
} from "three";
import { FullScreenQuad } from "three/addons/postprocessing/Pass.js";
import {
  type Box3,
  type GizmoHandle,
  type LightLook,
  type V3,
  COLORED_EDGE,
  COLORED_SOLID,
  DOT_GAP,
  DOT_KINDS,
  FOV_DEG,
  HALO_FROM,
  PLANE_AT,
  PLANE_SIZE,
  QUIET_EDGE,
  QUIET_SOLID,
  SPREAD_SIZE,
  clipRange,
  orbitEye,
  pixelsPerUnit,
} from "../../lib/layout3d";
import { DOT_RADIUS, FAINT, FILL, FINE_FROM, SMALLEST } from "../../lib/pixelBatches";
import { ARM_COLOR, BEAM_FADE, BEAM_STRENGTH, UNLIT_BEAM, spread } from "../../lib/beams";
import { FLOOD_RADIUS, FLOOD_STRENGTH, SMALLEST_WASH, UNLIT_WASH } from "../../lib/floodWash";
import { GLOW_STEPS, GLOW_STRENGTH, GLOW_WIDTH, glowStep } from "../../lib/pixelGlow";
import type { Scene3d } from "./scene";

const SKY_TOP = new Color("#04060f");
const SKY_HORIZON = new Color("#141b33");
const GROUND = new Color("#070a09");
const ACCENT = new Color("#a78bfa");
const HIGHLIGHT = new Color("#facc15");
const AXIS_COLORS: Record<"x" | "y" | "z", Color> = { x: new Color("#ef4444"), y: new Color("#22c55e"), z: new Color("#3b82f6") };

/** A number as a shader writes it. */
const glsl = (n: number) => (Number.isInteger(n) ? n.toFixed(1) : String(n));

/**
 * A dot's cone stands at least this many of the depth buffer's steps toward the camera, so
 * that far off, where the steps are coarse, it still tells neighbors' cones apart.
 */
const CONE_STEPS = 32;
/** A halo is drawn out to this many of its bell's widths, where it has faded to nothing that shows. */
const HALO_REACH = 2.4;
/** A halo's bell is drawn no narrower than this in the coarse picture (its pixels), fainter to make up: narrower, the picture couldn't hold its shape. */
const NARROWEST_BELL = 0.75;

/** The view's dot at a light (`largest`, device pixels) from `unit`, the device pixels to a layout unit there. */
const VIEW_DOT = /* glsl */ `
  uniform float scale;
  uniform float ratio;
  uniform vec2 radius;
  float viewDot(float unit) {
    return clamp(2.0 * ${glsl(DOT_RADIUS)} * unit, 2.0 * radius.x * ratio, 2.0 * radius.y * ratio);
  }
`;

/** A lit light's halo at glow level `glow`: its bell's width (device pixels) and its strength beside the dot (see `lightHalo`). */
const HALO = /* glsl */ `
  uniform float glow;
  vec2 haloOf(float unit) {
    float bell = min(radius.y * ratio, ${glsl(DOT_RADIUS)} * unit) * (${glsl(GLOW_WIDTH[0])} + ${glsl(GLOW_WIDTH[1] - GLOW_WIDTH[0])} * glow);
    float strength = (${glsl(GLOW_STRENGTH[0])} + ${glsl(GLOW_STRENGTH[1] - GLOW_STRENGTH[0])} * glow) * smoothstep(${glsl(HALO_FROM[0])}, ${glsl(HALO_FROM[1])}, bell);
    return vec2(bell, strength);
  }
`;

/** Puts a light that isn't drawn off the screen. */
const SKIP = /* glsl */ `{
      gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
      gl_PointSize = 1.0;
      return;
    }`;

/**
 * Whether something stands in front of a light `depth` from the camera, drawn at `clip`: whether
 * the depth picture has something nearer there. It takes the farthest of the picture's pixels
 * around the place, so a light on a wall seen at a slant isn't hidden by the wall beside it.
 */
const HIDDEN = /* glsl */ `
  uniform sampler2D hides;
  uniform vec3 hiding;
  uniform vec2 project;
  bool hidden(vec4 clip, float depth) {
    if (hiding.x < 0.5) return false;
    vec2 at = clip.xy / clip.w * 0.5 + 0.5;
    float deepest = 0.0;
    for (int y = -1; y <= 1; y++) {
      for (int x = -1; x <= 1; x++) {
        deepest = max(deepest, texture2D(hides, at + vec2(float(x), float(y)) * hiding.yz).r);
      }
    }
    // The picture's depth as a distance from the camera, and a hair more: more than the picture's
    // own steps of depth, and far less than a light stands off the wall it's on.
    return depth > 1.0001 * project.y / (2.0 * deepest - 1.0 + project.x);
  }
`;

// A light's dot (see `lightWidth`, `lightDim`): how wide, its color, and where it comes in the
// order dots are kept in. `tintBefore` and `tintAfter` are the colors of the lights wired before
// and after it.
const LIGHT_VERTEX = /* glsl */ `
  attribute vec3 tint;
  attribute vec3 tintBefore;
  attribute vec3 tintAfter;
  attribute vec4 spread;
  uniform vec4 quiet;
  uniform float faint;
  uniform vec2 depths;
  uniform vec2 viewport;
  uniform float still;
  varying vec4 vColor;
  varying vec3 vDot;
  varying vec4 vOrder;
  ${VIEW_DOT}
  ${HALO}
  ${HIDDEN}
  float fit(float pitch, float smallest, float largest) {
    return max(smallest, min(largest, min(pitch * ${glsl(FILL)}, pitch - ${glsl(DOT_GAP)})));
  }
  bool has(vec3 color) {
    return max(color.r, max(color.g, color.b)) > 0.0;
  }
  // The color of a light's dot, as light: its own (dimmer by dim); or, for a light with none
  // or a faint one, the quiet color showing through, the more the dimmer it is.
  vec4 looks(vec3 color, float dim) {
    float most = max(color.r, max(color.g, color.b));
    float solid = faint > 0.0 ? min(1.0, most / faint) : sign(most);
    return vec4(color * dim, solid) + (1.0 - solid) * quiet;
  }
  void main() {
    vec4 mv = modelViewMatrix * vec4(position, 1.0);
    gl_Position = projectionMatrix * mv;
    float depth = max(-mv.z, 1e-3);
    if (hidden(gl_Position, depth)) ${SKIP}
    float unit = scale / depth;
    float largest = viewDot(unit);
    float smallest = min(largest, ${glsl(SMALLEST)} * ratio);
    float along = spread.x * unit;
    float line = min(fit(spread.y * unit, smallest, largest), max(smallest, largest * ${glsl(FILL)}));
    float width = max(fit(along, smallest, largest), line);
    // Pixels with neighbors on every side, where their dots overlap, are whole squares: round
    // ones would leave the corners between four of them half covered. In between, the corners
    // round off little by little.
    float corners = spread.z > 0.5 ? clamp((along - width) / (0.25 * width), 0.0, 1.0) : 1.0;
    // Half a device pixel all round for the soft edge.
    gl_PointSize = width + 1.0;
    // With the camera still, a small dot is a whole number of device pixels wide and sits on
    // whole device pixels (stillDot in lib/layout3d.ts): a soft edge that fell anywhere would
    // blur it. Too small to round, it's its whole square.
    if (still > 0.5 && width < ${glsl(FINE_FROM)}) {
      width = floor(width + 0.5);
      vec2 at = (gl_Position.xy / gl_Position.w * 0.5 + 0.5) * viewport;
      at = floor(at - 0.5 * width + 0.5) + 0.5 * width;
      gl_Position.xy = (at / viewport * 2.0 - 1.0) * gl_Position.w;
      gl_PointSize = width;
      if (width < 3.0) corners = 0.0;
    }
    vDot = vec3(gl_PointSize, width, corners);
    // The lights wired before and after it, where they're its own prop's (spread.w says).
    bool hasBefore = mod(spread.w, 2.0) < 0.5;
    bool hasAfter = spread.w < 1.5;
    bool own = has(tint);
    bool beside = (hasBefore && has(tintBefore)) || (hasAfter && has(tintAfter));
    // Pixels closer together than a screen pixel can't each show their own color: each takes on
    // some of the two wired next to it, up to a third from each (neighborShare in
    // lib/layout3d.ts), as the flat previews make one light of such pixels, in their average
    // color.
    float share = clamp(1.0 - along / ratio, 0.0, 1.0) / 3.0;
    bool colored = own || (share > 0.0 && beside);
    // The cone is as deep as the dot is wide, in the layout (and never so shallow that the depth
    // buffer can't tell its slope).
    float cone = max(width / unit, depth * (exp2(${glsl(CONE_STEPS * DOT_KINDS)} / (depths.y * 16777216.0)) - 1.0));
    // A quiet light next on the wire to one with a color keeps its own half of the way between
    // them, as in the flat previews: its dot's solid part is kept as a colored light's is, so
    // the colored one's dot ends half way to it.
    vOrder = vec4(depth, cone, own || beside ? ${glsl(COLORED_SOLID)} : ${glsl(QUIET_SOLID)}, colored ? ${glsl(COLORED_EDGE)} : ${glsl(QUIET_EDGE)});
    if (!colored) {
      vColor = quiet;
      return;
    }
    // Under a halo, a dot is drawn dimmer by what the halo adds back.
    vec2 halo = haloOf(unit);
    float w = 2.0 * halo.x / width;
    float dim = halo.y > 0.0 ? 1.0 - halo.y * w * w * (1.0 - exp(-1.0 / (w * w))) : 1.0;
    vec4 mine = looks(tint, dim);
    vColor = mine + share * ((hasBefore ? looks(tintBefore, dim) : mine) + (hasAfter ? looks(tintAfter, dim) : mine) - 2.0 * mine);
  }
`;

// A dot's depth says where it comes in the order dots are kept in at a place (`dotOrder` in
// lib/layout3d.ts): each kind has its band of the depth buffer, the first kind nearest.
const LIGHT_FRAGMENT = /* glsl */ `
  uniform vec2 depths;
  varying vec4 vColor;
  varying vec3 vDot;
  varying vec4 vOrder;
  void main() {
    // From the dot's middle, in device pixels.
    vec2 p = (gl_PointCoord - 0.5) * vDot.x;
    float reach = 0.5 * vDot.y;
    float corner = reach * vDot.z;
    vec2 q = abs(p) - (reach - corner);
    // How much of this device pixel the dot covers: all of it inside, less across its soft edge
    // (one pixel wide), nothing past that.
    float cover = clamp(0.5 + corner - length(max(q, 0.0)) - min(max(q.x, q.y), 0.0), 0.0, 1.0);
    if (cover <= 0.0) discard;
    if (cover >= 1.0) {
      // Solid: the nearest to the camera first, the dot a cone with its tip at its middle and
      // half its depth left by a round dot's rim (coneFrom in lib/layout3d.ts). Distances count
      // by their ratio, so near and far lights are told apart as finely.
      float from = max(vOrder.x - vOrder.y * (1.0 - 0.5 * dot(p, p) / (reach * reach)), depths.x);
      gl_FragDepth = (vOrder.z + min(log2(from / depths.x) * depths.y, 0.9999)) / ${glsl(DOT_KINDS)};
    } else {
      // A soft edge: the fullest first.
      gl_FragDepth = (vOrder.w + 1.0 - cover) / ${glsl(DOT_KINDS)};
    }
    gl_FragColor = vColor * cover;
  }
`;

// A lit light's halo, in the coarse picture (`cell` device pixels to one of its pixels).
const HALO_VERTEX = /* glsl */ `
  attribute vec3 tint;
  uniform float cell;
  varying vec3 vColor;
  varying vec2 vBell;
  ${VIEW_DOT}
  ${HALO}
  ${HIDDEN}
  void main() {
    if (max(tint.r, max(tint.g, tint.b)) <= 0.0) ${SKIP}
    vec4 mv = modelViewMatrix * vec4(position, 1.0);
    gl_Position = projectionMatrix * mv;
    float depth = max(-mv.z, 1e-3);
    vec2 halo = haloOf(scale / depth);
    if (halo.y <= 0.0 || hidden(gl_Position, depth)) ${SKIP}
    float bell = halo.x / cell;
    float drawn = max(bell, ${glsl(NARROWEST_BELL)});
    gl_PointSize = 2.0 * ${glsl(HALO_REACH)} * drawn + 1.0;
    // A bell drawn wider than it is holds the same light, spread thinner.
    vColor = tint * halo.y * (bell * bell) / (drawn * drawn);
    vBell = vec2(gl_PointSize, drawn);
  }
`;

const HALO_FRAGMENT = /* glsl */ `
  varying vec3 vColor;
  varying vec2 vBell;
  void main() {
    vec2 p = (gl_PointCoord - 0.5) * vBell.x;
    // Less what's left of the bell at the halo's edge, so it ends without a step.
    float bell = exp(-dot(p, p) / (vBell.y * vBell.y)) - ${glsl(Math.exp(-HALO_REACH * HALO_REACH))};
    if (bell <= 0.0) discard;
    gl_FragColor = vec4(vColor * bell, 1.0);
  }
`;

// A DMX flood's cell: a soft round wash of its color round it, as the flat previews stamp it
// (lib/floodWash.ts): reaching FLOOD_RADIUS layout units (never less than SMALLEST_WASH screen
// pixels), FLOOD_STRENGTH × (1 - r²)² solid r of the way out, over what's behind it and under every
// dot. Not playing, a faint wash of the quiet color (or its own, selected) shows its reach.
const WASH_VERTEX = /* glsl */ `
  attribute vec3 tint;
  attribute float flood;
  uniform vec4 quiet;
  uniform float lit;
  varying vec4 vColor;
  varying float vReach;
  ${VIEW_DOT}
  ${HIDDEN}
  void main() {
    if (flood < 0.5) ${SKIP}
    vec4 mv = modelViewMatrix * vec4(position, 1.0);
    gl_Position = projectionMatrix * mv;
    float depth = max(-mv.z, 1e-3);
    if (hidden(gl_Position, depth)) ${SKIP}
    bool own = max(tint.r, max(tint.g, tint.b)) > 0.0;
    if (lit > 0.5 && !own) ${SKIP}
    float reach = max(${glsl(FLOOD_RADIUS)} * scale / depth, ${glsl(SMALLEST_WASH)} * ratio);
    vec3 color = own ? tint : quiet.rgb / max(quiet.a, 1e-3);
    vColor = vec4(color, lit > 0.5 ? ${glsl(FLOOD_STRENGTH)} : ${glsl(UNLIT_WASH)});
    vReach = reach;
    gl_PointSize = 2.0 * reach + 1.0;
  }
`;

const WASH_FRAGMENT = /* glsl */ `
  varying vec4 vColor;
  varying float vReach;
  void main() {
    vec2 p = (gl_PointCoord - 0.5) * (2.0 * vReach + 1.0);
    float r2 = dot(p, p) / (vReach * vReach);
    if (r2 >= 1.0) discard;
    float t = 1.0 - r2;
    gl_FragColor = vec4(vColor.rgb, vColor.a * t * t);
  }
`;

// A moving head's beam: a cone from the head opening along its direction (lib/beams.ts), as solid
// as the flat previews' beam along its middle (BEAM_STRENGTH, losing BEAM_FADE of it by the far
// end), softening toward its edges as seen from the camera.
const BEAM_VERTEX = /* glsl */ `
  varying float vAlong;
  varying vec3 vNormal;
  varying vec3 vView;
  void main() {
    vAlong = -position.y;
    vec4 seen = modelViewMatrix * vec4(position, 1.0);
    vNormal = normalize(normalMatrix * normal);
    vView = normalize(-seen.xyz);
    gl_Position = projectionMatrix * seen;
  }
`;

const BEAM_FRAGMENT = /* glsl */ `
  uniform vec3 color;
  uniform float strength;
  varying float vAlong;
  varying vec3 vNormal;
  varying vec3 vView;
  void main() {
    float facing = abs(dot(normalize(vNormal), normalize(vView)));
    gl_FragColor = vec4(color, strength * (1.0 - ${glsl(BEAM_FADE)} * clamp(vAlong, 0.0, 1.0)) * facing * facing);
  }
`;

const SKY_VERTEX = /* glsl */ `
  varying vec3 vDir;
  void main() {
    vDir = normalize(position);
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const OVER_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const OVER_FRAGMENT = /* glsl */ `
  uniform sampler2D halos;
  varying vec2 vUv;
  void main() {
    gl_FragColor = vec4(texture2D(halos, vUv).rgb, 1.0);
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
  // No multisampling: hundreds of thousands of dots cost far more with it, and they soften their
  // own edges. The stencil buffer keeps a place to one light where two tie for it.
  return new WebGLRenderer({ canvas, antialias: false, stencil: true, powerPreference: "high-performance" });
}

/**
 * The material the lights are drawn with, in one of their two passes: into the depth buffer
 * alone (`settling`), which keeps the first dot at each place in `LIGHT_FRAGMENT`'s order; or in
 * color, each place's kept dot and nothing else.
 */
function lightMaterial(uniforms: ShaderMaterial["uniforms"], settling: boolean): ShaderMaterial {
  return new ShaderMaterial({
    uniforms,
    vertexShader: LIGHT_VERTEX,
    fragmentShader: LIGHT_FRAGMENT,
    transparent: true,
    ...(settling
      ? { colorWrite: false, depthFunc: LessDepth, blending: NoBlending }
      : {
          depthWrite: false,
          depthFunc: EqualDepth,
          // Colors come out of the shader already multiplied by how solid they are.
          blending: CustomBlending,
          blendSrc: OneFactor,
          blendDst: OneMinusSrcAlphaFactor,
          // Where two dots tie for a place, only the first is drawn: it marks the place as taken.
          stencilWrite: true,
          stencilFunc: NotEqualStencilFunc,
          stencilRef: 1,
          stencilZPass: ReplaceStencilOp,
        }),
  });
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

  // What hides lights: the photo and the house model, drawn into a depth picture a screen pixel
  // to each of its own, where every light looks for whether something stands in front of it.
  const hidingScene = new Scene();
  const hidingTarget = new WebGLRenderTarget(1, 1, { depthTexture: new DepthTexture(1, 1) });

  // Every pixel: one set of points, positions and colors updated in place, drawn over the
  // finished scene in two passes (see `lightMaterial`).
  const dots = {
    scale: { value: 600 },
    ratio: { value: 1 },
    radius: { value: new Vector2(1, 4) },
    glow: { value: 0 },
    quiet: { value: new Vector4() },
    faint: { value: 0 },
    /** How far the nearest thing drawn is from the camera, and one over the doublings from there to the farthest. */
    depths: { value: new Vector2(0.1, 0.1) },
    /** What turns a depth in a depth picture into a distance from the camera. */
    project: { value: new Vector2(-1, -0.2) },
    viewport: { value: new Vector2(1, 1) },
    still: { value: 1 },
    hides: { value: hidingTarget.depthTexture },
    /** 1 when something may hide lights, and how far apart the depth picture's pixels are across it and down it. */
    hiding: { value: new Vector3(0, 1, 1) },
  };
  let pixelGeometry = new BufferGeometry();
  /** Every position must reach the GPU at the next frame, so partial updates add no ranges until then. */
  let fullUpload = false;
  const lightScene = new Scene();
  const passes = [true, false].map((settling, order) => {
    const points = new Points(pixelGeometry, lightMaterial(dots, settling));
    points.frustumCulled = false;
    points.renderOrder = order;
    lightScene.add(points);
    return points;
  });

  // DMX floods' washes, over the finished scene and under the dots. The points share their
  // geometry with the dots'; only floods' cells (`flood`) are drawn.
  const washLit = { value: 0 };
  const washScene = new Scene();
  const washes = new Points(
    pixelGeometry,
    new ShaderMaterial({
      uniforms: { scale: dots.scale, ratio: dots.ratio, radius: dots.radius, quiet: dots.quiet, lit: washLit, project: dots.project, hides: dots.hides, hiding: dots.hiding },
      vertexShader: WASH_VERTEX,
      fragmentShader: WASH_FRAGMENT,
      transparent: true,
      depthTest: false,
      depthWrite: false,
    }),
  );
  washes.frustumCulled = false;
  washScene.add(washes);
  let anyFloods = false;

  // Moving heads' beams and servos' arms, over the scene (behind the house model where it stands
  // in front of them) and under the dots. A beam's cone has its point at the origin and opens
  // along -y, one unit long and one wide: each is placed, turned and stretched to its own.
  const motionScene = new Scene();
  const beamCone = new ConeGeometry(1, 1, 32, 1, true);
  beamCone.translate(0, -0.5, 0);
  const beamMeshes: Mesh<ConeGeometry, ShaderMaterial>[] = [];
  const arms = new LineSegments(new BufferGeometry(), new LineBasicMaterial({ color: new Color(ARM_COLOR[0] / 255, ARM_COLOR[1] / 255, ARM_COLOR[2] / 255) }));
  arms.frustumCulled = false;
  motionScene.add(arms);
  let anyMotion = false;

  // The halos: the lit pixels alone, drawn into a picture a screen pixel to each of its own.
  // The points share their geometry with the dots'.
  const haloScene = new Scene();
  haloScene.background = new Color(0x000000);
  const haloCell = { value: 1 };
  const halos = new Points(
    pixelGeometry,
    new ShaderMaterial({
      uniforms: { scale: dots.scale, ratio: dots.ratio, radius: dots.radius, glow: dots.glow, cell: haloCell, project: dots.project, hides: dots.hides, hiding: dots.hiding },
      vertexShader: HALO_VERTEX,
      fragmentShader: HALO_FRAGMENT,
      transparent: true,
      depthTest: false,
      depthWrite: false,
      blending: AdditiveBlending,
    }),
  );
  halos.frustumCulled = false;
  haloScene.add(halos);
  const haloTarget = new WebGLRenderTarget(1, 1, { type: HalfFloatType, depthBuffer: false });
  // Added over the finished picture, a pixel of the halos' picture smoothed across the screen pixels it covers.
  const haloOver = new FullScreenQuad(
    new ShaderMaterial({
      uniforms: { halos: { value: haloTarget.texture } },
      vertexShader: OVER_VERTEX,
      fragmentShader: OVER_FRAGMENT,
      transparent: true,
      depthTest: false,
      depthWrite: false,
      blending: AdditiveBlending,
    }),
  );
  // Only where they are matters, from either side.
  const occluder = new MeshBasicMaterial({ colorWrite: false, side: DoubleSide });
  let backdropShadow: Mesh | null = null;
  const modelShadow = new Group();
  hidingScene.add(modelShadow);

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

  /** How much lit pixels glow (0–1, at the step the flat previews draw it), and whether the colors now are live ones. */
  let glow = 0;
  let lit = false;
  let look: LightLook = { radius: [1, 4], unlit: [0, 0, 0, 0], dark: [0, 0, 0, 0] };
  let size = { width: 1, height: 1, ratio: 1 };

  /** The uniforms that follow the look and whether the colors are live. */
  const applyLook = () => {
    dots.radius.value.set(look.radius[0], look.radius[1]);
    // Playing, a pixel that's off; otherwise one that isn't selected. As light: each color times how solid it is.
    const [r, g, b, a] = lit ? look.dark : look.unlit;
    dots.quiet.value.set((r / 255) * a, (g / 255) * a, (b / 255) * a, a);
    dots.faint.value = lit ? FAINT / 255 : 0;
    // Only lit pixels glow.
    dots.glow.value = lit ? glow : 0;
    washLit.value = lit ? 1 : 0;
  };

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
    // A see-through model doesn't hide the lights behind it.
    modelShadow.visible = modelOpacity >= 0.5;
  };

  const placeShadow = () => {
    modelShadow.position.copy(model.position);
    modelShadow.quaternion.copy(model.quaternion);
    modelShadow.scale.copy(model.scale);
  };

  /** The model's shape for the depth picture of what hides lights, placed like the model. */
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
      // The halos' picture, and the depth picture of what hides lights, have a pixel to a screen
      // pixel (to a device pixel, where those are bigger).
      haloCell.value = Math.max(1, ratio);
      const [across, down] = [Math.max(1, Math.round((size.width * ratio) / haloCell.value)), Math.max(1, Math.round((size.height * ratio) / haloCell.value))];
      haloTarget.setSize(across, down);
      hidingTarget.setSize(across, down);
      dots.hiding.value.set(dots.hiding.value.x, 1 / across, 1 / down);
      camera.aspect = size.width / size.height;
    },

    setPixels(xyz, spread) {
      const count = xyz.length / 3;
      const current = pixelGeometry.getAttribute("position") as BufferAttribute | undefined;
      if (current && current.count === count) {
        (current.array as Float32Array).set(xyz);
        current.clearUpdateRanges();
        current.needsUpdate = true;
        fullUpload = true;
        const spreads = pixelGeometry.getAttribute("spread") as BufferAttribute;
        (spreads.array as Float32Array).set(spread.subarray(0, spreads.array.length));
        spreads.needsUpdate = true;
        return;
      }
      fullUpload = true;
      pixelGeometry.dispose();
      pixelGeometry = new BufferGeometry();
      const position = new BufferAttribute(new Float32Array(xyz), 3);
      position.setUsage(DynamicDrawUsage);
      // One list of colors, with a pixel to spare at either end, read three ways: each pixel's
      // own, and those of the pixels before and after it.
      const tints = new InterleavedBuffer(new Uint8Array((count + 2) * 3), 3);
      tints.setUsage(DynamicDrawUsage);
      const spreads = new Float32Array(count * SPREAD_SIZE);
      spreads.set(spread.subarray(0, spreads.length));
      pixelGeometry.setAttribute("position", position);
      pixelGeometry.setAttribute("tintBefore", new InterleavedBufferAttribute(tints, 3, 0, true));
      pixelGeometry.setAttribute("tint", new InterleavedBufferAttribute(tints, 3, 3, true));
      pixelGeometry.setAttribute("tintAfter", new InterleavedBufferAttribute(tints, 3, 6, true));
      pixelGeometry.setAttribute("spread", new BufferAttribute(spreads, SPREAD_SIZE));
      for (const points of [...passes, halos, washes]) points.geometry = pixelGeometry;
      anyFloods = false;
    },

    setFloods(floods) {
      const count = (pixelGeometry.getAttribute("position") as BufferAttribute | undefined)?.count ?? 0;
      const flags = new Float32Array(count);
      for (let i = 0; i < Math.min(count, floods.length); i++) flags[i] = floods[i];
      pixelGeometry.setAttribute("flood", new BufferAttribute(flags, 1));
      anyFloods = floods.some((f) => f > 0);
    },

    setMotion(items) {
      const beams = items.filter((m) => m.kind === "beam");
      while (beamMeshes.length < beams.length) {
        const mesh = new Mesh(
          beamCone,
          new ShaderMaterial({
            uniforms: { color: { value: new Color() }, strength: { value: 0 } },
            vertexShader: BEAM_VERTEX,
            fragmentShader: BEAM_FRAGMENT,
            transparent: true,
            depthWrite: false,
            side: DoubleSide,
          }),
        );
        mesh.frustumCulled = false;
        motionScene.add(mesh);
        beamMeshes.push(mesh);
      }
      const down = new Vector3(0, -1, 0);
      beamMeshes.forEach((mesh, i) => {
        const beam = beams[i];
        const dir = beam ? new Vector3(beam.dir.x, beam.dir.y, beam.dir.z) : null;
        mesh.visible = !!beam && !!dir && dir.lengthSq() > 0;
        if (!beam || !dir || !mesh.visible) return;
        const width = beam.length * spread(beam.angleDeg);
        mesh.position.set(beam.origin.x, beam.origin.y, beam.origin.z);
        mesh.quaternion.setFromUnitVectors(down, dir.normalize());
        mesh.scale.set(width, beam.length, width);
        mesh.material.uniforms.color.value.setRGB(beam.color[0] / 255, beam.color[1] / 255, beam.color[2] / 255);
        mesh.material.uniforms.strength.value = beam.lit ? BEAM_STRENGTH : UNLIT_BEAM;
      });
      const ends = items.flatMap((m) =>
        m.kind === "arm" ? [m.origin.x, m.origin.y, m.origin.z, m.origin.x + m.dir.x * m.length, m.origin.y + m.dir.y * m.length, m.origin.z + m.dir.z * m.length] : [],
      );
      arms.geometry.dispose();
      arms.geometry = new BufferGeometry();
      arms.geometry.setAttribute("position", new BufferAttribute(new Float32Array(ends), 3));
      anyMotion = items.length > 0;
    },

    updatePixels(start, xyz) {
      const position = pixelGeometry.getAttribute("position") as BufferAttribute | undefined;
      if (!position || start * 3 + xyz.length > position.array.length) return;
      (position.array as Float32Array).set(xyz, start * 3);
      if (!fullUpload) position.addUpdateRange(start * 3, xyz.length);
      position.needsUpdate = true;
    },

    setColors(rgb, live) {
      const tint = pixelGeometry.getAttribute("tint") as InterleavedBufferAttribute | undefined;
      if (!tint) return;
      const array = tint.data.array as Uint8Array;
      // After the spare pixel at the start.
      array.set(rgb.length > array.length - 6 ? rgb.subarray(0, array.length - 6) : rgb, 3);
      tint.data.clearUpdateRanges();
      tint.data.needsUpdate = true;
      lit = live;
      applyLook();
    },

    setLook(next) {
      look = next;
      applyLook();
    },

    setBackdrop(next) {
      const remove = () => {
        if (backdrop) {
          scene.remove(backdrop);
          disposeTree(backdrop);
        }
        if (backdropShadow) hidingScene.remove(backdropShadow);
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
        hidingScene.add(backdropShadow);
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
      glow = glowStep(options.glow) / GLOW_STEPS;
      applyLook();
      ground.visible = options.ground;
    },

    render(orbit, moving = false) {
      const eye = orbitEye(orbit);
      const { near, far } = clipRange(orbit);
      camera.near = near;
      camera.far = far;
      camera.position.set(eye.x, eye.y, eye.z);
      camera.lookAt(orbit.target.x, orbit.target.y, orbit.target.z);
      camera.updateProjectionMatrix();
      sky.position.copy(camera.position);
      sky.scale.setScalar(far * 0.9);
      dots.scale.value = pixelsPerUnit(size.height, size.ratio);
      dots.ratio.value = size.ratio;
      // The drawing's size in device pixels, as the renderer rounds it.
      dots.viewport.value.set(Math.floor(size.width * size.ratio), Math.floor(size.height * size.ratio));
      dots.still.value = moving ? 0 : 1;
      dots.depths.value.set(near, 1 / Math.log2(far / near));
      dots.project.value.set(camera.projectionMatrix.elements[10], camera.projectionMatrix.elements[14]);
      renderer.autoClear = true;
      // With nothing that could hide a light, none of them looks.
      const hiding = !!backdropShadow?.visible || (modelShadow.visible && modelShadow.children.length > 0);
      dots.hiding.value.x = hiding ? 1 : 0;
      if (hiding) {
        renderer.setRenderTarget(hidingTarget);
        renderer.render(hidingScene, camera);
      }
      // With nothing glowing, there are no halos to draw.
      const glowing = dots.glow.value > 0;
      if (glowing) {
        renderer.setRenderTarget(haloTarget);
        renderer.render(haloScene, camera);
      }
      renderer.setRenderTarget(null);
      renderer.render(scene, camera);
      renderer.autoClear = false;
      // Floods' washes over the scene, under the dots.
      if (anyFloods) renderer.render(washScene, camera);
      // Heads' beams and servos' arms, the same.
      if (anyMotion) renderer.render(motionScene, camera);
      // The lights go over the finished scene, with the depth buffer to themselves.
      renderer.clearDepth();
      renderer.render(lightScene, camera);
      fullUpload = false;
      if (glowing) haloOver.render(renderer);
      if (selectionBox.visible || gizmo.group.visible) {
        renderer.clearDepth();
        renderer.render(overlayScene, camera);
      }
    },

    dispose() {
      modelLoads++;
      disposeTree(scene);
      disposeTree(overlayScene);
      disposeTree(lightScene);
      disposeTree(haloScene);
      disposeTree(washScene);
      disposeTree(motionScene);
      beamCone.dispose();
      disposeTree(hidingScene);
      occluder.dispose();
      hidingTarget.dispose();
      haloTarget.dispose();
      haloOver.material.dispose();
      haloOver.dispose();
      renderer.renderLists.dispose();
      renderer.dispose();
      // Let go of the WebGL context now rather than whenever it's collected: browsers allow only
      // a few at once, and each switch to 3D makes a new one.
      renderer.forceContextLoss();
    },
  };
}
