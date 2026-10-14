import { describe, expect, it } from "vitest";
import type { Show } from "../api/types";
import { DMX_LAYOUTS, channelsForColors, emittedColors, fixtureProblem, layoutChannels, layoutOf, newFixture, nextFreeAddress } from "./dmx";

const layout = (id: string) => DMX_LAYOUTS.find((l) => l.id === id)!;

describe("DMX fixtures in the window", () => {
  it("work out channels from a color as the engine does (the same cases as crates/pf-model/src/dmx.rs)", () => {
    expect(channelsForColors(layoutChannels(layout("dimmer-rgb"), 1), [13, 6, 0])).toEqual([13, 255, 118, 0]);
    expect(channelsForColors(layoutChannels(layout("rgbw"), 1), [100, 100, 250])).toEqual([0, 0, 150, 100]);
    expect(channelsForColors(layoutChannels(layout("rgbwa"), 1), [255, 200, 100])).toEqual([0, 22, 0, 100, 155]);
    expect(channelsForColors(layoutChannels(layout("bar-dimmer-strobe-rgb"), 2), [0, 0, 100, 0, 50, 0])).toEqual([100, 0, 0, 0, 255, 0, 128, 0]);
    const rgbwa = layoutChannels(layout("rgbwa"), 1);
    expect(emittedColors(rgbwa, channelsForColors(rgbwa, [255, 200, 100]), 1)).toEqual([[255, 200, 100]]);
  });

  it("number a library layout's channels by cell, and know a fixture made from one", () => {
    const bar = layoutChannels(layout("bar-rgb"), 2);
    expect(bar.map((c) => c.name)).toEqual(["Red 1", "Green 1", "Blue 1", "Red 2", "Green 2", "Blue 2"]);
    expect(bar[3]).toEqual({ name: "Red 2", role: "red", cell: 1 });
    expect(layoutOf({ address: 1, channels: bar, layout: "bar-rgb" }, 2)?.id).toBe("bar-rgb");
    expect(layoutOf({ address: 1, channels: bar.slice(1) }, 2)).toBeNull();
  });

  it("put a new flood at the first free address and say what's wrong with one", () => {
    const show = { props: [] } as unknown as Show;
    const first = newFixture(show);
    expect(first.dmx?.address).toBe(1);
    show.props.push(first);
    expect(nextFreeAddress(show, 4)).toBe(5);
    expect(newFixture(show).name).toBe("Flood 2");
    expect(fixtureProblem({ address: 510, channels: first.dmx!.channels }, 1)).toContain("past 512");
    expect(fixtureProblem(first.dmx!, 1)).toBeNull();
  });
});
