import { describe, expect, it } from "vitest";
import { COLORS, fitDisplay } from "./flatPreview";
import { packColor } from "./pixelBatches";
import { LAYOUT_LOOK, PREVIEW_LOOK, lightLook } from "./pixelLooks";

describe("how pixels look, flat and in 3D", () => {
  it("has the previews' 3D view read the flat previews' own colors and dot sizes", () => {
    expect(PREVIEW_LOOK.colors).toBe(COLORS);
    // The smallest and the largest dot a flat preview draws: fitted far out, and close in.
    const size = { width: 800, height: 600 };
    const far = fitDisplay({ minX: -5000, minY: -5000, maxX: 5000, maxY: 5000 }, size).radius;
    const close = fitDisplay({ minX: 0, minY: 0, maxX: 0.5, maxY: 0.5 }, size).radius;
    expect(PREVIEW_LOOK.radius).toEqual([far, close]);
  });

  it("gives the 3D view each screen's colors as the flat drawing stamps them, and its dot sizes", () => {
    for (const flat of [PREVIEW_LOOK, LAYOUT_LOOK]) {
      const { look, selected } = lightLook(flat);
      expect(look.radius).toEqual(flat.radius);
      // The same bytes, and as solid.
      const stamped = (css: string) => [packColor(css) & 255, (packColor(css) >> 8) & 255, (packColor(css) >> 16) & 255, (packColor(css) >>> 24) / 255];
      expect(look.unlit).toEqual(stamped(flat.colors.unlit));
      expect(look.dark).toEqual(stamped(flat.colors.dark));
      expect(selected).toEqual(stamped(flat.colors.selected).slice(0, 3));
    }
  });

  it("shows quiet pixels fainter in the previews than on the Layout screen, where props are picked", () => {
    const [looking, editing] = [lightLook(PREVIEW_LOOK), lightLook(LAYOUT_LOOK)];
    expect(looking.look.unlit).toEqual([200, 200, 200, 89 / 255]);
    expect(looking.look.dark).toEqual([70, 70, 70, 140 / 255]);
    expect(editing.look.unlit).toEqual([220, 220, 220, 179 / 255]);
    expect(editing.look.dark).toEqual([90, 90, 90, 153 / 255]);
    expect(editing.selected).toEqual([0xa7, 0x8b, 0xfa]);
    expect(editing.look.radius[1]).toBeGreaterThan(looking.look.radius[1]);
  });
});
