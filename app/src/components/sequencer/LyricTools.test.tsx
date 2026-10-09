import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import type { Mark, TimingTrack } from "../../api/sequence";
import { playClock } from "../../state/previewSync";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";

// The timeline is 1000 × 600 px at the window's corner: the demo's minute fits at 60 ms per pixel.
// Above the rows: ruler 24 + music 44, then 18 px per timing track: Beats, Bars, then the lyrics
// tracks added here (lines, words, syllables, phonemes).
const MS_PER_PX = 60;
const x = (ms: number) => ms / MS_PER_PX;
const Y = { lines: 113, words: 131, syllables: 149 };

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

const mark = (startMs: number, endMs: number, label: string): Mark => ({ startMs, endMs, label });
const lyricsTrack = (name: string, kind: TimingTrack["kind"], marks: Mark[]): TimingTrack => ({ id: crypto.randomUUID(), name, kind, marks });

/** The demo sequence with a line of lyrics: "hello there", its words, syllables, and mouth shapes. */
async function openScreen() {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  const doc = demoSequence(show, 60_000);
  doc.timingTracks.push(
    lyricsTrack("Lyrics", "lyrics", [mark(1000, 3000, "hello there")]),
    lyricsTrack("Lyrics (words)", "words", [mark(1000, 2000, "hello"), mark(2000, 3000, "there")]),
    lyricsTrack("Lyrics (syllables)", "custom", [mark(1000, 1400, "hel"), mark(1400, 2000, "lo"), mark(2000, 3000, "there")]),
    lyricsTrack("Lyrics (phonemes)", "phonemes", [mark(1000, 1400, "E"), mark(1400, 2000, "O"), mark(2000, 3000, "AI")]),
  );
  seq.files.set(DEMO_SEQUENCE_PATH, doc);
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  // Lyrics marks snap to the voice: wait for it.
  await waitFor(() => expect(backend.calls.some((c) => c.startsWith("vocalLane:"))).toBe(true));
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  return { backend, seq, user };
}

const timeline = () => screen.getByRole("application", { name: "Timeline" });
const track = (seq: MemorySequencer, name: string) => seq.doc!.timingTracks.find((t) => t.name === name)!;
const spans = (t: TimingTrack) => t.marks.map((m) => [m.startMs, m.endMs, m.label]);

function drag(el: Element, from: [number, number], to: [number, number], init: Partial<PointerEventInit> = {}) {
  fireEvent.pointerDown(el, { clientX: from[0], clientY: from[1], button: 0, pointerId: 1, ...init });
  fireEvent.pointerMove(el, { clientX: (from[0] + to[0]) / 2, clientY: (from[1] + to[1]) / 2, pointerId: 1, ...init });
  fireEvent.pointerMove(el, { clientX: to[0], clientY: to[1], pointerId: 1, ...init });
  fireEvent.pointerUp(el, { clientX: to[0], clientY: to[1], pointerId: 1, ...init });
}

async function openMenu(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(screen.getByRole("button", { name: `${name} menu` }));
  return screen.getByRole("menu", { name: `${name} menu` });
}

