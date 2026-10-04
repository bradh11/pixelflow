import type { Controller, Port, Prop, ShapeSource, Show } from "../api/types";

export type PropKind = "line" | "arch" | "circle" | "matrix" | "tree" | "star";

export const PROP_KINDS: { kind: PropKind; label: string }[] = [
  { kind: "arch", label: "Arch" },
  { kind: "line", label: "Line / string" },
  { kind: "matrix", label: "Matrix" },
  { kind: "tree", label: "Mega tree" },
  { kind: "star", label: "Star" },
  { kind: "circle", label: "Circle / wreath" },
];

const DEFAULT_SHAPES: Record<PropKind, ShapeSource> = {
  line: { source: "generator", type: "line", nodes: 50, length: 5 },
  arch: { source: "generator", type: "arch", nodes: 50, width: 4, height: 2 },
  circle: { source: "generator", type: "circle", nodes: 50, radius: 1 },
  matrix: {
    source: "generator",
    type: "matrix",
    columns: 32,
    rows: 16,
    width: 4,
    height: 2,
    wiring: { start: "bottomLeft", orientation: "horizontal", serpentine: true },
  },
  tree: {
    source: "generator",
    type: "tree",
    strings: 16,
    nodesPerString: 50,
    height: 5,
    baseRadius: 1.5,
    topRadius: 0.1,
    serpentine: true,
  },
  star: { source: "generator", type: "star", points: 5, nodes: 100, outerRadius: 1, innerRadius: 0.4 },
};

const KIND_NAMES: Record<PropKind, string> = {
  line: "Line",
  arch: "Arch",
  circle: "Circle",
  matrix: "Matrix",
  tree: "Mega Tree",
  star: "Star",
};

/** The first "Base N" name not already used. */
export function uniqueName(base: string, taken: string[]): string {
  const used = new Set(taken);
  for (let n = 1; ; n++) {
    const name = `${base} ${n}`;
    if (!used.has(name)) return name;
  }
}

/** A new prop of the given kind with sensible defaults and a unique name. */
export function newProp(kind: PropKind, show: Show): Prop {
  return {
    id: crypto.randomUUID(),
    name: uniqueName(KIND_NAMES[kind], show.props.map((p) => p.name)),
    shape: structuredClone(DEFAULT_SHAPES[kind]),
    transform: { position: { x: 0, y: 0, z: 0 }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } },
    colorOrder: "RGB",
    regions: [],
    tags: [],
  };
}

export function newPort(number: number): Port {
  return { number, maxPixels: null, brightness: 100, gamma: 1, slots: [] };
}

/** A new controller with `portCount` empty ports. */
export function newController(
  name: string,
  address: string,
  protocol: "ddp" | "sacn",
  portCount: number,
): Controller {
  return {
    id: crypto.randomUUID(),
    name,
    address,
    adapter: "generic",
    protocol:
      protocol === "ddp"
        ? { type: "ddp" }
        : { type: "sacn", startUniverse: null, universeSize: 510, allowPixelStraddle: false, multicast: false },
    ports: Array.from({ length: portCount }, (_, i) => newPort(i + 1)),
  };
}

/** Pixel count of a shape (mirrors the engine's node_count). */
export function nodeCount(shape: ShapeSource): number {
  if (shape.source === "measured") return shape.points.length;
  switch (shape.type) {
    case "line":
    case "arch":
    case "circle":
    case "star":
      return shape.nodes;
    case "matrix":
      return shape.columns * shape.rows;
    case "tree":
      return shape.strings * shape.nodesPerString;
    case "customGrid":
      return shape.cells.reduce((max, c) => Math.max(max, c), 0);
  }
}

/** Human label for a prop's shape. */
export function shapeLabel(shape: ShapeSource): string {
  if (shape.source === "measured") return "Measured";
  const labels: Record<string, string> = {
    line: "Line",
    arch: "Arch",
    circle: "Circle",
    matrix: "Matrix",
    tree: "Tree",
    star: "Star",
    customGrid: "Custom grid",
  };
  return labels[shape.type] ?? shape.type;
}

export function channelsPerPixel(prop: Prop): number {
  return prop.colorOrder === "RGBW" || prop.colorOrder === "GRBW" ? 4 : 3;
}
