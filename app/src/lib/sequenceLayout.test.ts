import { describe, expect, it } from "vitest";
import { sequenceArrangement, sidePreview, timelineMinHeight } from "./sequenceLayout";

describe("the Sequence screen's arrangement", () => {
  it("keeps everything docked when it isn't measured, or there's room", () => {
    expect(sequenceArrangement(null)).toEqual({ palette: "full", settings: "docked" });
    expect(sequenceArrangement(1104)).toEqual({ palette: "full", settings: "docked" });
    expect(sequenceArrangement(928)).toEqual({ palette: "full", settings: "docked" });
  });

  it("shows the effects as icons first, so the timeline keeps 480 px", () => {
    // 1440 wide with the assistant docked: the main area is 880.
    expect(sequenceArrangement(880)).toEqual({ palette: "icons", settings: "docked" });
    expect(sequenceArrangement(816)).toEqual({ palette: "icons", settings: "docked" });
  });

  it("then floats the effect settings over the timeline", () => {
    expect(sequenceArrangement(700)).toEqual({ palette: "icons", settings: "floating" });
  });
});

describe("the timeline's least height", () => {
  it("fits the ruler, music, and timing tracks, plus three rows and the zoom bar", () => {
    // Ruler 24 + music 44 + five timing tracks of 18 = 158.
    expect(timelineMinHeight(158)).toBe(158 + 3 * 30 + 36);
  });

  it("is never less than 240 px, nor more than 420", () => {
    expect(timelineMinHeight(24)).toBe(240);
    expect(timelineMinHeight(600)).toBe(420);
  });
});

describe("the preview beside the timeline", () => {
  it("fits only from 1200 px, and shows the effects as icons until 1300", () => {
    expect(sidePreview(null)).toBeNull();
    expect(sidePreview(1104)).toBeNull();
    expect(sidePreview(1200)).toEqual({ palette: "icons" });
    expect(sidePreview(1264)).toEqual({ palette: "icons" });
    expect(sidePreview(1744)).toEqual({ palette: "full" });
  });
});