describe("precise lyric editing", () => {
  it("shows the isolated vocals under the timing tracks from a lyrics track's menu, and hides it", async () => {
    const { user } = await openScreen();
    expect(screen.queryByText("Vocals (isolated)")).not.toBeInTheDocument();
    await user.click(within(await openMenu(user, "Lyrics (words)")).getByRole("menuitem", { name: "Show vocals lane" }));
    expect(screen.getByText("Vocals (isolated)")).toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.vocalsLane")).toBe("true");
    expect(within(await openMenu(user, "Lyrics")).getByRole("menuitem", { name: "Hide vocals lane" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Hide the vocals lane" }));
    expect(screen.queryByText("Vocals (isolated)")).not.toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.vocalsLane")).toBe("false");
  });

  it("moving a word takes its syllables and mouth shapes along, as one undo step", async () => {
    const { seq, user } = await openScreen();
    // Alt: no snapping, so it moves by exactly 600 ms.
    drag(timeline(), [x(1500), Y.words], [x(900), Y.words], { altKey: true });
    await waitFor(() => expect(spans(track(seq, "Lyrics (words)"))[0]).toEqual([400, 1400, "hello"]));
    expect(spans(track(seq, "Lyrics (syllables)"))).toEqual([
      [400, 800, "hel"],
      [800, 1400, "lo"],
      [2000, 3000, "there"],
    ]);
    expect(spans(track(seq, "Lyrics (phonemes)")).slice(0, 2)).toEqual([
      [400, 800, "E"],
      [800, 1400, "O"],
    ]);
    await user.click(screen.getByRole("button", { name: "Undo (sequence)" }));
    await waitFor(() => expect(spans(track(seq, "Lyrics (words)"))[0]).toEqual([1000, 2000, "hello"]));
    expect(spans(track(seq, "Lyrics (syllables)"))[0]).toEqual([1000, 1400, "hel"]);
    expect(spans(track(seq, "Lyrics (phonemes)"))[0]).toEqual([1000, 1400, "E"]);
  });

  it("stretching a word stretches its syllables, and edges snap to where the voice starts a sound", async () => {
    const { backend, seq } = await openScreen();
    // The memory backend's voice starts sounds at 1000, 1260, 1573, … ms.
    const onset = backend.vocalLaneFor("").onsets[1];
    expect(onset % 25).not.toBe(0);
    drag(timeline(), [x(1000), Y.words], [x(onset), Y.words]);
    await waitFor(() => expect(spans(track(seq, "Lyrics (words)"))[0]).toEqual([onset, 2000, "hello"]));
    const scale = (2000 - onset) / 1000;
    expect(spans(track(seq, "Lyrics (syllables)"))[0]).toEqual([onset, Math.round(onset + 400 * scale), "hel"]);
    // Alt places it freely, on the frame grid.
    drag(timeline(), [x(onset), Y.words], [x(onset + 100), Y.words], { altKey: true });
    await waitFor(() => expect(track(seq, "Lyrics (words)").marks[0].startMs % 25).toBe(0));
  });

  it("zooms to a word's line from its right-click menu", async () => {
    const { user } = await openScreen();
    fireEvent.contextMenu(timeline(), { clientX: x(1500), clientY: Y.words });
    await user.click(within(screen.getByRole("menu", { name: "Mark 'hello'" })).getByRole("menuitem", { name: "Zoom to line" }));
    // The line (1–3 s) with a little room either side.
    expect(screen.getByText("0:00.8 – 0:03.2")).toBeInTheDocument();
  });

  it("re-times words by tapping along, slowed down, as one undo step with the syllables following", async () => {
    const { backend, seq, user } = await openScreen();
    await user.click(within(await openMenu(user, "Lyrics (words)")).getByRole("menuitem", { name: "Tap timing…" }));
    const hud = screen.getByRole("dialog", { name: "Tap timing" });
    await user.click(within(hud).getByRole("button", { name: "50%" }));
    await user.click(within(hud).getByRole("button", { name: "Start" }));
    await waitFor(() => expect(useSequencer.getState().status?.state).toBe("playing"));
    await waitFor(() => expect(backend.calls).toContain("setPlaybackSpeed:0.5"));
    expect(within(hud).getByText("“hello”")).toBeInTheDocument();

    // Where the music is heard as each key goes down.
    let heard = 1100;
    vi.spyOn(playClock, "musicAt").mockImplementation(() => heard);
    fireEvent.keyDown(document.body, { key: " " });
    fireEvent.keyUp(document.body, { key: " " });
    expect(within(hud).getByText("“there”")).toBeInTheDocument();
    heard = 2050;
    fireEvent.keyDown(document.body, { key: "j" });
    fireEvent.keyUp(document.body, { key: "j" });
    expect(within(hud).getByText(/Every word is timed/)).toBeInTheDocument();
    // Space taps rather than pausing.
    expect(useSequencer.getState().status?.state).toBe("playing");
    fireEvent.keyDown(document.body, { key: "Escape" });

    await waitFor(() => expect(spans(track(seq, "Lyrics (words)"))).toEqual([[1100, 2050, "hello"], [2050, 3050, "there"]]));
    expect(screen.queryByRole("dialog", { name: "Tap timing" })).not.toBeInTheDocument();
    expect(spans(track(seq, "Lyrics (syllables)"))).toEqual([
      [1100, 1480, "hel"],
      [1480, 2050, "lo"],
      [2050, 3050, "there"],
    ]);
    expect(spans(track(seq, "Lyrics (phonemes)"))[2]).toEqual([2050, 3050, "AI"]);
    await waitFor(() => expect(useSequencer.getState().status?.state).toBe("paused"));
    expect(backend.calls).toContain("setPlaybackSpeed:1");

    await user.click(screen.getByRole("button", { name: "Undo (sequence)" }));
    await waitFor(() => expect(spans(track(seq, "Lyrics (words)"))).toEqual([[1000, 2000, "hello"], [2000, 3000, "there"]]));
    expect(spans(track(seq, "Lyrics (syllables)"))[0]).toEqual([1000, 1400, "hel"]);
  });

  it("closing tap timing before tapping changes nothing", async () => {
    const { seq, user } = await openScreen();
    const before = spans(track(seq, "Lyrics (words)"));
    fireEvent.contextMenu(timeline(), { clientX: x(2500), clientY: Y.words });
    await user.click(within(screen.getByRole("menu", { name: "Mark 'there'" })).getByRole("menuitem", { name: "Tap timing from here…" }));
    const hud = screen.getByRole("dialog", { name: "Tap timing" });
    expect(within(hud).getByText(/from 0:02\.0/)).toBeInTheDocument();
    await user.click(within(hud).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: "Tap timing" })).not.toBeInTheDocument();
    expect(spans(track(seq, "Lyrics (words)"))).toEqual(before);
    expect(useSequencer.getState().status).toBeNull();
  });
});
