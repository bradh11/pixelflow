import { describe, expect, it } from "vitest";
import { NO_LABELS, edited, nextLabels, stepped } from "./undoLabels";

describe("undo and redo names", () => {
  it("follow edits, undo, and redo", () => {
    let s = edited(NO_LABELS, 1, 2, "Add Arch 1");
    s = edited(s, 2, 3, "Move Arch 1");
    expect(nextLabels(s, 3)).toEqual({ undo: "Move Arch 1", redo: null });
    s = stepped(s, 3, 4);
    expect(nextLabels(s, 4)).toEqual({ undo: "Add Arch 1", redo: "Move Arch 1" });
    s = stepped(s, 4, 5, true);
    expect(nextLabels(s, 5)).toEqual({ undo: "Move Arch 1", redo: null });
  });

  it("a new edit clears the redo names", () => {
    let s = edited(NO_LABELS, 1, 2, "Add Arch 1");
    s = stepped(s, 2, 3);
    s = edited(s, 3, 4, "Add Tree 1");
    expect(nextLabels(s, 4)).toEqual({ undo: "Add Tree 1", redo: null });
  });

  it("edits in one gesture are one name", () => {
    let s = edited(NO_LABELS, 1, 2, "Add Fade");
    s = edited(s, 2, 3, "Change Fade", "slider-1");
    s = edited(s, 3, 4, "Change Fade", "slider-1");
    s = stepped(s, 4, 5);
    expect(nextLabels(s, 5).undo).toBe("Add Fade");
  });

  it("aren't said after a change made some other way, until the next edit", () => {
    let s = edited(NO_LABELS, 1, 2, "Add Arch 1");
    // Revision 3 came from elsewhere (a restored backup, say).
    expect(nextLabels(s, 3)).toEqual({ undo: null, redo: null });
    s = stepped(s, 3, 4);
    expect(nextLabels(s, 4)).toEqual({ undo: null, redo: null });
    s = edited(s, 4, 5, "Move Arch 1");
    expect(nextLabels(s, 5)).toEqual({ undo: "Move Arch 1", redo: null });
    // Undoing past what's known says nothing rather than something wrong.
    s = stepped(s, 5, 6);
    expect(nextLabels(s, 6)).toEqual({ undo: null, redo: "Move Arch 1" });
  });

  it("aren't trusted when a step skips a revision (a change made elsewhere landed in between)", () => {
    let s = edited(NO_LABELS, 1, 2, "Add Arch 1");
    s = edited(s, 2, 4, "Move Arch 1");
    expect(nextLabels(s, 4)).toEqual({ undo: null, redo: null });
    s = edited(NO_LABELS, 1, 2, "Add Arch 1");
    s = stepped(s, 2, 4);
    expect(nextLabels(s, 4)).toEqual({ undo: null, redo: null });
  });

  it("keep no more names than the engine keeps steps", () => {
    let s = NO_LABELS;
    for (let r = 0; r < 250; r++) s = edited(s, r, r + 1, `Step ${r}`);
    expect(s.undo).toHaveLength(200);
    expect(nextLabels(s, 250).undo).toBe("Step 249");
  });
});
