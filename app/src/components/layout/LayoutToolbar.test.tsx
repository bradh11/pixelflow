import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LABEL_CLASS, LABEL_FROM, LayoutToolbar } from "./LayoutToolbar";

describe("the layout tool bar's labels", () => {
  it("give way a group at a time, least needed first, and the drawing tools last", () => {
    expect(LABEL_FROM.toggles).toBeGreaterThan(LABEL_FROM.view);
    expect(LABEL_FROM.view).toBeGreaterThan(LABEL_FROM.shapes);
    expect(LABEL_FROM.shapes).toBeGreaterThan(LABEL_FROM.tools);
    // At 1440 px with the assistant open, the bar is about 830 px: the tools keep their names.
    expect(LABEL_FROM.tools).toBeLessThanOrEqual(820);
    for (const [group, from] of Object.entries(LABEL_FROM)) {
      expect(LABEL_CLASS[group as keyof typeof LABEL_FROM]).toBe(`sr-only @min-[${from}px]:not-sr-only`);
    }
  });

  it("put each button in its group", () => {
    render(<LayoutToolbar hasPhoto onChoosePhoto={() => undefined} />);
    const bar = screen.getByRole("toolbar", { name: "Layout tools" });
    const group = (name: string) =>
      [...within(bar).getByRole("button", { name }).querySelectorAll("span")].find((s) => s.textContent === name)?.className;
    expect(group("Select")).toBe(LABEL_CLASS.tools);
    expect(group("Tree")).toBe(LABEL_CLASS.tools);
    expect(group("More shapes")).toBe(LABEL_CLASS.shapes);
    expect(group("Fit")).toBe(LABEL_CLASS.view);
    expect(group("Edit photo")).toBe(LABEL_CLASS.view);
    expect(group("Snap to grid")).toBe(LABEL_CLASS.toggles);
    expect(group("Smart guides")).toBe(LABEL_CLASS.toggles);
  });
});
