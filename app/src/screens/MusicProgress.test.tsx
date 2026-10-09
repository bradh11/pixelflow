import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import type { AudioProgress } from "../api/types";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

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

async function openScreen(setUp: (backend: MemoryBackend, seq: MemorySequencer) => void, withSequence = true) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  backend.nextAudioPath = DEMO_MUSIC;
  const seq = new MemorySequencer(backend);
  setUp(backend, seq);
  if (withSequence) {
    seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
    await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  }
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  return { backend, seq, user };
}

describe("progress while music is read", () => {
  it("finds a new sequence's length from the music's header, without reading it through", async () => {
    const { backend, user } = await openScreen((b) => {
      // Reading a song through would be slow here: the dialog never does.
      b.musicReadMs = 5_000;
    }, false);
    const seen: AudioProgress[] = [];
    await backend.onAudioProgress((p) => seen.push(p));
    await user.click(screen.getByText("Start from a song.").closest("button")!);
    const dialog = screen.getByRole("dialog", { name: "New sequence" });
    await user.click(within(dialog).getByRole("button", { name: "Choose music…" }));
    expect(await within(dialog).findByText(/Christmas Medley 2017.mp3 · 1:00/)).toBeInTheDocument();
    expect(backend.calls).toContain(`probeAudio:${DEMO_MUSIC}`);
    expect(seen).toEqual([]);
    expect(within(dialog).queryByRole("progressbar")).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Create" })).toBeEnabled();
  });

  it("shows how far it has got when the music has to be read through to find its length", async () => {
    const { user } = await openScreen((b) => {
      b.probeFromHeader = false;
      b.musicReadMs = 600;
    }, false);
    await user.click(screen.getByText("Start from a song.").closest("button")!);
    const dialog = screen.getByRole("dialog", { name: "New sequence" });
    await user.click(within(dialog).getByRole("button", { name: "Choose music…" }));
    const bar = await within(dialog).findByRole("progressbar", { name: "Reading the music" });
    await waitFor(() => expect(Number(bar.getAttribute("aria-valuenow"))).toBeGreaterThan(0));
    expect(within(dialog).getByRole("button", { name: "Create" })).toBeDisabled();
    expect(await within(dialog).findByText(/Christmas Medley 2017.mp3 · 1:00/, {}, { timeout: 2000 })).toBeInTheDocument();
    expect(within(dialog).queryByRole("progressbar")).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Create" })).toBeEnabled();
  });

  it("shows the waveform being read in the Music row, then the waveform", async () => {
    await openScreen((b) => {
      b.musicReadMs = 600;
    });
    const bar = await screen.findByRole("progressbar", { name: "Reading the music" });
    await waitFor(() => expect(Number(bar.getAttribute("aria-valuenow"))).toBeGreaterThan(0));
    await waitFor(() => expect(screen.queryByRole("progressbar", { name: "Reading the music" })).toBeNull(), { timeout: 2000 });
  });

  it("shows a hairline while the music is got ready for effects in the background", async () => {
    await openScreen((_, seq) => {
      seq.audioTrackMs = 600;
    });
    const bar = await screen.findByRole("progressbar", { name: "Getting the music ready for effects" });
    expect(bar.closest("[title]")?.getAttribute("title")).toMatch(/^Getting the music ready for effects: \d+%/);
    await waitFor(() => expect(screen.queryByRole("progressbar", { name: "Getting the music ready for effects" })).toBeNull(), { timeout: 2000 });
  });

  it("shows how far Detect beats has got under its button", async () => {
    const { user } = await openScreen((_, seq) => {
      seq.analysisDelayMs = 600;
    });
    await user.click(screen.getByRole("button", { name: "Detect beats" }));
    const bar = await screen.findByRole("progressbar", { name: "Finding the beats" });
    await waitFor(() => expect(Number(bar.getAttribute("aria-valuenow"))).toBeGreaterThan(0));
    await waitFor(() => expect(useSequencer.getState().detecting).toBe(false), { timeout: 2000 });
    expect(screen.queryByRole("progressbar", { name: "Finding the beats" })).toBeNull();
    expect(screen.getByRole("button", { name: "Detect beats" })).toBeEnabled();
  });
});
