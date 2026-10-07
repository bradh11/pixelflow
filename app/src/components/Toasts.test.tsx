import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TOAST_ACTION_MS, TOAST_MS, toast } from "../state/toast";
import { Toasts } from "./Toasts";

describe("toasts", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("sit at the bottom right and go by themselves", () => {
    render(<Toasts />);
    act(() => void toast("Saved Demo House"));
    expect(screen.getByTestId("toasts")).toHaveClass("right-4");
    act(() => vi.advanceTimersByTime(TOAST_MS + 10));
    expect(screen.queryByTestId("toast")).not.toBeInTheDocument();
  });

  it("wait while pointed at or focused, so Undo can be reached", () => {
    render(<Toasts />);
    act(() => void toast("Deleted Arch 1", { label: "Undo", run: () => undefined }));
    const shown = screen.getByTestId("toast");
    fireEvent.pointerEnter(shown);
    act(() => vi.advanceTimersByTime(TOAST_ACTION_MS * 3));
    expect(screen.getByTestId("toast")).toBeInTheDocument();
    fireEvent.pointerLeave(shown);
    fireEvent.focus(screen.getByRole("button", { name: "Undo" }));
    act(() => vi.advanceTimersByTime(TOAST_ACTION_MS * 3));
    expect(screen.getByTestId("toast")).toBeInTheDocument();
    fireEvent.blur(screen.getByRole("button", { name: "Undo" }));
    act(() => vi.advanceTimersByTime(TOAST_ACTION_MS + 10));
    expect(screen.queryByTestId("toast")).not.toBeInTheDocument();
  });

  it("only success toasts show a check mark", () => {
    render(<Toasts />);
    act(() => void toast("Use Undo (⌘Z) instead.", undefined, "info"));
    expect(screen.getByTestId("toast")).toHaveAttribute("data-tone", "info");
    expect(screen.getByTestId("toast").querySelector(".lucide-info")).not.toBeNull();
  });
});
