import { describe, expect, it, vi } from "vitest";
import type { PreviewProp } from "../api/types";
import { v3, viewProjection } from "./layout3d";
import { type LabelSpot, deepSpots, drawLabels, fitText, flatSpots, placeLabels } from "./propLabels";

/** Every character 6 pixels wide. */
const widthOf = (text: string) => text.length * 6;
const size = { width: 400, height: 300 };
const spot = (text: string, x: number, top: number, bottom: number, weight = 100): LabelSpot => ({ text, x, top, bottom, weight });

describe("prop names on the view", () => {
  it("go under their prop, centered on it, in a tag as wide as the name", () => {
    expect(placeLabels([spot("Arch", 100, 50, 80)], size, widthOf)).toEqual([{ text: "Arch", x: 100 - 16, y: 83, width: 32, height: 16 }]);
  });

  it("go over a prop with no room under it, or across its middle, and stay on the screen", () => {
    const [over] = placeLabels([spot("Tree", 100, 100, 295)], size, widthOf);
    expect(over.y).toBe(100 - 3 - 16);
    const [across] = placeLabels([spot("Matrix", 200, 1, 299)], size, widthOf);
    expect(across.y).toBe(150 - 8);
    // At the screen's edge the tag slides in.
    const [edge] = placeLabels([spot("Roofline", 1, 10, 20)], size, widthOf);
    expect(edge.x).toBe(2);
  });

  it("don't pile up where props crowd together: the biggest is named, and a name with no clear place is left out", () => {
    const crowd = [spot("Small A", 100, 50, 60, 10), spot("Big", 102, 50, 62, 500), spot("Small B", 104, 51, 61, 20)];
    const placed = placeLabels(crowd, size, widthOf);
    const names = placed.map((p) => p.text);
    expect(names[0]).toBe("Big");
    for (let i = 0; i < placed.length; i++)
      for (let j = i + 1; j < placed.length; j++) {
        const [a, b] = [placed[i], placed[j]];
        const overlap = a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
        expect(overlap).toBe(false);
      }
    // The next biggest goes over the crowd; the smallest has nowhere left.
    expect(names).toEqual(["Big", "Small B"]);
    // Apart, each is named.
    expect(placeLabels([spot("Small A", 100, 50, 60, 10), spot("Big", 300, 50, 62, 500)], size, widthOf)).toHaveLength(2);
  });

  it("leave out props off the screen, and shorten long names", () => {
    expect(placeLabels([spot("Gone", -20, 10, 20), spot("Below", 50, 320, 330)], size, widthOf)).toEqual([]);
    const [long] = placeLabels([spot("A very long name for the garage roofline left", 200, 10, 20)], size, widthOf);
    expect(long.text.endsWith("…")).toBe(true);
    expect(long.width).toBeLessThanOrEqual(160);
    expect(fitText("Short", 100, widthOf)).toBe("Short");
    expect(fitText("Garage roof", 40, widthOf)).toBe("Garag…");
  });

  it("are placed from each prop's box in a flat view", () => {
    const props: PreviewProp[] = [
      { prop: "a", frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 2, 1] },
      { prop: "unnamed", frameOffset: 6, channelsPerPixel: 3, points: [5, 5] },
    ];
    const spots = flatSpots(props, new Map([["a", "Arch"]]), { cx: 0, cy: 0, zoom: 10 }, size);
    expect(spots).toEqual([{ text: "Arch", x: 210, top: 140, bottom: 150, weight: 21 * 11 }]);
  });

  it("are placed from where each prop's box lands through the 3D camera, and left out when it reaches behind the camera", () => {
    const orbit = { target: v3(0, 1, 0), yaw: 0, pitch: 0, distance: 10 };
    const m = viewProjection(orbit, size);
    const names = new Map([
      ["front", "Front"],
      ["behind", "Behind"],
    ]);
    const spots = deepSpots(
      [
        { prop: "front", box: { min: v3(-1, 0, 0), max: v3(1, 2, 0) } },
        { prop: "behind", box: { min: v3(-1, 0, 5), max: v3(1, 2, 12) } },
        { prop: "nameless", box: { min: v3(0, 0, 0), max: v3(1, 1, 1) } },
      ],
      names,
      m,
      size,
    );
    expect(spots.map((s) => s.text)).toEqual(["Front"]);
    expect(spots[0].x).toBeCloseTo(200);
    expect(spots[0].top).toBeLessThan(150);
    expect(spots[0].bottom).toBeGreaterThan(150);
  });

  it("are drawn as tags with their text", () => {
    const calls: string[] = [];
    const ctx = {
      save: () => calls.push("save"),
      restore: () => calls.push("restore"),
      measureText: (t: string) => ({ width: t.length * 6 }),
      beginPath: () => {},
      roundRect: vi.fn(),
      fill: () => calls.push("fill"),
      fillText: (t: string) => calls.push(`text ${t}`),
    } as unknown as CanvasRenderingContext2D;
    drawLabels(ctx, [spot("Arch", 100, 50, 80)], size);
    expect(calls).toEqual(["save", "fill", "text Arch", "restore"]);
    calls.length = 0;
    drawLabels(ctx, [], size);
    expect(calls).toEqual([]);
  });
});
