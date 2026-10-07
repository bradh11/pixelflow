import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { MemoryBackend } from "../api/memory";
import { SHORTCUTS, hintFor, keysLabel } from "../lib/shortcuts";
import { useApp } from "../state/store";

async function startFresh() {
  await useApp.getState().connect(new MemoryBackend());
  const user = userEvent.setup();
  render(<App />);
  await user.click(screen.getByRole("button", { name: /^new show/i }));
  return user;
}

const sheet = () => screen.queryByRole("dialog", { name: "Keyboard shortcuts" });

describe("the keyboard shortcut sheet", () => {
  it("opens with ?, lists every shortcut by screen, and closes with Escape", async () => {
    const user = await startFresh();
    await user.keyboard("?");
    const dialog = sheet()!;
    expect(dialog).toBeInTheDocument();
    for (const group of ["Everywhere", "Layout", "Sequence", "Wiring"]) {
      expect(within(dialog).getByRole("region", { name: group })).toBeInTheDocument();
    }
    // Built from the registry: every entry is there, with its keys.
    for (const s of SHORTCUTS) {
      const region = within(dialog).getByRole("region", { name: s.group });
      const term = within(region).getByText(s.label);
      expect(term.parentElement).toHaveTextContent(keysLabel(s));
    }
    expect(within(dialog).getByRole("searchbox", { name: "Search shortcuts" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(sheet()).not.toBeInTheDocument();
  });

  it("narrows to what's searched for", async () => {
    const user = await startFresh();
    await user.keyboard("?");
    await user.type(screen.getByRole("searchbox", { name: "Search shortcuts" }), "loop");
    const dialog = sheet()!;
    expect(within(dialog).getByText("Loop playback on or off")).toBeInTheDocument();
    expect(within(dialog).queryByText("Save")).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("region", { name: "Layout" })).not.toBeInTheDocument();
    await user.clear(screen.getByRole("searchbox", { name: "Search shortcuts" }));
    await user.type(screen.getByRole("searchbox", { name: "Search shortcuts" }), "zzzz");
    expect(within(dialog).getByText(/No shortcuts match/)).toBeInTheDocument();
  });

  it("isn't opened by typing ? in a field", async () => {
    const user = await startFresh();
    await user.click(screen.getByRole("button", { name: /^Commands/ }));
    await user.keyboard("?");
    expect(sheet()).not.toBeInTheDocument();
    expect(screen.getByPlaceholderText("Type a command…")).toHaveValue("?");
  });

  it("opens from the command palette, which shows the registry's key hints", async () => {
    const user = await startFresh();
    await user.keyboard("{Meta>}k{/Meta}");
    await user.type(screen.getByPlaceholderText("Type a command…"), "keyboard");
    const option = screen.getByRole("option", { name: /Keyboard shortcuts/ });
    expect(option).toHaveTextContent(hintFor("shortcuts"));
    await user.keyboard("{Enter}");
    expect(sheet()).toBeInTheDocument();
  });

  it("gives the top bar's Undo its hint from the registry", async () => {
    await startFresh();
    expect(screen.getByRole("button", { name: "Undo" })).toHaveAttribute("data-tip-key", hintFor("undo"));
  });
});
