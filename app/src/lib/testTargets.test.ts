import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { sampleShow } from "../api/sampleShow";
import { colorName, describeUse, isDemoShow, sendingSummary, testDestinations } from "./testTargets";

async function demo() {
  const backend = new MemoryBackend(demoShow());
  const { show, channelMap } = await backend.getSnapshot();
  const byName = (name: string) => show.props.find((p) => p.name === name)!.id;
  return { show, map: channelMap, byName };
}

describe("test destinations", () => {
  it("lists every wired controller for the whole show, with the channels and universes used", async () => {
    const { show, map } = await demo();
    const all = testDestinations(show, map, { type: "show" });
    // The porch WLED has nothing wired to it, so nothing goes there.
    expect(all.map((d) => d.controller.name)).toEqual(["Main FPP"]);
    const fpp = all[0];
    expect(fpp.protocol).toBe("sACN");
    expect(fpp.first).toBe(1);
    expect(fpp.channels).toBe(fpp.last);
    expect(describeUse(fpp)).toMatch(/^sACN · universes 1–\d+ · ch 1–[\d,]+$/);
  });

  it("lists only the controllers a prop, group, or port is wired to", async () => {
    const { show, map, byName } = await demo();
    const fpp = show.controllers[0];
    const tree = testDestinations(show, map, { type: "prop", id: byName("Mega Tree") });
    expect(tree.map((d) => d.controller.name)).toEqual(["Main FPP"]);
    // The tree alone is on port 2: fewer channels than the whole controller, starting later.
    const whole = testDestinations(show, map, { type: "controller", id: fpp.id })[0];
    expect(tree[0].channels).toBeLessThan(whole.channels);
    expect(tree[0].first).toBeGreaterThan(1);
    expect(testDestinations(show, map, { type: "port", controller: fpp.id, port: 2 })[0]).toMatchObject({ first: tree[0].first, last: tree[0].last });
    // Unwired things go nowhere.
    expect(testDestinations(show, map, { type: "prop", id: byName("Porch Star") })).toEqual([]);
    expect(testDestinations(show, map, { type: "controller", id: show.controllers[1].id })).toEqual([]);
    const group = { id: "g", name: "Porch", members: [byName("Porch Star")] };
    expect(testDestinations({ ...show, groups: [group] }, map, { type: "group", id: "g" })).toEqual([]);
    const mixed = { ...group, members: [byName("Porch Star"), { prop: byName("Garage Arch"), region: "r" }] };
    expect(testDestinations({ ...show, groups: [mixed] }, map, { type: "group", id: "g" }).map((d) => d.controller.name)).toEqual(["Main FPP"]);
  });

  it("describes DDP without universes", () => {
    const d = { controller: demoShow().controllers[1], protocol: "DDP" as const, first: 1, last: 1500, channels: 1500, universes: null };
    expect(describeUse(d)).toBe("DDP · ch 1–1,500");
  });
});

describe("what the screen says", () => {
  it("knows the demo show, by its controllers", () => {
    expect(isDemoShow(demoShow())).toBe(true);
    expect(isDemoShow(sampleShow())).toBe(true);
    const own = demoShow();
    own.controllers[0] = { ...own.controllers[0], name: "Garage FPP", address: "10.28.128.177" };
    expect(isDemoShow(own)).toBe(false);
    expect(isDemoShow({ ...own, controllers: [] })).toBe(false);
  });

  it("says what is being sent where", () => {
    const [fpp, wled] = demoShow().controllers;
    const at = (controller: typeof fpp) => ({ controller, protocol: "DDP" as const, first: 1, last: 3, channels: 3, universes: null });
    expect(sendingSummary("Chase", "#ffffff", [at(fpp), at(wled)])).toBe("Chase, white → Main FPP (192.168.1.50), Porch WLED (192.168.1.60)");
    expect(sendingSummary("Identify (blink)", null, [at(fpp)])).toBe("Identify (blink) → Main FPP (192.168.1.50)");
    expect(colorName("#FF0000")).toBe("red");
    expect(colorName("#ff8800")).toBe("#FF8800");
  });
});
