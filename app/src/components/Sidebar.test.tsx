import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";
import { Sidebar } from "./Sidebar";

async function setup(width = 1920) {
  window.innerWidth = width;
  await useApp.getState().connect(new MemoryBackend());
  useApp.setState({ started: true });
  const user = userEvent.setup();
  const view = render(<Sidebar />);
  return { user, view };
}

function resize(width: number) {
  act(() => {
    window.innerWidth = width;
    window.dispatchEvent(new Event("resize"));
  });
}

const nav = () => screen.getByRole("navigation", { name: "Screens" });
const collapsed = () => nav().dataset.collapsed === "true";

describe("the sidebar", () => {
  it("lists the screens in the order a show is made", async () => {
    await setup();
    const names = within(nav())
      .getAllByRole("button")
      .filter((b) => b.dataset.screen)
      .map((b) => b.textContent);
    expect(names).toEqual(["Layout", "Devices", "Wiring", "Test", "Sequence", "Play", "History"]);
  });

  it("shows only icons in a narrow window, each still named and with a tooltip", async () => {
    await setup(1200);
    expect(collapsed()).toBe(true);
    const wiring = within(nav()).getByRole("button", { name: "Wiring" });
    expect(wiring).toHaveAttribute("data-tip", "Wiring");
    resize(1440);
    expect(collapsed()).toBe(false);
    expect(within(nav()).getByRole("button", { name: "Wiring" })).not.toHaveAttribute("data-tip");
  });

  it("can be folded and unfolded by hand, and remembers the choice", async () => {
    const { user, view } = await setup(1600);
    await user.click(screen.getByRole("button", { name: "Show only icons" }));
    expect(collapsed()).toBe(true);
    // Remembered: a wider window doesn't unfold it, and neither does starting again.
    resize(1920);
    expect(collapsed()).toBe(true);
    view.unmount();
    render(<Sidebar />);
    expect(collapsed()).toBe(true);
    await user.click(screen.getByRole("button", { name: "Show names" }));
    expect(collapsed()).toBe(false);
  });

  it("unfolded by hand in a narrow window, stays unfolded", async () => {
    const { user } = await setup(1100);
    expect(collapsed()).toBe(true);
    await user.click(screen.getByRole("button", { name: "Show names" }));
    expect(collapsed()).toBe(false);
    resize(1000);
    expect(collapsed()).toBe(false);
  });

  it("goes to a screen", async () => {
    const { user } = await setup();
    await user.click(within(nav()).getByRole("button", { name: "Devices" }));
    expect(useApp.getState().screen).toBe("devices");
    expect(within(nav()).getByRole("button", { name: "Devices" })).toHaveAttribute("aria-current", "page");
  });
});
