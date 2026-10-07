/** How the Sequence screen fits its effects palette, timeline, and effect settings side by side. */

/** The least width the timeline keeps before the panels beside it give way. */
export const MIN_TIMELINE_WIDTH = 480;
const PALETTE_WIDTH = 160;
const PALETTE_ICONS_WIDTH = 48;
const SETTINGS_WIDTH = 288;

export interface SequenceArrangement {
  /** "icons": the effects palette shows only icons (each still named, with a tooltip). */
  palette: "full" | "icons";
  /** "floating": a selected effect's settings show over the timeline's right edge. */
  settings: "docked" | "floating";
}

/**
 * The arrangement for a workspace `width` px wide (null: not measured, so everything stays
 * docked). The palette gives way first, then the settings.
 */
export function sequenceArrangement(width: number | null): SequenceArrangement {
  if (width === null) return { palette: "full", settings: "docked" };
  return {
    palette: width - PALETTE_WIDTH - SETTINGS_WIDTH >= MIN_TIMELINE_WIDTH ? "full" : "icons",
    settings: width - PALETTE_ICONS_WIDTH - SETTINGS_WIDTH >= MIN_TIMELINE_WIDTH ? "docked" : "floating",
  };
}

/**
 * With the preview beside the timeline (in a column about 360 px wide, the effect settings under
 * it): whether a workspace `width` px wide has room (from 1200 px), and how the palette shows then
 * (icons until 1300, so the timeline keeps its room). Null: no room.
 */
export function sidePreview(width: number | null): { palette: "full" | "icons" } | null {
  if (width === null || width < 1200) return null;
  return { palette: width >= 1300 ? "full" : "icons" };
}

const LANE_PX = 30;
const ZOOM_BAR_PX = 36;
const MIN_TIMELINE_PX = 240;
const MAX_TIMELINE_MIN_PX = 420;

/**
 * The least height the timeline keeps: its ruler, music, and timing tracks (`top` px), with room
 * for three rows and the zoom bar under them.
 */
export function timelineMinHeight(top: number): number {
  return Math.min(MAX_TIMELINE_MIN_PX, Math.max(MIN_TIMELINE_PX, top + 3 * LANE_PX + ZOOM_BAR_PX));
}
