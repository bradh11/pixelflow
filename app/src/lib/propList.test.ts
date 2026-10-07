import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { emptyShow } from "../api/memory";
import type { Prop, Show } from "../api/types";
import { listedProps, needsWiring, rangeSelect, wiringStatuses } from "./propList";
import { newProp } from "./shows";

function show(): { show: Show; pixels: Map<string, number> } {
  const s = demoShow();
  return { show: s, pixels: new Map([[s.props[0].id, 50], [s.props[1].id, 800], [s.props[2].id, 512], [s.props[3].id, 60]]) };
}
const names = (props: Prop[]) => props.map((p) => p.name);

describe("listedProps", () => {
  it("keeps layout order, or sorts by name, pixels, type, or wiring", () => {
    const { show: s, pixels } = show();
    const wired = wiringStatuses(s, pixels);
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
    const find = (query: string) => names(listedProps(s.props, { sort: "layout", query, unwiredOnly: false }, pixels, wiringStatuses(s, pixels)));
    expect(find("tree")).toEqual(["Mega Tree"]);
    expect(find("  ARCH ")).toEqual(["Garage Arch"]);
    expect(find("matrix")).toEqual(["Window Matrix"]);
    expect(find("nothing like it")).toEqual([]);
  });

  it("can show only the props not wired yet", () => {
    const { show: s, pixels } = show();
    expect(names(listedProps(s.props, { sort: "layout", query: "", unwiredOnly: true }, pixels, wiringStatuses(s, pixels)))).toEqual(["Porch Star"]);
  });

  it("sorts names the way people count (Arch 2 before Arch 10)", () => {
    const s = emptyShow("x");
    s.props = [10, 2, 1].map((n) => ({ ...newProp("arch", s), name: `Arch ${n}` }));
    expect(names(listedProps(s.props, { sort: "name", query: "", unwiredOnly: false }, new Map(), new Map()))).toEqual(["Arch 1", "Arch 2", "Arch 10"]);
  });

  it("stays quick with thousands of props", () => {
    const s = emptyShow("x");
    const base = newProp("line", s);
    s.props = Array.from({ length: 5000 }, (_, i) => ({ ...base, id: `p${i}`, name: `Line ${5000 - i}` }));
    const started = performance.now();
    const listed = listedProps(s.props, { sort: "name", query: "line 4", unwiredOnly: false }, new Map(), new Map());
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

describe("wiringStatuses", () => {
  it("says which props are wired, partly wired, or not wired", () => {
    const { show: s, pixels } = show();
    // Only the first 400 of the mega tree's 800 pixels are on a port.
    s.controllers[0].ports[1].slots[0].segment = { start: 0, end: 400 };
    const status = wiringStatuses(s, pixels);
    expect(s.props.map((p) => status.get(p.id))).toEqual(["wired", "partial", "wired", "unwired"]);
    expect(["unwired", "partial", "twice", "wired"].map((w) => needsWiring(w as never))).toEqual([true, true, false, false]);
    // "Not wired" includes the partly wired, and they sort first after the unwired.
    expect(names(listedProps(s.props, { sort: "layout", query: "", unwiredOnly: true }, pixels, status))).toEqual(["Mega Tree", "Porch Star"]);
    expect(names(listedProps(s.props, { sort: "unwired", query: "", unwiredOnly: false }, pixels, status))).toEqual(["Porch Star", "Mega Tree", "Garage Arch", "Window Matrix"]);
  });
});
