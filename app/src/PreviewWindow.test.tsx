// The preview's own window: the page opened with `?window=preview`. It draws only the preview,
// follows the main window's note and the engine, and changes nothing itself.

import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PreviewWindow, connectPreviewWindow } from "./PreviewWindow";
import { appWindow } from "./api/appWindow";
import { previewPageUrl } from "./api/browserPreview";
import { demoShow } from "./api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "./api/demoSequence";
import { MemoryBackend } from "./api/memory";
import { MemorySequencer } from "./api/memorySequencer";
import { isPreviewWindow } from "./api/tauri";
import type { Scene3d } from "./components/layout3d/scene";
import type { PreviewFeed, PreviewReply } from "./state/previewWindow";
import { usePreviewSync } from "./state/previewSync";
import { useView3d } from "./state/view3d";

// The 3D view draws with WebGL, which jsdom hasn't: a stand-in scene.
vi.mock("./components/layout3d/threeScene", () => ({
  measureModel: async () => ({ min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } }),
  createThreeScene: (): Scene3d =>
    ({
      resize() {},
      setPixels() {},
      updatePixels() {},
      setFloods() {},
      setColors() {},
      setLook() {},
      setBackdrop() {},
      setModel: async () => null,
      placeModel: () => null,
      surfaceAt: () => null,
      setSelectionBox() {},
      setGizmo() {},
      setOptions() {},
      render() {},
      dispose() {},
    }) as Scene3d,
}));

const FEED: PreviewFeed = {
  source: "sequence",
  playheadMs: 12_000,
  durationMs: 60_000,
  stamp: "1:1:0",
  only: null,
  showRevision: 0,
  title: "Chimes Medley 2020",
  transport: true,
  mode: "2d",
  glow: 0,
  syncMs: 0,
};

/** The preview's window on the demo show and sequence, with the main window's note saying `feed`. */
async function openWindow(feed: Partial<PreviewFeed> = {}) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await backend.openPreviewWindow();
  /** What the main window says next. */
  const say = (next: Partial<PreviewFeed>) => backend.previewLink("main", { ...FEED, ...feed, ...next });
  await say({});
  const frames = vi.spyOn(seq, "sequenceDocFrame");
  await connectPreviewWindow(backend, seq);
  const user = userEvent.setup();
  render(<PreviewWindow />);
  /** What this window last told the main one. */
  const said = async () => (await backend.previewLink("main")).theirs as PreviewReply | null;
  return { backend, seq, say, said, frames, user };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.documentElement.removeAttribute("data-theme");
});

