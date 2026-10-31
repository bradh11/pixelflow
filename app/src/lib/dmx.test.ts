import { describe, expect, it } from "vitest";
import type { DmxChannel, Show } from "../api/types";
import {
  DMX_LAYOUTS,
  USUAL_SHUTTER,
  channelsForColors,
  emittedColors,
  fixtureFromLayout,
  fixtureProblem,
  flashes,
  layoutChannels,
  layoutOf,
  newFixture,
  newMovingHead,
  newServo,
  nextFreeAddress,
  shutterAt,
  shutterOpen,
} from "./dmx";

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

  it("make heads at Home with a beam, servos dark, and colour from wheels and CMY (the engine's cases)", () => {
    const head = fixtureFromLayout(layout("head-rgbw-16"), 1, 1);
    expect(head.channels.map((c) => c.value ?? 0)).toEqual([128, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(head.channels[0].motor).toEqual({ rangeDeg: 540, homeDeg: 270 });
    expect(head.beam).toEqual({ angleDeg: 15, length: 6 });
    expect(layoutOf({ ...head, channels: head.channels.map((c, i) => (i === 0 ? { ...c, value: 9 } : c)) }, 1)?.id).toBe("head-rgbw-16");

    const cmy: DmxChannel[] = [
      { name: "Dimmer", role: "dimmer" },
      { name: "Cyan", role: "cyan" },
      { name: "Magenta", role: "magenta" },
      { name: "Yellow", role: "yellow" },
    ];
    expect(channelsForColors(cmy, [128, 64, 0])).toEqual([128, 0, 127, 255]);
    expect(emittedColors(cmy, [128, 0, 127, 255], 1)).toEqual([[128, 64, 0]]);

    const wheel: DmxChannel[] = [
      { name: "Dimmer", role: "dimmer" },
      {
        name: "Wheel",
        role: "colorWheel",
        value: 40,
        slots: [
          { value: 0, color: "#ffffff" },
          { value: 20, color: "#ff0000" },
          { value: 40, color: "#00ff00" },
          { value: 60, color: "#0000ff" },
        ],
      },
    ];
    expect(channelsForColors(wheel, [100, 10, 0])).toEqual([100, 20]);
    expect(channelsForColors(wheel, [0, 0, 0])).toEqual([0, 40]);
    expect(emittedColors(wheel, [255, 23], 1)).toEqual([[255, 0, 0]]);
    expect(emittedColors(wheel, [128, 55], 1)).toEqual([[0, 0, 128]]);

    const servos = layoutChannels(layout("servo-8"), 2);
    expect(channelsForColors(servos, [255, 255, 255, 255, 255, 255])).toEqual([128, 128]);
    expect(emittedColors(servos, [128, 128], 2)).toEqual([
      [0, 0, 0],
      [0, 0, 0],
    ]);
  });

  it("put a new head and a new servo after the fixtures already there", () => {
    const show = { props: [] } as unknown as Show;
    show.props.push(newFixture(show));
    const head = newMovingHead(show);
    expect(head.name).toBe("Moving Head 1");
    expect(head.dmx?.address).toBe(5);
    show.props.push(head);
    const servo = newServo(show);
    expect(servo.dmx?.address).toBe(16);
    expect(servo.dmx?.channels.map((c) => c.role)).toEqual(["servo", "servoFine"]);
    expect(fixtureProblem(servo.dmx!, 1)).toBeNull();
    expect(fixtureProblem({ ...head.dmx!, beam: { angleDeg: 0, length: 6 } }, 1)).toContain("1° to 120°");
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

  it("show a fixture's light while its shutter is open or flashing (the same cases as crates/pf-model/src/shutter.rs)", () => {
    expect(shutterAt(USUAL_SHUTTER, 9)).toEqual({ kind: "open" });
    expect(shutterAt(USUAL_SHUTTER, 10)).toEqual({ kind: "strobe", hz: 1 });
    expect(shutterAt(USUAL_SHUTTER, 255)).toEqual({ kind: "strobe", hz: 20 });
    const head: DmxChannel["shutter"] = [
      { from: 0, does: "closed" },
      { from: 8, does: "open" },
      { from: 16, does: "strobe" },
      { from: 132, does: "open" },
    ];
    expect([3, 8, 16, 131, 200].map((v) => shutterAt(head!, v))).toEqual([
      { kind: "closed" },
      { kind: "open" },
      { kind: "strobe", hz: 1 },
      { kind: "strobe", hz: 20 },
      { kind: "open" },
    ]);
    expect(shutterAt([{ from: 20, does: "closed" }], 5)).toEqual({ kind: "open" });
    // Once a second at 40 frames a second: one frame in 40; ten a second: every fourth; twenty or more: every other.
    const lit = (hz: number, frames: number) => Array.from({ length: frames }, (_, n) => n * 25).filter((t) => flashes(hz, t, 25));
    expect(lit(1, 120)).toEqual([0, 1000, 2000]);
    expect(lit(10, 40)).toHaveLength(10);
    expect(lit(35, 6)).toEqual([0, 50, 100]);
    const flood = layoutChannels(layout("dimmer-rgb-strobe"), 1);
    expect(shutterOpen(flood, [255, 255, 0, 0, 0], 25, 25)).toBe(true);
    expect(shutterOpen(flood, [255, 255, 0, 0, 255], 25, 25)).toBe(false);
    expect(shutterOpen(flood, [255, 255, 0, 0, 255], 0, 25)).toBe(true);
    flood[4] = { ...flood[4], shutter: [{ from: 9, does: "closed" }, { from: 3, does: "open" }] };
    expect(fixtureProblem({ address: 1, channels: flood }, 1)).toContain("Strobe");
  });
});
