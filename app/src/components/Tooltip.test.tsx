import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TOOLTIP_DELAY_MS, TooltipLayer } from "./Tooltip";

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
});
afterEach(() => {
  vi.useRealTimers();
});

const wait = (ms: number) => act(() => vi.advanceTimersByTime(ms));

function setup(ui: React.ReactNode) {
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  render(
    <>
      <TooltipLayer />
      {ui}
    </>,
  );
  return user;
}

describe("the shared tooltip", () => {
  it("shows a button's tip after a short wait on hover, and hides when the pointer leaves", async () => {
    const user = setup(
      <>
        <button type="button" aria-label="Undo" data-tip="Undo: Move Mega Tree" data-tip-key="⌘Z">
          ↶
        </button>
        <p>elsewhere</p>
      </>,
    );
    await user.hover(screen.getByRole("button", { name: "Undo" }));
    expect(screen.queryByRole("tooltip")).toBeNull();
    await wait(TOOLTIP_DELAY_MS + 10);
    const tip = screen.getByRole("tooltip");
    expect(tip).toHaveTextContent("Undo: Move Mega Tree");
    expect(tip).toHaveTextContent("⌘Z");
    // It says more than the name, so it describes the button too.
    expect(screen.getByRole("button", { name: "Undo" })).toHaveAccessibleDescription("Undo: Move Mega Tree⌘Z");
    await user.unhover(screen.getByRole("button", { name: "Undo" }));
    await user.hover(screen.getByText("elsewhere"));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("shows on keyboard focus, and Escape hides it", async () => {
    const user = setup(
      <button type="button" aria-label="Zoom in" data-tip="Zoom in">
        +
      </button>,
    );
    await user.tab();
    await wait(TOOLTIP_DELAY_MS + 10);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Zoom in");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("doesn't show when a click focuses the button", async () => {
    const user = setup(
      <button type="button" aria-label="Zoom in" data-tip="Zoom in">
        +
      </button>,
    );
    fireEvent.pointerDown(screen.getByRole("button"));
    fireEvent.focus(screen.getByRole("button"));
    await wait(TOOLTIP_DELAY_MS + 10);
    expect(screen.queryByRole("tooltip")).toBeNull();
    void user;
  });

  it("takes over a title while it shows, and gives it back", async () => {
    const user = setup(
      <>
        <button type="button" title="Fold the list away">
          «
        </button>
        <p>elsewhere</p>
      </>,
    );
    const button = screen.getByRole("button");
    await user.hover(button);
    // The browser's own tooltip would show the title too: it's held while the tooltip shows.
    expect(button).not.toHaveAttribute("title");
    await wait(TOOLTIP_DELAY_MS + 10);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Fold the list away");
    await user.unhover(button);
    await user.hover(screen.getByText("elsewhere"));
    expect(button).toHaveAttribute("title", "Fold the list away");
  });

  it("shows the next tip straight away while moving along a bar", async () => {
    const user = setup(
      <>
        <button type="button" data-tip="Line" aria-label="Line" />
        <button type="button" data-tip="Arch" aria-label="Arch" />
      </>,
    );
    await user.hover(screen.getByRole("button", { name: "Line" }));
    await wait(TOOLTIP_DELAY_MS + 10);
    await user.hover(screen.getByRole("button", { name: "Arch" }));
    await wait(1);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Arch");
  });

  it("shows aria-keyshortcuts as the shortcut", async () => {
    const user = setup(<button type="button" aria-label="Loop playback" data-tip="Loop playback" aria-keyshortcuts="L" />);
    await user.hover(screen.getByRole("button"));
    await wait(TOOLTIP_DELAY_MS + 10);
    expect(screen.getByRole("tooltip")).toHaveTextContent(/Loop playback\s*L/);
  });
});
