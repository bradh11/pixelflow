import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import { MemoryVideo } from "../../api/memoryVideo";
import type { Sequence } from "../../api/sequence";
import type { VideoProgress } from "../../api/video";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { useView3d } from "../../state/view3d";
import { qualityLabel, selectionSpan } from "./ExportVideoDialog";

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
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

/** The Sequence screen on the demo sequence (a minute, with music, on a show with a photo). */
async function openScreen({ ffmpeg = null, stepMs = 0 }: { ffmpeg?: string | null; stepMs?: number } = {}) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  backend.nextAudioPath = DEMO_MUSIC;
  const seq = new MemorySequencer(backend);
  seq.video.ffmpeg = ffmpeg;
  seq.video.stepMs = stepMs;
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  await user.click(screen.getByRole("button", { name: "More ways to export" }));
  await user.click(screen.getByRole("menuitem", { name: "Export video…" }));
  const dialog = await screen.findByRole("dialog", { name: "Export video" });
  // The choices arrive (the Export button waits for them).
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Export…" })).toBeEnabled());
  return { seq, user, dialog };
}

describe("Export video", () => {
  it("exports the whole sequence at 1080p with the photo, showing each stage, and says where it went", async () => {
    const { seq, user, dialog } = await openScreen({ stepMs: 2 });
    expect(within(dialog).getByLabelText("Size")).toHaveValue("1080");
    expect(within(dialog).getByLabelText("Frame rate")).toHaveValue("30");
    expect(within(dialog).getByLabelText("What to export")).toHaveDisplayValue("The whole sequence (1:00)");
    expect(within(dialog).getByRole("checkbox", { name: "Show the house photo behind the lights" })).toBeChecked();
    // No ffmpeg installed: only the built-in encoder.
    expect(within(within(dialog).getByLabelText("Encoder")).getAllByRole("option").map((o) => o.textContent)).toEqual(["Built in"]);

    const labels = new Set<string>();
    const exportVideo = seq.video.exportVideo.bind(seq.video);
    vi.spyOn(seq.video, "exportVideo").mockImplementation((path, request, onProgress) =>
      exportVideo(path, request, (p: VideoProgress) => {
        labels.add(p.label);
        onProgress?.(p);
      }),
    );
    await user.click(within(dialog).getByRole("button", { name: "Export…" }));
    expect(await within(dialog).findByRole("progressbar")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(await within(dialog).findByText("Saved Christmas Medley 2017.mp4 (1:00, 1080p).")).toBeInTheDocument();
    expect([...labels]).toEqual(["Rendering frames", "Encoding the sound", "Writing the MP4"]);
    expect(seq.calls).toContain("exportVideo");
    expect(seq.video.requests).toEqual([
      {
        path: "/Users/you/Movies/Christmas Medley 2017.mp4",
        request: { width: 1920, height: 1080, fps: 30, startMs: 0, endMs: null, photo: true, pixelSize: 1, glow: 0, ffmpeg: false },
      },
    ]);
    expect(within(dialog).getByText(/H\.264 \(OpenH264\) · AAC 192 kb\/s/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("dialog", { name: "Export video" })).not.toBeInTheDocument();
  });

  it("remembers the size, rate, dot size, glow, and encoder for next time", async () => {
    const { seq, user, dialog } = await openScreen({ ffmpeg: "x264" });
    await user.selectOptions(within(dialog).getByLabelText("Size"), "720");
    await user.selectOptions(within(dialog).getByLabelText("Frame rate"), "60");
    await user.selectOptions(within(dialog).getByLabelText("Pixel size"), "Large");
    await user.selectOptions(within(dialog).getByLabelText("Encoder"), "ffmpeg (higher quality)");
    await user.click(within(dialog).getByRole("checkbox", { name: "Show the house photo behind the lights" }));
    // Glow starts at none (bare bulbs); slide it up for lights behind diffusers.
    const glow = within(dialog).getByRole("slider", { name: "Glow" });
    expect(glow).toHaveValue("0");
    fireEvent.change(glow, { target: { value: "40" } });
    expect(within(dialog).getByText("40%")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Export…" }));
    expect(await within(dialog).findByText("Saved Christmas Medley 2017.mp4 (1:00, 720p60).")).toBeInTheDocument();
    expect(seq.video.requests[0].request).toEqual({ width: 1280, height: 720, fps: 60, startMs: 0, endMs: null, photo: false, pixelSize: 1.5, glow: 0.4, ffmpeg: true });
    await user.click(within(dialog).getByRole("button", { name: "Done" }));

    await user.click(screen.getByRole("button", { name: "More ways to export" }));
    await user.click(screen.getByRole("menuitem", { name: "Export video…" }));
    const again = await screen.findByRole("dialog", { name: "Export video" });
    await waitFor(() => expect(within(again).getByLabelText("Encoder")).toHaveValue("ffmpeg"));
    expect(within(again).getByLabelText("Size")).toHaveValue("720");
    expect(within(again).getByLabelText("Frame rate")).toHaveValue("60");
    expect(within(again).getByLabelText("Pixel size")).toHaveValue("1.5");
    expect(within(again).getByRole("checkbox", { name: "Show the house photo behind the lights" })).not.toBeChecked();
    expect(within(again).getByRole("slider", { name: "Glow" })).toHaveValue("40");
  });

  it("starts with the glow the previews are shown with, until a glow is chosen here", async () => {
    useView3d.getState().setGlow(0.3);
    const { seq, user, dialog } = await openScreen();
    const glow = within(dialog).getByRole("slider", { name: "Glow" });
    expect(glow).toHaveValue("30");
    expect(within(dialog).getByText("30%")).toBeInTheDocument();
    // Other choices are remembered without fixing the glow.
    await user.selectOptions(within(dialog).getByLabelText("Size"), "720");
    await user.click(within(dialog).getByRole("button", { name: "Export…" }));
    expect(await within(dialog).findByText("Saved Christmas Medley 2017.mp4 (1:00, 720p).")).toBeInTheDocument();
    expect(seq.video.requests[0].request).toMatchObject({ height: 720, glow: 0.3 });
    await user.click(within(dialog).getByRole("button", { name: "Done" }));

    // The previews are turned up: the dialog follows.
    const reopen = async () => {
      await user.click(screen.getByRole("button", { name: "More ways to export" }));
      await user.click(screen.getByRole("menuitem", { name: "Export video…" }));
      return screen.findByRole("dialog", { name: "Export video" });
    };
    act(() => useView3d.getState().setGlow(0.6));
    let again = await reopen();
    expect(within(again).getByLabelText("Size")).toHaveValue("720");
    expect(within(again).getByRole("slider", { name: "Glow" })).toHaveValue("60");

    // A glow chosen here stays, whatever the previews are set to afterwards.
    fireEvent.change(within(again).getByRole("slider", { name: "Glow" }), { target: { value: "10" } });
    await user.click(within(again).getByRole("button", { name: "Close" }));
    act(() => useView3d.getState().setGlow(1));
    again = await reopen();
    expect(within(again).getByRole("slider", { name: "Glow" })).toHaveValue("10");
    // And choosing one here doesn't change the previews.
    expect(useView3d.getState().glow).toBe(1);
  });

  it("cancels, saying no file was written", async () => {
    const { user, dialog } = await openScreen({ stepMs: 30 });
    await user.click(within(dialog).getByRole("button", { name: "Export…" }));
    await within(dialog).findByRole("progressbar");
    // Escape doesn't close the dialog while it works.
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Export video" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(await within(dialog).findByText("Export cancelled. No file was written.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Export…" })).toBeEnabled();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Export video" })).not.toBeInTheDocument();
  });

  it("exports just the selected effects, or a stretch typed in", async () => {
    const { seq, user } = await openScreen();
    // Select two effects, then open the dialog again: it starts on the selection.
    await user.click(screen.getByRole("button", { name: "Close" }));
    const ids = useSequencer
      .getState()
      .doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects))
      .slice(0, 2)
      .map((e) => e.id);
    useSequencer.getState().select(ids);
    await user.click(screen.getByRole("button", { name: "More ways to export" }));
    await user.click(screen.getByRole("menuitem", { name: "Export video…" }));
    const dialog = await screen.findByRole("dialog", { name: "Export video" });
    const span = selectionSpan(useSequencer.getState().doc!, ids)!;
    expect(span.endMs).toBeGreaterThan(span.startMs);
    expect(within(dialog).getByLabelText("What to export")).toHaveValue("selection");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Export…" })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: "Export…" }));
    await within(dialog).findByText(/^Saved /);
    expect(seq.video.requests[0].request).toMatchObject({ startMs: span.startMs, endMs: span.endMs });
    await user.click(within(dialog).getByRole("button", { name: "Done" }));

    await user.click(screen.getByRole("button", { name: "More ways to export" }));
    await user.click(screen.getByRole("menuitem", { name: "Export video…" }));
    const custom = await screen.findByRole("dialog", { name: "Export video" });
    await user.selectOptions(within(custom).getByLabelText("What to export"), "custom");
    const from = within(custom).getByLabelText("From");
    const to = within(custom).getByLabelText("To");
    await user.clear(to);
    await user.type(to, "soon");
    expect(within(custom).getByRole("alert")).toHaveTextContent("Enter times like 1:05");
    expect(within(custom).getByRole("button", { name: "Export…" })).toBeDisabled();
    await user.clear(from);
    await user.type(from, "0:10");
    await user.clear(to);
    await user.type(to, "0:30.5");
    expect(within(custom).queryByRole("alert")).toBeNull();
    await user.click(within(custom).getByRole("button", { name: "Export…" }));
    expect(await within(custom).findByText("Saved Christmas Medley 2017.mp4 (0:21, 1080p).")).toBeInTheDocument();
    expect(seq.video.requests[1].request).toMatchObject({ startMs: 10_000, endMs: 30_500 });
  });

  it("does nothing when the save dialog is cancelled, and shows errors", async () => {
    const { seq, user, dialog } = await openScreen();
    seq.video.nextPath = null;
    await user.click(within(dialog).getByRole("button", { name: "Export…" }));
    expect(seq.calls).not.toContain("exportVideo");
    expect(within(dialog).getByRole("button", { name: "Export…" })).toBeEnabled();

    seq.video.nextPath = "/Movies/x.mp4";
    vi.spyOn(seq.video, "exportVideo").mockRejectedValue(new Error("Couldn't write /Movies/x.mp4: disk full"));
    await user.click(within(dialog).getByRole("button", { name: "Export…" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("disk full");
  });

  it("offers the photo only when the show has one", async () => {
    const { seq, user, dialog } = await openScreen();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    seq.backend!.show.background = null;
    await user.click(screen.getByRole("button", { name: "More ways to export" }));
    await user.click(screen.getByRole("menuitem", { name: "Export video…" }));
    const again = await screen.findByRole("dialog", { name: "Export video" });
    const photo = within(again).getByRole("checkbox", { name: "Show the house photo behind the lights" });
    await waitFor(() => expect(photo).toBeDisabled());
    expect(photo).not.toBeChecked();
  });
});

describe("video export helpers", () => {
  it("names the quality as people say it", () => {
    expect(qualityLabel(1080, 30)).toBe("1080p");
    expect(qualityLabel(720, 60)).toBe("720p60");
  });

  it("finds the selected effects' span", () => {
    const doc = { rows: [{ layers: [{ effects: [{ id: "a", startMs: 500, endMs: 900 }, { id: "b", startMs: 100, endMs: 300 }, { id: "c", startMs: 0, endMs: 5000 }] }] }] } as unknown as Sequence;
    expect(selectionSpan(doc, ["a", "b"])).toEqual({ startMs: 100, endMs: 900 });
    expect(selectionSpan(doc, [])).toBeNull();
  });

  it("the in-memory export checks requests and simulates the stages", async () => {
    const doc = { durationMs: 10_000, audio: null } as unknown as Sequence;
    const video = new MemoryVideo(
      () => doc,
      () => false,
    );
    const ok = { width: 1280, height: 720, fps: 30, startMs: 0, endMs: null, photo: false, pixelSize: 1, glow: 0, ffmpeg: false };
    const stages: string[] = [];
    const summary = await video.exportVideo("/v.mp4", ok, (p) => stages.push(p.stage));
    expect(new Set(stages)).toEqual(new Set(["rendering", "writing"]));
    expect(summary).toMatchObject({ frames: 300, durationMs: 10_000, sound: null, notes: ["The sequence has no music, so the video has no sound."] });
    await expect(video.exportVideo("/v.mp4", { ...ok, fps: 24 })).rejects.toThrow("30 or 60");
    await expect(video.exportVideo("/v.mp4", { ...ok, startMs: 10_000 })).rejects.toThrow("range to export is empty");
    await expect(video.exportVideo("/v.mp4", { ...ok, ffmpeg: true })).rejects.toThrow("ffmpeg isn't installed");
    expect(await video.videoExportChoices()).toEqual({ ffmpeg: null, photo: false });
  });
});
