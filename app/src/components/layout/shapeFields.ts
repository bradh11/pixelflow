// The settings the properties panel offers for each kind of generated prop, in plain words.
// Keys name the shape's own fields (a dot reaches into a nested one, like "wiring.start").

export type ShapeField = (
  | { kind: "number"; key: string; label: string; integer?: boolean; min: number; max?: number; hint?: string }
  | { kind: "bool"; key: string; label: string; hint?: string }
  | { kind: "choice"; key: string; label: string; options: { value: string; label: string }[]; hint?: string }
  /** Whole numbers typed as a comma list, like an icicle drop pattern "3,4,5,4"; `allowEmpty` lets it be cleared. */
  | { kind: "numbers"; key: string; label: string; min: number; max?: number; hint?: string; allowEmpty?: boolean }
) & {
  /** Shown only when this says so for the shape (a setting that only matters with another one). */
  showIf?: (shape: Record<string, unknown>) => boolean;
};

/** `field`, shown only when `when` holds for the shape. */
const only = (when: (shape: Record<string, unknown>) => boolean, field: ShapeField): ShapeField => ({ ...field, showIf: when });
const hasLayers = (shape: Record<string, unknown>) => Array.isArray(shape.layers) && shape.layers.length > 0;
/** Circles and stars nest only with two layers or more; one is just the plain ring or star. */
const severalLayers = (shape: Record<string, unknown>) => Array.isArray(shape.layers) && shape.layers.length > 1;
const noLayers = (shape: Record<string, unknown>) => !hasLayers(shape);

export const COUNT = (key: string, label: string, min = 1, hint?: string): ShapeField => ({ kind: "number", key, label, integer: true, min, hint });
export const SIZE = (key: string, label: string, min = 0.01, hint?: string): ShapeField => ({ kind: "number", key, label, min, hint });
export const NUMBER = (key: string, label: string, min: number, max: number, hint?: string): ShapeField => ({ kind: "number", key, label, min, max, hint });
export const BOOL = (key: string, label: string, hint?: string): ShapeField => ({ kind: "bool", key, label, hint });
export const CHOICE = (key: string, label: string, options: [string, string][], hint?: string): ShapeField => ({
  kind: "choice",
  key,
  label,
  options: options.map(([value, l]) => ({ value, label: l })),
  hint,
});

/** A list of pixels per layer (innermost first) that may be left empty for one layer. */
const LAYERS = (label: string, hint: string): ShapeField => ({ kind: "numbers", key: "layers", label, min: 1, max: 1_000_000, allowEmpty: true, hint });
const INNER_SIZE = (label: string, hint: string): ShapeField => ({ kind: "number", key: "innerPercent", label, integer: true, min: 0, max: 100, hint });

const CORNERS: [string, string][] = [
  ["bottomLeft", "Bottom left"],
  ["bottomRight", "Bottom right"],
  ["topLeft", "Top left"],
  ["topRight", "Top right"],
];

const STRAND_STYLES: [string, string][] = [
  ["zigZag", "Zig-zag (every other one runs back)"],
  ["noZigZag", "All run the same way"],
  ["alternatePixel", "Every other pixel out, the rest on the way back"],
];

