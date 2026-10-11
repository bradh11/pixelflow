// The preview in a window of its own, as the main window sees it: the note left where a preview
// was, what the main window tells the other window, and what it takes on from it. (What the other
// window draws is PreviewWindow.test.tsx's.)

import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import { usePreviewSync } from "../../state/previewSync";
import { type PreviewFeed, type PreviewReply, usePreviewWindow } from "../../state/previewWindow";
import { useSequencer } from "../../state/sequencer";
import { type Screen, useApp } from "../../state/store";
import { useView3d } from "../../state/view3d";

/** The app on `screenName`, with the demo sequence open in the editor. */
async function openApp(screenName: Screen, setup?: (backend: MemoryBackend) => void) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  setup?.(backend);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: screenName });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  return { backend, seq, user };
}

/** What the main window last told the preview's window. */
const feed = async (backend: MemoryBackend) => (await backend.previewLink("preview")).theirs as PreviewFeed;
/** Says something from the preview's window. */
const reply = (backend: MemoryBackend, note: PreviewReply) => backend.previewLink("preview", note);

const preview = () => screen.getByRole("region", { name: "Preview" });
const picture = () => screen.queryByRole("img", { name: "Preview of the show at the playhead" });

describe("the preview in its own window, from the Sequence screen", () => {
  it("leaves a slim note above the timeline, and comes back when asked", async () => {
    const { backend, user } = await openApp("sequence");
    expect(picture()).toBeInTheDocument();
    expect(screen.getByRole("separator", { name: "Preview size" })).toBeInTheDocument();
    await user.click(within(preview()).getByRole("button", { name: "Pop out the preview" }));
    expect(backend.calls).toContain("openPreviewWindow");
    expect(preview()).toHaveTextContent("Preview is in its own window");
    // The room goes to the timeline: no picture here, and nothing to resize.
    expect(picture()).not.toBeInTheDocument();
    expect(screen.queryByRole("separator", { name: "Preview size" })).not.toBeInTheDocument();
    expect(within(preview()).queryByRole("button", { name: "Pop out the preview" })).not.toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.previewPoppedOut")).toBe("true");

    await user.click(within(preview()).getByRole("button", { name: "Bring it back" }));
    expect(backend.calls).toContain("closePreviewWindow");
    expect(picture()).toBeInTheDocument();
    expect(screen.getByRole("separator", { name: "Preview size" })).toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.previewPoppedOut")).toBeNull();
  });

  it("comes back when its window is closed there", async () => {
    const { backend, user } = await openApp("sequence");
    await user.click(within(preview()).getByRole("button", { name: "Pop out the preview" }));
    expect(picture()).not.toBeInTheDocument();
    backend.previewWindow!.close();
    await waitFor(() => expect(picture()).toBeInTheDocument());
    expect(usePreviewWindow.getState().poppedOut).toBe(false);
    expect(localStorage.getItem("pixelflow.previewPoppedOut")).toBeNull();
  });

  it("tells the window what to show: the open sequence at the playhead, and how", async () => {
    const { backend, user } = await openApp("sequence");
    const name = useSequencer.getState().doc!.name;
    await user.click(within(preview()).getByRole("button", { name: "Pop out the preview" }));
    // Said before the window opens, so it has something to draw at once.
    expect(await feed(backend)).toMatchObject({ source: "sequence", playheadMs: 0, durationMs: 60_000, title: name, transport: true, mode: "2d", glow: 0, syncMs: 0, only: null });

    await act(() => useSequencer.getState().seek(12_000));
    await waitFor(async () => expect((await feed(backend)).playheadMs).toBe(12_000));
    // An edit changes what's at the playhead without moving it: the window is told to draw again.
    const before = (await feed(backend)).stamp;
    const row = useSequencer.getState().doc!.rows[0];
    await act(() => useSequencer.getState().edit([{ type: "removeRow", id: row.id }]));
    await waitFor(async () => expect((await feed(backend)).stamp).not.toBe(before));

    // "Selected row only" stays here, with the note, and reaches the window as that row's target.
    const target = useSequencer.getState().doc!.rows[0];
    act(() => useSequencer.getState().setActiveRow(target.id));
    await user.click(within(preview()).getByRole("checkbox", { name: "Selected row only" }));
    await waitFor(async () => expect((await feed(backend)).only).toEqual(target.target));

    // Glow, 2D | 3D, and the preview sync offset chosen here go too.
    act(() => useView3d.getState().setGlow(0.5));
    act(() => useView3d.getState().setSequenceMode("3d"));
    act(() => usePreviewSync.getState().setOffset(60));
    await waitFor(async () => expect(await feed(backend)).toMatchObject({ glow: 0.5, mode: "3d", syncMs: 60 }));
  });

  it("takes on what's chosen in the window, and plays or pauses when asked there", async () => {
    const { backend, seq, user } = await openApp("sequence");
    await user.click(within(preview()).getByRole("button", { name: "Pop out the preview" }));
    await reply(backend, { mode: "2d", glow: 0, toggles: 0 });
    await reply(backend, { mode: "3d", glow: 0.75, toggles: 0 });
    await waitFor(() => expect(useView3d.getState().sequenceMode).toBe("3d"));
    expect(useView3d.getState().glow).toBe(0.75);
    // The Play screen's own choice is left alone: the Sequence screen is in front.
    expect(useView3d.getState().playMode).toBe("2d");
    expect(seq.calls).not.toContain("playSequenceDoc@0");

    await reply(backend, { mode: "3d", glow: 0.75, toggles: 1 });
    await waitFor(() => expect(useSequencer.getState().status?.state).toBe("playing"));
    expect(seq.calls).toContain("playSequenceDoc@0");
    await reply(backend, { mode: "3d", glow: 0.75, toggles: 2 });
    await waitFor(() => expect(useSequencer.getState().status?.state).toBe("paused"));
    await act(() => useSequencer.getState().stop());
  });

  it("opens again when it was open as PixelFlow last closed", async () => {
    localStorage.setItem("pixelflow.previewPoppedOut", "true");
    const { backend } = await openApp("sequence");
    await waitFor(() => expect(backend.calls).toContain("openPreviewWindow"));
    expect(await screen.findByText("Preview is in its own window")).toBeInTheDocument();
  });

  it("says so when the window can't be opened, and stays as it was", async () => {
    const { backend, user } = await openApp("sequence", (b) => {
      b.openWindow = () => null;
    });
    await user.click(within(preview()).getByRole("button", { name: "Pop out the preview" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The preview's window couldn't be opened");
    expect(backend.previewWindow).toBeNull();
    expect(picture()).toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.previewPoppedOut")).toBeNull();
  });
});

describe("the preview in its own window, from the Layout screen", () => {
  it("shows the lights live beside the editor, which stays as it is", async () => {
    const { backend, user } = await openApp("layout");
    const button = screen.getByRole("button", { name: "Preview window" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    await user.click(button);
    expect(backend.calls).toContain("openPreviewWindow");
    expect(button).toHaveAttribute("aria-pressed", "true");
    // The canvas is the editor: nothing is put away here.
    expect(screen.getByRole("application", { name: "Layout canvas" })).toBeInTheDocument();
    // What plays or is being tested, not a sequence's playhead; nothing here to play or pause.
    expect(await feed(backend)).toMatchObject({ source: "live", transport: false, only: null });

    // The window's 2D | 3D is its own: the editor's view doesn't follow it.
    await reply(backend, { mode: "2d", glow: 0, toggles: 0 });
    await reply(backend, { mode: "3d", glow: 0, toggles: 0 });
    await waitFor(() => expect(useView3d.getState().playMode).toBe("3d"));
    expect(useView3d.getState().mode).toBe("2d");

    await user.click(button);
    expect(backend.calls).toContain("closePreviewWindow");
    expect(button).toHaveAttribute("aria-pressed", "false");
  });

  it("follows the screen in front: the open sequence on the Sequence screen, the lights live elsewhere", async () => {
    const { backend } = await openApp("layout");
    await act(() => usePreviewWindow.getState().popOut());
    expect((await feed(backend)).source).toBe("live");
    act(() => useApp.getState().setScreen("sequence"));
    await waitFor(async () => expect(await feed(backend)).toMatchObject({ source: "sequence", transport: true }));
    act(() => useApp.getState().setScreen("play"));
    // Play shows the open sequence too, ready to play.
    await waitFor(async () => expect(await feed(backend)).toMatchObject({ source: "sequence", transport: true }));
    act(() => useApp.getState().setScreen("wiring"));
    await waitFor(async () => expect(await feed(backend)).toMatchObject({ source: "live", transport: false }));
  });
});
