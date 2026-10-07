import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { MemoryBackend } from "../api/memory";
import { useAssistant } from "../state/assistant";
import { useLayoutEditor } from "../state/layoutEditor";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

async function startFresh() {
  await useApp.getState().connect(new MemoryBackend());
  const user = userEvent.setup();
  render(<App />);
  await user.click(screen.getByRole("button", { name: /^new show/i }));
  return user;
}

const nav = () => screen.getByRole("navigation", { name: "Screens" });

describe("Settings", () => {
  it("opens from the sidebar's gear, ⌘, and the command palette", async () => {
    const user = await startFresh();
    await user.click(within(nav()).getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(within(nav()).getByRole("button", { name: "Settings" })).toHaveAttribute("aria-current", "page");

    useApp.getState().setScreen("layout");
    await user.keyboard("{Meta>},{/Meta}");
    expect(useApp.getState().screen).toBe("settings");

    useApp.getState().setScreen("layout");
    await user.keyboard("{Meta>}k{/Meta}");
    await user.type(screen.getByPlaceholderText("Type a command…"), "settings");
    await user.click(screen.getByRole("option", { name: /^Settings/ }));
    expect(useApp.getState().screen).toBe("settings");
  });

  it("changes the theme, and keeps it on this computer", async () => {
    const user = await startFresh();
    useApp.getState().setScreen("settings");
    await user.click(await screen.findByRole("radio", { name: "Dark" }));
    expect(useApp.getState().themeChoice).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("pixelflow.theme")).toBe("dark");
    await user.click(screen.getByRole("radio", { name: "System" }));
    expect(useApp.getState().themeChoice).toBe("system");
  });

  it("keeps the layout grid and smart guides on this computer", async () => {
    const user = await startFresh();
    useApp.getState().setScreen("settings");
    await user.selectOptions(await screen.findByRole("combobox", { name: /Grid spacing/ }), "1");
    expect(useLayoutEditor.getState().grid).toBe(1);
    expect(localStorage.getItem("pixelflow.grid")).toBe("1");
    await user.click(screen.getByRole("checkbox", { name: /Smart guides/ }));
    expect(useLayoutEditor.getState().smartGuides).toBe(false);
    expect(localStorage.getItem("pixelflow.smartGuides")).toBe("false");
  });

  it("turns playback looping on, and opens the AI settings", async () => {
    const user = await startFresh();
    useApp.getState().setScreen("settings");
    await user.click(await screen.findByRole("checkbox", { name: /Loop sequences/ }));
    expect(useSequencer.getState().looping).toBe(true);
    expect(localStorage.getItem("pixelflow.sequenceLoop")).toBe("true");
    await user.click(screen.getByRole("button", { name: /AI settings/ }));
    expect(useAssistant.getState().settingsOpen).toBe(true);
  });

  it("says which version this is", async () => {
    await startFresh();
    useApp.getState().setScreen("settings");
    expect(await screen.findByText(/^PixelFlow \d+\.\d+\.\d+/)).toBeInTheDocument();
  });
});
