import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import type { Sequence } from "../../api/sequence";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    width: 1000,
    height: 600,
    right: 1000,
    bottom: 600,
    toJSON: () => ({}),
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

/** The Sequence screen on the demo sequence (with `saved` as its file has it), with its settings
 * panel open. */
async function openSettings(saved: Partial<Sequence> = {}) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, { ...demoSequence(show, 60_000), ...saved });
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  await user.click(settingsButton());
  return { backend, seq, user };
}

const settingsButton = () => screen.getByRole("button", { name: "Sequence settings" });
const panel = () => screen.getByRole("dialog", { name: "Sequence settings" });
const choice = (name: RegExp) => within(panel()).getByRole("radio", { name });

describe("Sequence settings", () => {
  it("changes how rows meet the rows above them, as one undo step", async () => {
    const { seq, user } = await openSettings();
    // A sequence made in PixelFlow has rows that cover.
    expect(choice(/^Cover/)).toBeChecked();
    await user.click(choice(/^Blend with the rows above/));
    await waitFor(() => expect(seq.doc!.rowBlending).toBe("blend"));
    expect(useSequencer.getState().doc!.rowBlending).toBe("blend");
    expect(choice(/^Blend with the rows above/)).toBeChecked();
    const undo = screen.getByRole("button", { name: "Undo (sequence)" });
    expect(undo).toHaveAttribute("data-tip", "Undo (sequence): Change how rows overlap");
    // A click outside the panel closes it.
    await user.click(undo);
    await waitFor(() => expect(seq.doc).not.toHaveProperty("rowBlending"));
    expect(screen.queryByRole("dialog", { name: "Sequence settings" })).toBeNull();
    await user.click(settingsButton());
    expect(choice(/^Cover/)).toBeChecked();
  });

  it("changes which channel of the music is how loud it is, as one undo step", async () => {
    const { seq, user } = await openSettings();
    const level = () => within(panel()).getByRole("combobox", { name: "Loudness from" });
    // A sequence made in PixelFlow follows the louder channel, and the file doesn't say so.
    expect(level()).toHaveValue("louderChannel");
    expect(within(level()).getByRole("option", { name: "The louder channel" })).toBeInTheDocument();
    expect(level()).toHaveAccessibleDescription(/Sequences imported from xLights use the left channel, so they play as they did there\./);
    await user.selectOptions(level(), "The left channel");
    await waitFor(() => expect(seq.doc!.musicLevelSource).toBe("leftChannel"));
    expect(useSequencer.getState().doc!.musicLevelSource).toBe("leftChannel");
    expect(level()).toHaveValue("leftChannel");
    // The panel stays open, and how rows meet is as it was.
    expect(choice(/^Cover/)).toBeChecked();
    const undo = screen.getByRole("button", { name: "Undo (sequence)" });
    expect(undo).toHaveAttribute("data-tip", "Undo (sequence): Change where the music's loudness comes from");
    await user.click(undo);
    await waitFor(() => expect(seq.doc).not.toHaveProperty("musicLevelSource"));
    await user.click(settingsButton());
    expect(level()).toHaveValue("louderChannel");
  });

  it("opens a sequence imported from xLights on the left channel", async () => {
    await openSettings({ musicLevelSource: "leftChannel" });
    expect(within(panel()).getByRole("combobox", { name: "Loudness from" })).toHaveValue("leftChannel");
  });

  it("shows the frame time and the music, which Locate… points at the file chosen", async () => {
    const { backend, seq, user } = await openSettings();
    expect(within(panel()).getByText("25 ms apart (40 a second)")).toBeInTheDocument();
    expect(within(panel()).getByText("Chimes Medley 2020.mp3")).toBeInTheDocument();
    backend.nextLocatePath = "/Music/Medley.mp3";
    await user.click(within(panel()).getByRole("button", { name: "Locate Chimes Medley 2020.mp3" }));
    await waitFor(() => expect(seq.doc!.audio).toBe("/Music/Medley.mp3"));
    expect(within(panel()).getByText("Medley.mp3")).toBeInTheDocument();
  });

  it("opens on the chosen way, and Escape closes it and goes back to its button", async () => {
    const { user } = await openSettings();
    expect(choice(/^Cover/)).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Sequence settings" })).toBeNull();
    expect(settingsButton()).toHaveFocus();
  });
});
