import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import { useAssistant } from "../state/assistant";
import { useLayoutEditor } from "../state/layoutEditor";
import { usePreviewSync } from "../state/previewSync";
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

  it("plays a click to line the preview up, with an offset kept on this computer", async () => {
    const user = await startFresh();
    const backend = useApp.getState().backend as MemoryBackend;
    useApp.getState().setScreen("settings");
    const slider = await screen.findByRole("slider", { name: "Preview offset" });
    fireEvent.change(slider, { target: { value: "40" } });
    expect(usePreviewSync.getState().offsetMs).toBe(40);
    expect(localStorage.getItem("pixelflow.previewSyncMs")).toBe("40");
    expect(screen.getByText("+40 ms (picture earlier)")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Play the click" }));
    expect(backend.calls).toContain("syncClickStart:500");
    await user.click(screen.getByRole("button", { name: "Tap along" }));
    expect(screen.getByText(/Press Space or T on every click/)).toBeInTheDocument();
    // Taps need to know where the click is. The square only lights once that has been read,
    // however long a slow computer takes over it.
    const square = screen.getByRole("img", { name: "Flashes on each click" });
    await waitFor(() => expect(square).toHaveAttribute("data-lit", "true"), { timeout: 5000 });
    for (let i = 0; i < 6; i++) fireEvent.keyDown(window, { key: " " });
    const use = await screen.findByRole("button", { name: /^Use / });
    const suggested = Number(use.textContent!.replace(/^Use \+?/, "").replace(" ms", ""));
    await user.click(use);
    expect(usePreviewSync.getState().offsetMs).toBe(suggested);

    await user.click(screen.getByRole("button", { name: "Stop the click" }));
    expect(backend.calls).toContain("syncClickStop");
    await user.click(screen.getByRole("button", { name: "Reset" }));
    expect(usePreviewSync.getState().offsetMs).toBe(0);
  });

  it("shows Cisco's video encoder, downloads it only after asking, and turns it off and removes it", async () => {
    const user = await startFresh();
    const seq = new MemorySequencer(useApp.getState().backend as MemoryBackend);
    await useSequencer.getState().connect(seq);
    useApp.getState().setScreen("settings");
    expect(await screen.findByText(/^Not downloaded\. PixelFlow asks before downloading it \(about 0\.5 MB from ciscobinary\.openh264\.org\)/)).toBeInTheDocument();
    // Where it's turned on and off, Cisco's line, and its licence to read.
    expect(screen.getByText("OpenH264 Video Codec provided by Cisco Systems, Inc.")).toBeInTheDocument();
    await user.click(screen.getByText("Cisco's licence for the encoder"));
    expect(screen.getByText(/About The Cisco-Provided Binary of OpenH264 Video Codec/)).toBeVisible();

    // Turned on, it asks first: cancelled, nothing is downloaded.
    const use = screen.getByRole("checkbox", { name: "Use Cisco's OpenH264 video encoder" });
    expect(use).not.toBeChecked();
    await user.click(use);
    const ask = await screen.findByRole("alertdialog", { name: "Download Cisco's video encoder?" });
    expect(within(ask).getByText(/ciscobinary\.openh264\.org, about 0\.5 MB/)).toBeInTheDocument();
    expect(within(ask).getByText("OpenH264 Video Codec provided by Cisco Systems, Inc.")).toBeInTheDocument();
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(seq.calls).not.toContain("downloadH264Encoder");
    expect(use).not.toBeChecked();

    // Agreed: downloaded and on.
    await user.click(use);
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Download" }));
    expect(await screen.findByText("Downloaded (version 2.6.0). Video export uses it.")).toBeInTheDocument();
    expect(use).toBeChecked();

    // Off, kept downloaded; then removed.
    await user.click(use);
    expect(await screen.findByText(/turned off: video export asks before using it/)).toBeInTheDocument();
    expect(seq.video.h264State).toBe("off");
    await user.click(screen.getByRole("button", { name: "Remove the encoder" }));
    expect(await screen.findByText(/^Not downloaded\./)).toBeInTheDocument();
    expect(seq.video.h264State).toBe("missing");
    expect(seq.video.h264Downloads).toBe(1);
  });

  it("says why the encoder couldn't be downloaded, with a way to try again", async () => {
    const user = await startFresh();
    const seq = new MemorySequencer(useApp.getState().backend as MemoryBackend);
    seq.video.h264Failure = "The file from Cisco isn't the one PixelFlow expects (its SHA-256 doesn't match), so it was thrown away.";
    await useSequencer.getState().connect(seq);
    useApp.getState().setScreen("settings");
    await user.click(await screen.findByRole("checkbox", { name: "Use Cisco's OpenH264 video encoder" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Download" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("its SHA-256 doesn't match");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Downloaded (version 2.6.0). Video export uses it.")).toBeInTheDocument();
  });
});
