// Friendly names for how effects lay out a target's pixels: a group's layout ("Effects draw on")
// and an effect's render style and turn or flip. The values are the engine's (xLights' group
// layouts, render styles, and buffer transformations).

import type { BufferTransform, RenderStyle } from "../api/sequence";
import type { GroupLayout } from "../api/types";

/** xLights' default grid size, and the range it allows. */
export const GRID_SIZE = { default: 400, min: 10, max: 4000 };

export const GROUP_LAYOUTS: { value: GroupLayout; label: string }[] = [
  { value: "minimalGrid", label: "Where the lights are" },
  { value: "grid", label: "Where the lights are, on the whole layout" },
  { value: "horizontalPerModel", label: "A column per member" },
  { value: "verticalPerModel", label: "A row per member" },
  { value: "horizontalStack", label: "Members side by side" },
  { value: "verticalStack", label: "Members one above the other" },
  { value: "horizontalStackScaled", label: "Members side by side, same size" },
  { value: "verticalStackScaled", label: "Members one above the other, same size" },
  { value: "singleLine", label: "One line, member after member" },
  { value: "overlayCentered", label: "Members on top of each other, centered" },
  { value: "overlayScaled", label: "Members on top of each other, stretched" },
  { value: "singleLineModelAsPixel", label: "Each member as one light, in a row" },
  { value: "defaultModelAsPixel", label: "Each member as one light, where it is" },
  { value: "perModelDefault", label: "Each member on its own" },
];

/** The styles for a prop or submodel; a group has all of them. */
const OWN_STYLES: RenderStyle[] = ["default", "perPreview", "singleLine", "asPixel"];

const STYLE_LABELS: Record<RenderStyle, string> = {
  default: "Its own layout",
  perPreview: "Where the lights are",
  singleLine: "One line",
  asPixel: "All as one light",
  horizontalPerModel: "A column per member",
  verticalPerModel: "A row per member",
  horizontalStack: "Members side by side",
  verticalStack: "Members one above the other",
  horizontalStackScaled: "Members side by side, same size",
  verticalStackScaled: "Members one above the other, same size",
  overlayCentered: "Members on top of each other, centered",
  overlayScaled: "Members on top of each other, stretched",
  singleLineModelAsPixel: "Each member as one light, in a row",
  defaultModelAsPixel: "Each member as one light, where it is",
  perModelDefault: "Each member on its own",
  perModelPerPreview: "Each member on its own, where the lights are",
  perModelSingleLine: "Each member on its own, as one line",
};

/** The render styles to offer on a target (a group gets the ones that lay out its members). */
export function renderStyleOptions(group: boolean): { value: RenderStyle; label: string }[] {
  const styles = group ? (Object.keys(STYLE_LABELS) as RenderStyle[]) : OWN_STYLES;
  return styles.map((value) => ({
    value,
    label: value === "default" && group ? "The group's layout" : STYLE_LABELS[value],
  }));
}

export const BUFFER_TRANSFORMS: { value: BufferTransform; label: string }[] = [
  { value: "none", label: "As it is" },
  { value: "rotateCw90", label: "Turned right" },
  { value: "rotateCcw90", label: "Turned left" },
  { value: "rotate180", label: "Upside down" },
  { value: "flipVertical", label: "Flipped top to bottom" },
  { value: "flipHorizontal", label: "Flipped left to right" },
  { value: "rotateCw90FlipHorizontal", label: "Turned right and flipped" },
  { value: "rotateCcw90FlipHorizontal", label: "Turned left and flipped" },
];
