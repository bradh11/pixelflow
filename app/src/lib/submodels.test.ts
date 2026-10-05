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
    const right: Region = { id: "r", name: "Right", kind: "subBuffer", x1: 50, y1: 0, x2: 100, y2: 100 };
    expect(regionNodes(right, 6, points)).toEqual([1, 2, 4, 5]);
    const top: Region = { ...right, x1: 0, y1: 60 };
    expect(regionNodes(top, 6, points)).toEqual([3, 4, 5]);
    expect(regionNodes(top, 6), "no positions yet").toEqual([]);
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

  it("a group's submodels light only their pixels", () => {
    const prop = arch([lines([[{ first: 0, last: 2 }]])]);
    const other = { ...arch(), id: "b" };
    const show = { ...emptyShow("x"), props: [prop, other], groups: [{ id: "g", name: "G", members: ["b"], submodels: [{ prop: prop.id, region: "r1" }] }] };
    const lit = targetNodes(show, { group: "g" }, () => 50, () => []);
    expect([...lit]).toEqual([
      ["b", "all"],
      [prop.id, [0, 1, 2]],
    ]);
  });
});

describe("singing", () => {
  it("guesses mouth shapes from a word's letters", () => {
    expect(wordPhonemes("Moon")).toEqual(["MBP", "U", "ETC"]);
    expect(wordPhonemes("Happy!")).toEqual(["ETC", "AI", "MBP", "E"]);
    expect(wordPhonemes("we've")).toEqual(["WQ", "E", "FV", "E"]);
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