/** The size and pixel settings for each kind of generated prop. */
export const SHAPE_FIELDS: Record<string, ShapeField[]> = {
  line: [COUNT("nodes", "Pixels"), SIZE("length", "Length")],
  arch: [
    COUNT("nodes", "Pixels", 1, "Pixels on each arch (on a layered arch, on all its layers together)"),
    only(noLayers, COUNT("arches", "Arches", 1, "Arches in a row, one after another on the same string")),
    SIZE("width", "Width", 0.01, "Between an arch's two feet"),
    SIZE("height", "Height", 0.01, "From the feet to the top"),
    NUMBER("arc", "Curve (°)", 1, 180, "How much of a circle each arch is: 180 is a half circle, less is a flatter arch"),
    NUMBER("skewDeg", "Lean (°)", -180, 180, "How far the arches lean; positive leans left"),
    only(noLayers, SIZE("gap", "Gap between arches", 0, "From one arch's right foot to the next one's left foot")),
    {
      kind: "numbers",
      key: "layers",
      label: "Layers (pixels each, inside first)",
      min: 1,
      max: 1_000_000,
      allowEmpty: true,
      hint: "For an arch made of arches inside each other: the pixels on each, innermost first, like 20,30,40. Leave it empty for plain arches",
    },
    only(hasLayers, { kind: "number", key: "hollow", label: "Innermost layer (%)", integer: true, min: 0, max: 100, hint: "The innermost arch's size, in percent of the outermost" }),
    BOOL("startRight", "First pixel on the right (the data comes in there)"),
    only(hasLayers, BOOL("startInside", "Starts on the innermost layer")),
    only(hasLayers, BOOL("zigZag", "Every other layer runs back the other way")),
  ],
  circle: [
    COUNT("nodes", "Pixels", 1, "All the pixels, on all the rings together"),
    SIZE("radius", "Radius", 0.01, "Of the outermost ring"),
    LAYERS("Rings (pixels each, inside first)", "For rings inside each other: the pixels on each ring, innermost first, like 10,20,30. Leave it empty for one ring"),
    only(severalLayers, INNER_SIZE("Innermost ring (%)", "The innermost ring's size, in percent of the outermost")),
    BOOL("startAtBottom", "Starts at the bottom"),
    BOOL("counterClockwise", "Goes round counter-clockwise"),
    only(severalLayers, BOOL("startInside", "Starts on the innermost ring")),
  ],
  matrix: [
    COUNT("columns", "Columns"),
    COUNT("rows", "Rows"),
    SIZE("width", "Width"),
    SIZE("height", "Height"),
    CHOICE("wiring.start", "First pixel", CORNERS, "The corner where the data comes in"),
    CHOICE(
      "wiring.orientation",
      "Strings run",
      [
        ["horizontal", "Across (rows)"],
        ["vertical", "Up and down (columns)"],
      ],
      "Which way each string of pixels runs",
    ),
    BOOL("wiring.serpentine", "Zig-zag (each string runs back the other way)"),
  ],
  tree: [
    COUNT("strings", "Strings"),
    COUNT("nodesPerString", "Pixels per string"),
    SIZE("height", "Height"),
    SIZE("baseRadius", "Base radius", 0.01, "For a flat or ribbon tree, half its width at the bottom"),
    SIZE("topRadius", "Top radius", 0, "For a flat or ribbon tree, half its width at the top"),
    NUMBER("degrees", "Goes round (°)", 1, 360, "A round tree: 360 all the way round, 180 a half tree against a wall"),
    NUMBER("startAngle", "First string at (°)", -360, 360, "A round tree: how far round from the front the first string stands"),
    CHOICE("style", "Style", [
      ["round", "Round (a cone)"],
      ["flat", "Flat (strings fanned out)"],
      ["ribbon", "Ribbon (fanned, strings the same length)"],
    ]),
    CHOICE("start", "First pixel", CORNERS, "Where the data comes in: a top corner runs the first string down, a right one goes round the other way"),
    BOOL("serpentine", "Zig-zag (every other string runs back)"),
    only(
      (shape) => shape.serpentine === true,
      COUNT(
        "strandsPerString",
        "Zig-zag restarts every",
        0,
        "For strings folded up and down a few times: how many strands each string makes. 0 is one long zig-zag; 1 means no zig-zag, and an even number is the same as 0",
      ),
    ),
    BOOL("alternateNodes", "Pixels go up every other spot and come back down"),
    only(
      (shape) => (shape.style ?? "round") === "round",
      NUMBER("spiralRotations", "Spiral turns", -100, 100, "How many times the strings wind round the tree on the way up; 0 runs them straight up"),
    ),
  ],
  star: [
    COUNT("points", "Points", 2),
    COUNT("nodes", "Pixels", 1, "All the pixels, on all the outlines together"),
    SIZE("outerRadius", "Outer radius", 0.01, "Out to the tips"),
    SIZE("innerRadius", "Inner radius", 0.01, "Out to the corners between the tips"),
    CHOICE(
      "start",
      "First pixel",
      [
        ["top", "Top tip"],
        ["bottom", "Bottom, between the legs"],
        ["leftLeg", "Bottom left tip"],
        ["rightLeg", "Bottom right tip"],
      ],
      "Where the data comes in. Starting at the bottom turns a star with an even number of points so a corner is there",
    ),
    BOOL("counterClockwise", "Goes round counter-clockwise"),
    LAYERS("Layers (pixels each, inside first)", "For stars inside each other: the pixels on each, innermost first, like 20,40. Leave it empty for one star"),
    only(severalLayers, INNER_SIZE("Innermost star (%)", "The innermost star's size, in percent of the outermost")),
    only(severalLayers, BOOL("startInside", "Starts on the innermost star")),
  ],
  candyCanes: [
    COUNT("canes", "Canes"),
    COUNT("nodesPerCane", "Pixels per cane"),
    SIZE("width", "Width", 0.01, "From the first cane's foot to the far side of the last"),
    NUMBER("skewDeg", "Lean (°)", -90, 90, "How far each cane leans; positive leans left"),
    BOOL("startRight", "First cane on the right (the data comes in there)"),
    SIZE("height", "Cane size (×)", 0.01, "Makes the canes taller and their hooks wider; 1 is the usual size"),
    SIZE("caneHeight", "Cane height (×)", 0.01, "Makes the canes taller without widening their hooks; 1 is the usual height"),
    BOOL("reverse", "Hooks point left"),
    BOOL("sticks", "Straight sticks (no hooks)"),
    BOOL("alternateNodes", "Pixels go up every other spot and come back down"),
  ],
  icicles: [
    COUNT("strings", "Strings"),
    COUNT("lightsPerString", "Pixels per string"),
    SIZE("width", "Width"),
    NUMBER("dropHeight", "Drop length", -1000, 1000, "How far below the line the longest drop hangs; below zero, the drops stand up instead"),
    {
      kind: "numbers",
      key: "drops",
      label: "Drop pattern",
      min: 0,
      max: 1000,
      hint: "Pixels in each drop, repeating along the line, like 3,4,5,4 (a 0 leaves a gap)",
    },
    BOOL("alternateNodes", "Pixels go down every other spot and come back up"),
  ],
  windowFrame: [
    COUNT("top", "Pixels across the top", 0),
    COUNT("sides", "Pixels up each side", 0),
    COUNT("bottom", "Pixels across the bottom", 0),
    SIZE("width", "Width"),
    SIZE("height", "Height"),
    CHOICE("start", "First pixel", CORNERS, "The corner where the data comes in"),
    BOOL("counterClockwise", "Goes round counter-clockwise"),
  ],
  wreath: [
    COUNT("nodes", "Pixels"),
    SIZE("radius", "Radius"),
    BOOL("startAtBottom", "Starts at the bottom"),
    BOOL("counterClockwise", "Goes round counter-clockwise"),
  ],
  spinner: [
    COUNT("arms", "Arms"),
    COUNT("nodesPerArm", "Pixels per arm"),
    SIZE("radius", "Radius", 0.01, "From the middle to the outermost pixel"),
    { kind: "number", key: "hollow", label: "Hollow middle (%)", integer: true, min: 0, max: 100, hint: "How much of the middle has no pixels" },
    NUMBER("startAngle", "First arm turned (°)", -360, 360, "How far the first arm is turned counter-clockwise from pointing straight down"),
    NUMBER("arc", "Arms spread over (°)", 1, 360, "360 spreads the arms all the way round"),
    BOOL("fromCenter", "Each arm's pixels start in the middle"),
    BOOL("zigZag", "Every other arm runs back the other way"),
    BOOL("alternate", "Pixels go out every other spot and come back in"),
    BOOL("clockwise", "Arms follow each other clockwise"),
  ],
  sphere: [
    COUNT("columns", "Strands around"),
    COUNT("rows", "Pixels per strand"),
    SIZE("radius", "Radius"),
    NUMBER("degrees", "Goes round (°)", 1, 360, "360 goes all the way round; less leaves a gap at the back"),
    NUMBER("startLatitude", "Lowest pixels (latitude °)", -90, 90, "-90 is the bottom of the globe"),
    NUMBER("endLatitude", "Highest pixels (latitude °)", -90, 90, "90 is the top of the globe"),
    CHOICE("start", "First pixel", CORNERS, "Which side the first strand is on, and whether it starts at the bottom or the top"),
    CHOICE("strandStyle", "Strands", STRAND_STYLES),
  ],
  cube: [
    COUNT("width", "Pixels across"),
    COUNT("height", "Pixels up"),
    COUNT("depth", "Pixels deep"),
    SIZE("spacing", "Space between pixels"),
    CHOICE(
      "start",
      "First pixel",
      [
        ["frontBottomLeft", "Front bottom left"],
        ["frontBottomRight", "Front bottom right"],
        ["frontTopLeft", "Front top left"],
        ["frontTopRight", "Front top right"],
        ["backBottomLeft", "Back bottom left"],
        ["backBottomRight", "Back bottom right"],
        ["backTopLeft", "Back top left"],
        ["backTopRight", "Back top right"],
      ],
      "The corner where the data comes in. For the across and stacked styles, xLights starts a \"back top\" cube at the front, and so does PixelFlow",
    ),
    CHOICE(
      "style",
      "Strands run",
      [
        ["verticalFrontBack", "Up and down, layers front to back"],
        ["verticalLeftRight", "Up and down, layers left to right"],
        ["horizontalFrontBack", "Across, layers front to back"],
        ["horizontalLeftRight", "Across, layers left to right"],
        ["stackedFrontBack", "Across, layers stacked bottom to top (front to back)"],
        ["stackedLeftRight", "Across, layers stacked bottom to top (left to right)"],
      ],
    ),
    CHOICE("strandStyle", "Strands", STRAND_STYLES),
    BOOL("strandPerLayer", "Each layer starts on the same side (instead of where the last one ended)"),
  ],
};

