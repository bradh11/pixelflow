// The settings the properties panel offers for each kind of generated prop, in plain words.
// Keys name the shape's own fields (a dot reaches into a nested one, like "wiring.start").

export type ShapeField =
  | { kind: "number"; key: string; label: string; integer?: boolean; min: number; max?: number; hint?: string }
  | { kind: "bool"; key: string; label: string; hint?: string }
  | { kind: "choice"; key: string; label: string; options: { value: string; label: string }[]; hint?: string }
  /** Whole numbers typed as a comma list, like an icicle drop pattern "3,4,5,4". */
  | { kind: "numbers"; key: string; label: string; min: number; max?: number; hint?: string };

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

const CORNERS: [string, string][] = [
  ["bottomLeft", "Bottom left"],
  ["bottomRight", "Bottom right"],
  ["topLeft", "Top left"],
  ["topRight", "Top right"],
];

/** The size and pixel settings for each kind of generated prop. */
export const SHAPE_FIELDS: Record<string, ShapeField[]> = {
  line: [COUNT("nodes", "Pixels"), SIZE("length", "Length")],
  arch: [COUNT("nodes", "Pixels"), SIZE("width", "Width"), SIZE("height", "Height")],
  circle: [COUNT("nodes", "Pixels"), SIZE("radius", "Radius")],
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
    SIZE("baseRadius", "Base radius"),
    SIZE("topRadius", "Top radius", 0),
  ],
  star: [COUNT("points", "Points", 2), COUNT("nodes", "Pixels"), SIZE("outerRadius", "Outer radius"), SIZE("innerRadius", "Inner radius")],
  candyCanes: [
    COUNT("canes", "Canes"),
    COUNT("nodesPerCane", "Pixels per cane"),
    SIZE("width", "Width", 0.01, "From the first cane's foot to the far side of the last"),
    NUMBER("skewDeg", "Lean (°)", -90, 90, "How far each cane leans; positive leans left"),
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
    SIZE("dropHeight", "Drop length", 0.01, "How far below the line the longest drop hangs"),
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

/** "3,4,5,4" as numbers, or null when it isn't a list of whole numbers within the bounds. */
export function parseNumbers(text: string, min: number, max = Infinity): number[] | null {
  const parts = text.split(",").map((s) => s.trim());
  if (parts.length === 0 || parts.some((p) => p === "")) return null;
  const nums = parts.map(Number);
  return nums.every((n) => Number.isInteger(n) && n >= min && n <= max) ? nums : null;
}
