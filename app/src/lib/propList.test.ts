import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { emptyShow } from "../api/memory";
import type { Prop, Show } from "../api/types";
import { listedProps, rangeSelect, wiredProps } from "./propList";
import { newProp } from "./shows";

function show(): { show: Show; pixels: Map<string, number> } {
  const s = demoShow();
  return { show: s, pixels: new Map([[s.props[0].id, 50], [s.props[1].id, 800], [s.props[2].id, 512], [s.props[3].id, 60]]) };
}
const names = (props: Prop[]) => props.map((p) => p.name);

describe("listedProps", () => {
  it("keeps layout order, or sorts by name, pixels, type, or wiring", () => {
    const { show: s, pixels } = show();
    const wired = wiredProps(s);
    const list = (sort: Parameters<typeof listedProps>[1]["sort"]) => names(listedProps(s.props, { sort, query: "", unwiredOnly: false }, pixels, wired));
    expect(list("layout")).toEqual(["Garage Arch", "Mega Tree", "Window Matrix", "Porch Star"]);
    expect(list("name")).toEqual(["Garage Arch", "Mega Tree", "Porch Star", "Window Matrix"]);
    expect(list("pixels")).toEqual(["Mega Tree", "Window Matrix", "Porch Star", "Garage Arch"]);
    expect(list("type")).toEqual(["Garage Arch", "Window Matrix", "Porch Star", "Mega Tree"]);
    // Porch Star is the one prop not wired.
    expect(list("unwired")[0]).toBe("Porch Star");
  });

  it("finds props by any part of their name or type, ignoring case", () => {
    const { show: s, pixels } = show();
    const find = (query: string) => names(listedProps(s.props, { sort: "layout", query, unwiredOnly: false }, pixels, wiredProps(s)));
    expect(find("tree")).toEqual(["Mega Tree"]);
    expect(find("  ARCH ")).toEqual(["Garage Arch"]);
    expect(find("matrix")).toEqual(["Window Matrix"]);
    expect(find("nothing like it")).toEqual([]);
  });

  it("can show only the props not wired yet", () => {
    const { show: s, pixels } = show();
    expect(names(listedProps(s.props, { sort: "layout", query: "", unwiredOnly: true }, pixels, wiredProps(s)))).toEqual(["Porch Star"]);
  });

  it("sorts names the way people count (Arch 2 before Arch 10)", () => {
    const s = emptyShow("x");
    s.props = [10, 2, 1].map((n) => ({ ...newProp("arch", s), name: `Arch ${n}` }));
    expect(names(listedProps(s.props, { sort: "name", query: "", unwiredOnly: false }, new Map(), new Set()))).toEqual(["Arch 1", "Arch 2", "Arch 10"]);
  });

  it("stays quick with thousands of props", () => {
    const s = emptyShow("x");
    const base = newProp("line", s);
    s.props = Array.from({ length: 5000 }, (_, i) => ({ ...base, id: `p${i}`, name: `Line ${5000 - i}` }));
    const started = performance.now();
    const listed = listedProps(s.props, { sort: "name", query: "line 4", unwiredOnly: false }, new Map(), new Set());
    expect(performance.now() - started).toBeLessThan(200);
    expect(listed[0].name).toBe("Line 4");
  });
});

describe("rangeSelect", () => {
  const order = ["a", "b", "c", "d", "e"];
  it("selects from the anchor to the clicked one, in list order", () => {
    expect(rangeSelect(order, "b", "d")).toEqual(["b", "c", "d"]);
    expect(rangeSelect(order, "d", "b")).toEqual(["b", "c", "d"]);
  });
  it("selects just the clicked one when the anchor isn't listed", () => {
    expect(rangeSelect(order, "zz", "c")).toEqual(["c"]);
  });
});

describe("wiredProps", () => {
  it("is every prop on some port", () => {
    const s = demoShow();
    expect([...wiredProps(s)].sort()).toEqual([s.props[0].id, s.props[1].id, s.props[2].id].sort());
  });
});