/** The value at a dotted `key` in `obj` (undefined when a part is missing). */
export function fieldValue(obj: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>((o, k) => (o && typeof o === "object" ? (o as Record<string, unknown>)[k] : undefined), obj);
}

/** A copy of `obj` with the dotted `key` set to `value`; a missing nested object starts from `defaults`. */
export function withField<T>(obj: T, key: string, value: unknown, defaults: Record<string, unknown> = {}): T {
  const [head, ...rest] = key.split(".");
  const o = obj as Record<string, unknown>;
  if (rest.length === 0) return { ...o, [head]: value } as T;
  const inner = (o[head] as Record<string, unknown> | undefined) ?? (defaults[head] as Record<string, unknown> | undefined) ?? {};
  return { ...o, [head]: withField(inner, rest.join("."), value) } as T;
}

/** "3,4,5,4" as numbers, or null when it isn't a list of whole numbers within the bounds with at least one not 0 (with `allowEmpty`, blank reads as no numbers). */
export function parseNumbers(text: string, min: number, max = Infinity, allowEmpty = false): number[] | null {
  if (allowEmpty && text.trim() === "") return [];
  const parts = text.split(",").map((s) => s.trim());
  if (parts.length === 0 || parts.some((p) => p === "")) return null;
  const nums = parts.map(Number);
  return nums.every((n) => Number.isInteger(n) && n >= min && n <= max) && nums.some((n) => n !== 0) ? nums : null;
}
