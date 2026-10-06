import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import type { FaceDefinition, Prop, Region } from "../api/types";
import { newProp } from "./shows";
import {
  HIGHLIGHT,
  facePartColor,
  formatLine,
  formatRanges,
  highlightPixels,
  newSubmodel,
  parseLine,
  phonemeAt,
  regionNameProblem,
  regionNodes,
  targetKey,
  targetName,
  targetNodes,
  wordPhonemes,
} from "./submodels";

const lines = (rows: Extract<Region, { kind: "nodes" }>["lines"]): Region => ({ id: "r1", name: "Sub", kind: "nodes", lines: rows, layout: "horizontal", buffer: "default" });

const face = (def: Partial<FaceDefinition> = {}): Region & FaceDefinition => ({
  id: "f1",
  name: "Singer",
  kind: "face",
  mouths: { AI: [{ start: 0, end: 2 }], REST: [{ start: 2, end: 3 }] },
  eyesOpen: [{ start: 4, end: 6 }],
  eyesClosed: [{ start: 6, end: 7 }],
  outline: [{ start: 8, end: 10 }],
  ...def,
});

function arch(regions: Region[] = []): Prop {
  return { ...newProp("arch", emptyShow("x")), name: "Arch", regions };
}

describe("pixel lists", () => {
  it("reads and writes xLights-style lines", () => {
    const read = parseLine("1-10, 0,15,,20-12");
    expect(read).toEqual({ line: [{ first: 0, last: 9 }, null, { first: 14, last: 14 }, null, { first: 19, last: 11 }] });
    expect(formatLine("line" in read ? read.line : [])).toBe("1-10,0,15,0,20-12");
    expect(parseLine("  ")).toEqual({ line: [] });
  });

  it("explains what it can't read", () => {
    for (const bad of ["a", "1-x", "-3", "1.5", "1-2-3"]) {
      const read = parseLine(bad);
      expect("error" in read && read.error, bad).toMatch(/isn't a pixel number or range; use numbers from 1, like 1-10 or 15\./);
    }
  });

  it("writes face ranges as pixel numbers from 1", () => {
    expect(formatRanges([{ start: 0, end: 4 }, { start: 8, end: 9 }, { start: 5, end: 5 }])).toBe("1-4, 9");
  });
});

describe("which pixels a region lights", () => {
  it("follows the lines in order, each pixel once, and stops at the prop's end", () => {
    const region = lines([[{ first: 3, last: 1 }, null, { first: 8, last: 8 }], [{ first: 2, last: 4 }, { first: 50, last: 50 }]]);
    expect(regionNodes(region, 10)).toEqual([3, 2, 1, 8, 4]);
  });

  it("takes a rectangle's pixels by where they are on the prop", () => {
    // A 3 × 2 grid of pixels, 0–2 along the bottom row.
    const points = [0, 0, 1, 0, 2, 0, 0, 1, 1, 1, 2, 1];
    // 50% of 3 columns is 1.5, which rounds to column 2 for a start edge.
    const right: Region = { id: "r", name: "Right", kind: "subBuffer", x1: 50, y1: 0, x2: 100, y2: 100 };
    expect(regionNodes(right, 6, points)).toEqual([2, 5]);
    const top: Region = { ...right, x1: 0, y1: 60 };
    expect(regionNodes(top, 6, points)).toEqual([3, 4, 5]);
    expect(regionNodes(top, 6), "no positions yet").toEqual([]);
  });

  // Worked through xLights' SubModel::initSubbufferRange and Model::IsNodeInBufferRange: an
  // edge's percent scales to the prop's buffer cells, the start rounds, the end truncates, and
  // both ends are in. Same cases as pf-render's geometry tests.
  it.each([
    [10, 0, 50, [0, 5]],
    [10, 50, 100, [5, 9]],
    [10, 0, 33, [0, 3]],
    [10, 33, 66, [3, 6]],
    [10, 66, 100, [7, 9]],
    [7, 0, 50, [0, 3]],
    [7, 50, 100, [4, 6]],
    [7, 0, 33, [0, 2]],
    [7, 33, 66, [2, 4]],
    [7, 66, 100, [5, 6]],
    [10, 50, 0, [0, 5]],
  ])("a %i-wide grid, %i to %i percent across, takes columns %j", (width, x1, x2, [lo, hi]) => {
    // A width × 5 grid, wired row by row from the bottom left, spaced so its cells are square.
    const rows = 5;
    const points: number[] = [];
    for (let r = 0; r < rows; r++) for (let c = 0; c < width; c++) points.push((c * width) / (width - 1), (r * rows) / (rows - 1));
    const region: Region = { id: "w", name: "Window", kind: "subBuffer", x1, y1: 0, x2, y2: 50 };
    const expected: number[] = [];
    // Rows 0 to trunc(2.5) = 2.
    for (let r = 0; r <= 2; r++) for (let c = lo; c <= hi; c++) expected.push(r * width + c);
    expect(regionNodes(region, width * rows, points)).toEqual(expected);
  });

  it("lists every part of a face", () => {
    expect(regionNodes(face(), 20)).toEqual([0, 1, 2, 4, 5, 6, 8, 9]);
  });
});

describe("names", () => {
  it("must be given and different on each prop", () => {
    const prop = arch([lines([]), { ...face(), name: "Singer" }]);
    expect(regionNameProblem(prop, " ")).toBe("Give it a name.");
    expect(regionNameProblem(prop, "singer ")).toBe('"singer" is taken on Arch; names must be different on each prop.');
    expect(regionNameProblem(prop, "Sub", "r1")).toBeNull();
    expect(regionNameProblem(prop, "Left")).toBeNull();
  });

  it("new submodels take the first free name", () => {
    const prop = arch([{ ...lines([]), name: "Submodel 1" }]);
    expect(newSubmodel(prop)).toMatchObject({ name: "Submodel 2", kind: "nodes", lines: [[]], layout: "horizontal", buffer: "default" });
  });

  it("rows on submodels are named after the prop and the submodel", () => {
    const prop = arch([lines([])]);
    const show = { ...emptyShow("x"), props: [prop] };
    const target = { region: { prop: prop.id, region: "r1" } };
    expect(targetName(show, target)).toBe("Arch / Sub");
    expect(targetName(show, { region: { prop: prop.id, region: "gone" } })).toBe("Missing submodel");
    expect(targetKey(target)).toBe(`${prop.id}/r1`);
    expect(targetKey({ prop: prop.id })).toBe(prop.id);
  });

  it("a group's members light in their listed order, submodels mixed in", () => {
    const prop = arch([lines([[{ first: 0, last: 2 }]]), { ...lines([[{ first: 4, last: 5 }]]), id: "r2" }]);
    const other = { ...arch(), id: "b" };
    const members = [{ prop: prop.id, region: "r1" }, "b", { prop: prop.id, region: "r2" }, { prop: prop.id, region: "r1" }];
    const show = { ...emptyShow("x"), props: [prop, other], groups: [{ id: "g", name: "G", members }] };
    const lit = targetNodes(show, { group: "g" }, () => 50, () => []);
    expect(lit).toEqual([
      { prop: prop.id, nodes: [0, 1, 2] },
      { prop: "b", nodes: "all" },
      { prop: prop.id, nodes: [4, 5] },
    ]);
    // A whole prop after one of its submodels adds only the pixels not already in.
    show.groups[0].members = [{ prop: prop.id, region: "r1" }, prop.id];
    const whole = targetNodes(show, { group: "g" }, () => 5, () => []);
    expect(whole).toEqual([
      { prop: prop.id, nodes: [0, 1, 2] },
      { prop: prop.id, nodes: [3, 4] },
    ]);
  });
});

describe("singing", () => {
  it("guesses mouth shapes from a word's letters", () => {
    expect(wordPhonemes("Moon")).toEqual(["MBP", "U", "ETC"]);
    expect(wordPhonemes("Happy!")).toEqual(["ETC", "AI", "MBP", "E"]);
    expect(wordPhonemes("we've")).toEqual(["WQ", "E", "FV", "E"]);
  });

  it("keeps the mouth at rest on a track without lyrics, as xLights does", () => {
    for (const kind of ["beats", "bars", "sections", "custom"]) {
      expect(phonemeAt({ kind, marks: [{ startMs: 0, endMs: 1000, label: "Chorus" }] }, 500), kind).toBe("REST");
    }
    expect(phonemeAt({ kind: "lyrics", marks: [{ startMs: 0, endMs: 1000, label: "Oh" }] }, 100)).toBe("O");
  });

  it("reads the phoneme under the playhead, resting between marks", () => {
    const phonemes = { kind: "phonemes", marks: [{ startMs: 100, endMs: 200, label: "AI" }, { startMs: 200, endMs: 300, label: "etc" }] };
    expect(phonemeAt(phonemes, 150)).toBe("AI");
    expect(phonemeAt(phonemes, 250)).toBe("ETC");
    expect(phonemeAt(phonemes, 300)).toBe("REST");
    expect(phonemeAt(undefined, 150)).toBe("REST");
    const words = { kind: "words", marks: [{ startMs: 1000, endMs: 1300, label: "moon" }] };
    expect([1000, 1100, 1299].map((t) => phonemeAt(words, t))).toEqual(["MBP", "U", "ETC"]);
  });

  it("colors parts from the face, or from the palette (mouth, eyes, outline)", () => {
    const own = face({ colors: { mouths: { AI: "#ff0000" }, eyesOpen: "#0000ff" } });
    expect(facePartColor(own, "mouth", "AI", false, true, [])).toBe("#ff0000");
    expect(facePartColor(own, "outline", "AI", false, true, [])).toBe("#ffffff");
    expect(facePartColor(own, "eyes", "AI", false, false, ["#111111", "#222222"])).toBe("#222222");
    expect(facePartColor(own, "outline", "AI", false, false, ["#111111", "#222222"]), "a short palette repeats its last color").toBe("#222222");
    expect(facePartColor(face(), "mouth", "AI", false, true, ["#333333"]), "no colors of its own: the palette").toBe("#333333");
  });
});

describe("highlighting on the canvas", () => {
  const points = Array.from({ length: 12 }, (_, n) => [n, 0]).flat();

  it("draws a submodel's pixels bright", () => {
    const { points: at, rgb } = highlightPixels(lines([[{ first: 2, last: 3 }]]), 12, points, null);
    expect(at).toEqual([2, 0, 3, 0]);
    const bright = [1, 3, 5].map((i) => parseInt(HIGHLIGHT.slice(i, i + 2), 16));
    expect([...rgb]).toEqual([...bright, ...bright]);
  });

  it("shows a face's mouth shape with open eyes and the outline", () => {
    const { points: at, rgb } = highlightPixels(face({ colors: { mouths: { AI: "#ff0000" } } }), 12, points, "AI");
    expect(at.filter((_, i) => i % 2 === 0)).toEqual([0, 1, 2, 4, 5, 6, 8, 9]);
    const colors = Array.from({ length: rgb.length / 3 }, (_, i) => [...rgb.slice(i * 3, i * 3 + 3)].join(","));
    expect(colors).toEqual(["255,0,0", "255,0,0", "0,0,0", "255,255,255", "255,255,255", "0,0,0", "255,255,255", "255,255,255"]);
  });
});
