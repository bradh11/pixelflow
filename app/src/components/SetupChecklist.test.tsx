import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend, emptyShow } from "../api/memory";
import { currentSetupKey, useSetup } from "../state/setup";
import { useApp } from "../state/store";
import { CommandPalette } from "./CommandPalette";
import { Sidebar } from "./Sidebar";

async function setup({ width = 1920, show = demoShow() } = {}) {
  window.innerWidth = width;
  const backend = new MemoryBackend(show);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  const view = render(<Sidebar />);
  return { user, backend, view };
}

const card = () => screen.getByRole("region", { name: "Set up your show" });
const step = (name: RegExp) => within(card()).getByRole("button", { name });

describe("the setup checklist", () => {
  it("ticks each step from the show, and points at the next one", async () => {
    await setup();
    // The demo has controllers and props, but the Porch Star isn't wired.
    expect(within(card()).getByRole("progressbar", { name: "Steps done" })).toHaveAttribute("aria-valuetext", "2 of 6 done");
    expect(step(/^Find your controllers, done/)).toBeInTheDocument();
    expect(step(/^Draw your props, done/)).toBeInTheDocument();
    expect(step(/^Wire your props, next/)).toHaveTextContent("1 prop not wired");
    expect(step(/^Test your lights$/)).toBeInTheDocument();
  });

  it("goes to the screen where a step is done", async () => {
    const { user } = await setup();
    await user.click(step(/^Wire your props/));
    expect(useApp.getState().screen).toBe("wiring");
    await user.click(step(/^Find your controllers/));
    expect(useApp.getState().screen).toBe("devices");
  });

  it("follows the show as it changes", async () => {
    await setup({ show: emptyShow("New") });
    expect(step(/^Find your controllers, next/)).toBeInTheDocument();
    const arch = demoShow().props.find((p) => p.name === "Garage Arch")!;
    await act(() => useApp.getState().apply([{ type: "addProp", prop: arch }]));
    expect(step(/^Draw your props, done/)).toBeInTheDocument();
    // Not next (there are no controllers yet): its progress shows on hover.
    expect(step(/^Wire your props$/)).toHaveAttribute("data-tip", "1 prop not wired");
    act(() => useSetup.getState().markTested(currentSetupKey()));
    expect(step(/^Test your lights, done/)).toBeInTheDocument();
  });

  it("can be put away for the show, and stays away", async () => {
    const { user, view } = await setup();
    await user.click(within(card()).getByRole("button", { name: "Put the checklist away" }));
    expect(screen.queryByRole("region", { name: "Set up your show" })).not.toBeInTheDocument();
    view.unmount();
    render(<Sidebar />);
    expect(screen.queryByRole("region", { name: "Set up your show" })).not.toBeInTheDocument();
    // The command palette brings it back.
    render(<CommandPalette />);
    act(() => useApp.getState().setPaletteOpen(true));
    await user.click(screen.getByRole("option", { name: "Show the setup checklist" }));
    expect(card()).toBeInTheDocument();
  });

  it("is a button beside the icons-only sidebar, showing the steps when pressed", async () => {
    const { user } = await setup({ width: 1100 });
    expect(screen.queryByRole("region", { name: "Set up your show" })).not.toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Set up your show: 2 of 6 done" });
    await user.click(button);
    const dialog = screen.getByRole("dialog", { name: "Set up your show" });
    await user.click(within(dialog).getByRole("button", { name: /^Wire your props/ }));
    expect(useApp.getState().screen).toBe("wiring");
    expect(screen.queryByRole("dialog", { name: "Set up your show" })).not.toBeInTheDocument();
    await user.click(button);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Set up your show" })).not.toBeInTheDocument();
    expect(button).toHaveFocus();
  });
});
