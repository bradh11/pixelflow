import { describe, expect, it } from "vitest";
import type { VendorInspection, VendorItem } from "../api/types";
import { autoMapping, itemTree, mappingStats, withLoaded } from "./vendorMapping";

function item(name: string, effects: number, parent: string | null = null): VendorItem {
  return { name, label: parent ? name.slice(parent.length + 1) : name, parent, kind: parent ? "submodel" : "model", type: "other", displayAs: null, effects, pixels: 0 };
}

const items = [item("Arch", 10), item("Tree", 30), item("Tree/Star", 5, "Tree"), item("Flood", 0, null), item("Arch/Left", 20, "Arch")];

describe("vendor mappings", () => {
  it("counts what's mapped by effects, leaving out items with none", () => {
    expect(mappingStats(items, { items: { Tree: ["Mega Tree"], "Tree/Star": [], Flood: ["Porch"] } })).toEqual({
      mapped: 1,
      total: 4,
      effects: 65,
      mappedEffects: 30,
      percent: 46,
    });
    expect(mappingStats([], { items: {} }).percent).toBe(100);
  });

  it("applies confident and saved suggestions only", () => {
    const inspection = {
      suggestions: [
        { item: "Tree", targets: ["Mega Tree"], confidence: 0.7, reason: "type" },
        { item: "Arch", targets: ["Garage Arch"], confidence: 0.49, reason: "type" },
        { item: "Flood", targets: [], confidence: 1, reason: "saved" },
      ],
    } as unknown as VendorInspection;
    expect(autoMapping(inspection)).toEqual({ items: { Tree: ["Mega Tree"], Flood: [] } });
  });

  it("lays a loaded mapping over the current one for this sequence's items", () => {
    const loaded = withLoaded(items, { items: { Tree: ["Mega Tree"], Arch: ["Arch"] } }, { items: { Arch: ["Garage Arch", "Porch"], Other: ["Tree"] } });
    expect(loaded).toEqual({ mapping: { items: { Tree: ["Mega Tree"], Arch: ["Garage Arch", "Porch"] } }, used: 1, unused: 1 });
  });

  it("lists models busiest first (their parts counted), each followed by its parts", () => {
    expect(itemTree(items).map(({ item, children }) => [item.name, children.map((c) => c.name)])).toEqual([
      ["Tree", ["Tree/Star"]],
      ["Arch", ["Arch/Left"]],
      ["Flood", []],
    ]);
  });
});