describe("the preview's own window", () => {
  it("is the app's page with a flag, in a browser", () => {
    expect(previewPageUrl("http://localhost:5198/?demo")).toBe("http://localhost:5198/?demo=&window=preview");
    expect(previewPageUrl("http://localhost:1420/index.html#top")).toBe("http://localhost:1420/index.html?window=preview");
    expect(isPreviewWindow("?demo=&window=preview")).toBe(true);
    expect(isPreviewWindow("?window=preview")).toBe(true);
    expect(isPreviewWindow("?demo")).toBe(false);
    expect(isPreviewWindow("")).toBe(false);
  });

  it("draws only the preview: the open sequence at the main window's playhead", async () => {
    const { backend, seq, frames } = await openWindow();
    expect(await screen.findByRole("img", { name: "Preview of the show at the playhead" })).toBeInTheDocument();
    await waitFor(() => expect(frames).toHaveBeenCalledWith(12_000));
    expect(screen.getByRole("heading", { name: "Chimes Medley 2020" })).toBeInTheDocument();
    expect(screen.getByText("0:12 / 1:00")).toBeInTheDocument();
    // None of the app around it.
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
    expect(screen.queryByRole("toolbar")).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Transport" })).not.toBeInTheDocument();
    expect(screen.queryByRole("contentinfo")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Commands|Assistant|Save/ })).not.toBeInTheDocument();
    expect(document.title).toBe("PixelFlow Preview");
    // It only looked: nothing was asked of the engine but what it draws.
    expect(backend.calls).toEqual(["openPreviewWindow"]);
    expect(seq.calls).toEqual(["openSequenceDoc"]);
  });

  it("follows the main window's playhead, and draws again when the sequence was edited", async () => {
    const { say, frames } = await openWindow();
    await waitFor(() => expect(frames).toHaveBeenCalledWith(12_000));
    await say({ playheadMs: 30_000 });
    await waitFor(() => expect(frames).toHaveBeenCalledWith(30_000));
    expect(await screen.findByText("0:30 / 1:00")).toBeInTheDocument();
    const asked = frames.mock.calls.length;
    await say({ playheadMs: 30_000, stamp: "1:2:0" });
    await waitFor(() => expect(frames.mock.calls.length).toBeGreaterThan(asked));
    expect(frames).toHaveBeenLastCalledWith(30_000);
  });

  it("follows the engine's clock while the sequence plays, whoever started it", async () => {
    const { seq, backend, frames } = await openWindow({ playheadMs: 0 });
    await waitFor(() => expect(frames).toHaveBeenCalledWith(0));
    expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
    // The main window starts it.
    await act(async () => void (await seq.playSequenceDoc(20_000)));
    expect(await screen.findByRole("button", { name: "Pause" })).toBeInTheDocument();
    await waitFor(() => expect(frames.mock.calls.some(([ms]) => ms >= 20_000)).toBe(true));
    await act(() => backend.stopPlayback());
    expect(await screen.findByRole("button", { name: "Play" })).toBeInTheDocument();
  });

  it("shows the lights live when that's what the main window's preview shows", async () => {
    const { backend, frames } = await openWindow({ source: "live", title: "Wizards", transport: false });
    const live = vi.spyOn(backend, "liveFrame");
    expect(await screen.findByRole("img", { name: "Preview of the lights" })).toBeInTheDocument();
    await waitFor(() => expect(live).toHaveBeenCalled());
    expect(frames).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: "Wizards" })).toBeInTheDocument();
    // No transport in front in the main window: nothing here to play or pause it with.
    expect(screen.queryByRole("button", { name: "Play" })).not.toBeInTheDocument();
  });

  it("looks the way the main window's preview does, and says what's chosen here", async () => {
    const { say, said, user } = await openWindow({ mode: "3d", glow: 0.5 });
    expect(await screen.findByRole("application", { name: "3D preview" })).toBeInTheDocument();
    expect(useView3d.getState().glow).toBe(0.5);
    // Nothing chosen here yet: its note says what the main window's did.
    await waitFor(async () => expect(await said()).toEqual({ mode: "3d", glow: 0.5, toggles: 0 }));

    await user.click(screen.getByRole("button", { name: "2D" }));
    expect(await screen.findByRole("img", { name: "Preview of the show at the playhead" })).toBeInTheDocument();
    await waitFor(async () => expect(await said()).toEqual({ mode: "2d", glow: 0.5, toggles: 0 }));
    // The main window moving its playhead doesn't undo that…
    await say({ mode: "3d", glow: 0.5, playheadMs: 13_000 });
    await screen.findByText("0:13 / 1:00");
    expect(screen.getByRole("img", { name: "Preview of the show at the playhead" })).toBeInTheDocument();
    // …but a new choice there is taken on, and so is the viewer's preview sync offset.
    await say({ mode: "2d", glow: 0.25, playheadMs: 13_000, syncMs: -40 });
    await waitFor(() => expect(useView3d.getState().glow).toBe(0.25));
    expect(usePreviewSync.getState().offsetMs).toBe(-40);
  });

  it("asks the main window to play or pause: it doesn't touch playback itself", async () => {
    const { backend, seq, said, user } = await openWindow();
    await user.click(await screen.findByRole("button", { name: "Play" }));
    await waitFor(async () => expect((await said())?.toggles).toBe(1));
    await user.keyboard(" ");
    await waitFor(async () => expect((await said())?.toggles).toBe(2));
    expect(await backend.playbackStatus()).toBeNull();
    expect(seq.calls).toEqual(["openSequenceDoc"]);
  });

  it("fills its own display, and leaves that with Escape or however the system does it", async () => {
    const fill = vi.spyOn(appWindow, "setFullscreen").mockResolvedValue();
    let changed: (on: boolean) => void = () => {};
    vi.spyOn(appWindow, "onFullscreenChange").mockImplementation((handler) => {
      changed = handler;
      return () => {};
    });
    const { user } = await openWindow();
    await user.click(screen.getByRole("button", { name: "Full screen" }));
    expect(fill).toHaveBeenLastCalledWith(true);
    expect(screen.getByRole("button", { name: "Leave full screen" })).toHaveAttribute("aria-pressed", "true");
    await user.keyboard("{Escape}");
    expect(fill).toHaveBeenLastCalledWith(false);
    await user.keyboard("f");
    expect(fill).toHaveBeenLastCalledWith(true);
    // The system's own button took it out of full screen.
    act(() => changed(false));
    expect(screen.getByRole("button", { name: "Full screen" })).toHaveAttribute("aria-pressed", "false");
  });

  it("fetches the show again when the main window says it changed", async () => {
    const { backend, say } = await openWindow();
    const snapshots = vi.spyOn(backend, "getSnapshot");
    await screen.findByText("0:12 / 1:00");
    const before = snapshots.mock.calls.length;
    await say({ showRevision: 7 });
    await waitFor(() => expect(snapshots.mock.calls.length).toBeGreaterThan(before));
  });
});
