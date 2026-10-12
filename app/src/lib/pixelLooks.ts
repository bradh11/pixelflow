// How pixels look where the display is drawn, flat or in 3D: the colors of the ones with none of
// their own, and how big a view's dot gets. The previews (the Sequence and Play screens, and the
// preview's own window) have the flat previews' look (lib/flatPreview.ts); the Layout screen,
// where props are picked and moved and so stand out more, has its own. The 3D view takes whichever
// its screen has, as numbers its renderer can use.

import { COLORS, DOT_RADII } from "./flatPreview";
import type { LightLook, Rgb, Rgba } from "./layout3d";
import { type PixelColors, packColor } from "./pixelBatches";

export interface PixelLook {
  colors: Required<PixelColors>;
  /** The smallest and the largest radius a view's dot gets, in screen pixels: in between, it's `DOT_RADIUS` of a layout unit. */
  radius: readonly [number, number];
}

export const PREVIEW_LOOK: PixelLook = { colors: COLORS, radius: DOT_RADII };

export const LAYOUT_LOOK: PixelLook = {
  colors: { unlit: "rgba(220, 220, 220, 0.7)", selected: "#a78bfa", dark: "rgba(90, 90, 90, 0.6)" },
  radius: [1.3, 4.5],
};

/** A CSS color as red, green and blue (of 255) and how solid it is (0–1). */
function rgba(css: string): Rgba {
  const packed = packColor(css);
  return [packed & 255, (packed >> 8) & 255, (packed >> 16) & 255, (packed >>> 24) / 255];
}

/** A look as the 3D view draws it, and the color of a selected prop's pixels there. */
export function lightLook(look: PixelLook): { look: LightLook; selected: Rgb } {
  const [r, g, b] = rgba(look.colors.selected);
  return { look: { radius: look.radius, unlit: rgba(look.colors.unlit), dark: rgba(look.colors.dark) }, selected: [r, g, b] };
}
