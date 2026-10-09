import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import type { Mark, TimingTrack } from "../../api/sequence";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";

// The timeline is 1000 × 600 px at the window's corner: the demo's minute fits at 60 ms per pixel.
// Above the rows: ruler 24 + music 44, then 18 px per timing track: Beats 68–86, Bars 86–104, and a
// third track (added by a test) 104–122.
const MS_PER_PX = 60;
const x = (ms: number) => ms / MS_PER_PX;
const TRACK_Y = [77, 95, 113];

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

async function openScreen() {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  return { backend, seq, user };
}

const timeline = () => screen.getByRole("application", { name: "Timeline" });
const track = (seq: MemorySequencer, name: string) => seq.doc!.timingTracks.find((t) => t.name === name)!;
const spans = (t: TimingTrack) => t.marks.map((m) => [m.startMs, m.endMs, m.label]);

/** A third track, "Lyrics", with these marks. */
async function withLyrics(marks: Mark[]) {
  const lyrics: TimingTrack = { id: crypto.randomUUID(), name: "Lyrics", kind: "lyrics", marks };
  await act(() => useSequencer.getState().edit([{ type: "addTimingTrack", track: lyrics }]));
  return lyrics.id;
}

function drag(el: Element, from: [number, number], to: [number, number], init: Partial<PointerEventInit> = {}) {
  fireEvent.pointerDown(el, { clientX: from[0], clientY: from[1], button: 0, pointerId: 1, ...init });
  fireEvent.pointerMove(el, { clientX: (from[0] + to[0]) / 2, clientY: (from[1] + to[1]) / 2, pointerId: 1, ...init });
  fireEvent.pointerMove(el, { clientX: to[0], clientY: to[1], pointerId: 1, ...init });
  fireEvent.pointerUp(el, { clientX: to[0], clientY: to[1], pointerId: 1, ...init });
}

/** Holds the engine's next answer until `release` is called (a slow engine, a queue backing up). */
function holdNextEdit(seq: MemorySequencer) {
  const real = seq.editSequence.bind(seq);
  let release = () => undefined as void;
  const gate = new Promise<void>((resolve) => (release = resolve));
  vi.spyOn(seq, "editSequence").mockImplementationOnce(async (...args) => {
    await gate;
    return real(...args);
  });
  return () => release();
}

describe("timing tracks", () => {
  it("adds a track and taps marks onto it, each tap one undo step", async () => {
    const { seq, user } = await openScreen();
    await user.click(screen.getByRole("button", { name: "Add timing track" }));
    const dialog = screen.getByRole("dialog", { name: "Add timing track" });
    expect(within(dialog).getByRole("textbox", { name: "Name" })).toHaveValue("Lyrics");
    await user.click(within(dialog).getByRole("button", { name: "Add track" }));
    await waitFor(() => expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars", "Lyrics"]));
    // The new track is picked, so T taps onto it.
    expect(useSequencer.getState().activeTrack).toBe(track(seq, "Lyrics").id);
    expect(screen.getByRole("group", { name: "Timing track Lyrics" })).toHaveAttribute("aria-current", "true");

    act(() => useSequencer.getState().setPlayhead(1000));
    await user.keyboard("t");
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[1000, 1500, ""]]));
    act(() => useSequencer.getState().setPlayhead(2200));
    await user.keyboard("t");
    // The second tap ends the first mark and starts the next.
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[1000, 2200, ""], [2200, 2700, ""]]));
    await user.click(screen.getByRole("button", { name: "Undo (sequence)" }));
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[1000, 1500, ""]]));
  });

  it("folds the timing tracks away and back, remembering the choice", async () => {
    await openScreen();
    useSequencer.setState({ timingHidden: false });
    expect(screen.getByText("Beats")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Timing" }));
    expect(useSequencer.getState().timingHidden).toBe(true);
    expect(screen.queryByText("Beats")).not.toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.timingTracksHidden")).toBe("true");
    const unfold = screen.getByRole("button", { name: "2 timing" });
    expect(unfold).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(unfold);
    expect(screen.getByText("Beats")).toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.timingTracksHidden")).toBe("false");
  });

  it("asks for a track to be picked before tapping", async () => {
    const { user } = await openScreen();
    await user.keyboard("t");
    expect(useApp.getState().error).toBe("Pick a timing track first: click its name, then press T in time with the music.");
  });

  it("pastes lyrics, breaks them into words, renames, generates marks, and deletes from the header menu", async () => {
    const { seq, user } = await openScreen();
    await withLyrics([]);
    const openMenu = async (name: string) => {
      await user.click(screen.getByRole("button", { name: `${name} menu` }));
      return screen.getByRole("menu", { name: `${name} menu` });
    };

    await user.click(within(await openMenu("Lyrics")).getByRole("menuitem", { name: "Paste lyrics…" }));
    let dialog = screen.getByRole("dialog", { name: "Paste lyrics onto Lyrics" });
    await user.type(within(dialog).getByRole("textbox", { name: "Lyrics, one phrase per line" }), "Deck the halls{Enter}fa la");
    const from = within(dialog).getByRole("textbox", { name: "From" });
    await user.clear(from);
    await user.type(from, "0:10");
    const to = within(dialog).getByRole("textbox", { name: "To" });
    await user.clear(to);
    await user.type(to, "12");
    await user.click(within(dialog).getByRole("button", { name: "Add 2 lines" }));
    // 12 and 4 letters over two seconds.
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[10_000, 11_500, "Deck the halls"], [11_500, 12_000, "fa la"]]));

    await user.click(within(await openMenu("Lyrics")).getByRole("menuitem", { name: "Break into words" }));
    await waitFor(() => expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars", "Lyrics", "Lyrics (words)"]));
    expect(spans(track(seq, "Lyrics (words)"))).toEqual([
      [10_000, 10_500, "Deck"],
      [10_500, 10_875, "the"],
      [10_875, 11_500, "halls"],
      [11_500, 11_750, "fa"],
      [11_750, 12_000, "la"],
    ]);
    // Adding the words track and its words was one step.
    await user.click(screen.getByRole("button", { name: "Undo (sequence)" }));
    await waitFor(() => expect(seq.doc!.timingTracks).toHaveLength(3));
    await user.click(screen.getByRole("button", { name: "Redo (sequence)" }));
    await waitFor(() => expect(seq.doc!.timingTracks).toHaveLength(4));

    // Double-click a name to rename it.
    await user.dblClick(screen.getByTitle("Bars (Bars) — double-click to rename"));
    const rename = screen.getByRole("textbox", { name: "Rename Bars" });
    await user.clear(rename);
    await user.type(rename, "Measures{Enter}");
    await waitFor(() => expect(seq.doc!.timingTracks[1].name).toBe("Measures"));

    await user.click(within(await openMenu("Lyrics (words)")).getByRole("menuitem", { name: "Generate marks…" }));
    dialog = screen.getByRole("dialog", { name: "Generate marks on Lyrics (words)" });
    const every = within(dialog).getByRole("spinbutton", { name: "Every (ms)" });
    await user.clear(every);
    await user.type(every, "1000");
    await user.clear(within(dialog).getByRole("textbox", { name: "To" }));
    await user.type(within(dialog).getByRole("textbox", { name: "To" }), "3");
    await user.click(within(dialog).getByRole("button", { name: "Generate" }));
    await waitFor(() => expect(track(seq, "Lyrics (words)").marks.slice(0, 3).map((m) => m.startMs)).toEqual([0, 1000, 2000]));
    expect(track(seq, "Lyrics (words)").marks).toHaveLength(8);

    // Every 4th beat, from another track.
    await user.click(within(await openMenu("Lyrics (words)")).getByRole("menuitem", { name: "Generate marks…" }));
    dialog = screen.getByRole("dialog", { name: "Generate marks on Lyrics (words)" });
    await user.click(within(dialog).getByRole("radio", { name: /From another track/ }));
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Track" }), "Beats");
    await user.clear(within(dialog).getByRole("spinbutton", { name: "Take every" }));
    await user.type(within(dialog).getByRole("spinbutton", { name: "Take every" }), "4");
    await user.click(within(dialog).getByRole("button", { name: "Generate" }));
    await waitFor(() => expect(track(seq, "Lyrics (words)").marks).toHaveLength(30));

    await user.click(within(await openMenu("Lyrics (words)")).getByRole("menuitem", { name: "Delete track" }));
    await waitFor(() => expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Measures", "Lyrics"]));
  });

  it("puts pasted lyrics onto the selected marks, and says when the counts don't match", async () => {
    const { seq, user } = await openScreen();
    const id = await withLyrics([
      { startMs: 0, endMs: 900, label: "" },
      { startMs: 1000, endMs: 1900, label: "" },
    ]);
    act(() => useSequencer.getState().selectMarks(id, [0, 1000]));
    await user.click(screen.getByRole("button", { name: "Lyrics menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Paste lyrics…" }));
    const dialog = screen.getByRole("dialog", { name: "Paste lyrics onto Lyrics" });
    expect(within(dialog).getByRole("radio", { name: /Onto the 2 selected marks/ })).toBeChecked();
    const box = within(dialog).getByRole("textbox", { name: "Lyrics, one phrase per line" });
    await user.type(box, "one");
    expect(within(dialog).getByText(/There are 1 lines and 2 selected marks/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Add 1 line" })).toBeDisabled();
    await user.type(box, "{Enter}two");
    await user.click(within(dialog).getByRole("button", { name: "Add 2 lines" }));
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[0, 900, "one"], [1000, 1900, "two"]]));
  });

  it("puts lyrics on the marks that were selected, even when an earlier edit changes the marks first", async () => {
    const { seq, user } = await openScreen();
    const id = await withLyrics([
      { startMs: 0, endMs: 900, label: "" },
      { startMs: 1000, endMs: 1900, label: "" },
      { startMs: 2000, endMs: 2900, label: "" },
    ]);
    act(() => useSequencer.getState().selectMarks(id, [1000, 2000]));
    await user.click(screen.getByRole("button", { name: "Lyrics menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Paste lyrics…" }));
    const dialog = screen.getByRole("dialog", { name: "Paste lyrics onto Lyrics" });
    await user.type(within(dialog).getByRole("textbox", { name: "Lyrics, one phrase per line" }), "one{Enter}two");
    // The first mark goes on its way to the engine before the lyrics are added.
    const release = holdNextEdit(seq);
    const removed = useSequencer.getState().edit([{ type: "removeMarks", track: id, indices: [0] }]);
    await user.click(within(dialog).getByRole("button", { name: "Add 2 lines" }));
    release();
    await act(() => removed);
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[1000, 1900, "one"], [2000, 2900, "two"]]));
  });

  it("breaks lyrics into their own words track, never another lyrics track's", async () => {
    const { seq, user } = await openScreen();
    const add = (t: TimingTrack) => act(() => useSequencer.getState().edit([{ type: "addTimingTrack", track: t }]));
    await add({ id: crypto.randomUUID(), name: "Backing", kind: "lyrics", marks: [{ startMs: 0, endMs: 1000, label: "ooh aah" }] });
    await add({ id: crypto.randomUUID(), name: "Lead", kind: "lyrics", marks: [{ startMs: 0, endMs: 1000, label: "la" }] });
    await add({ id: crypto.randomUUID(), name: "Lead (words)", kind: "words", marks: [{ startMs: 0, endMs: 1000, label: "la" }] });
    await user.click(screen.getByRole("button", { name: "Backing menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Break into words" }));
    await waitFor(() => expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars", "Backing", "Backing (words)", "Lead", "Lead (words)"]));
    expect(spans(track(seq, "Backing (words)"))).toEqual([[0, 500, "ooh"], [500, 1000, "aah"]]);
    expect(spans(track(seq, "Lead (words)"))).toEqual([[0, 1000, "la"]]);
  });

  it("breaks a words track into syllables and mouth shapes, again replacing them", async () => {
    const { seq, user } = await openScreen();
    const add = (t: TimingTrack) => act(() => useSequencer.getState().edit([{ type: "addTimingTrack", track: t }]));
    await add({ id: crypto.randomUUID(), name: "Lead", kind: "lyrics", marks: [{ startMs: 0, endMs: 2000, label: "paper lanterns" }] });
    await add({ id: crypto.randomUUID(), name: "Lead (words)", kind: "words", marks: [{ startMs: 0, endMs: 1000, label: "paper" }, { startMs: 1000, endMs: 2000, label: "lanterns" }] });
    // Only on a words track with words.
    await user.click(screen.getByRole("button", { name: "Lead menu" }));
    expect(screen.queryByRole("menuitem", { name: "Break into syllables" })).toBeNull();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Lead (words) menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Break into syllables" }));
    await waitFor(() => expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars", "Lead", "Lead (words)", "Lead (syllables)", "Lead (phonemes)"]));
    expect(spans(track(seq, "Lead (syllables)"))).toEqual([[0, 500, "pa"], [500, 1000, "per"], [1000, 1500, "lan"], [1500, 2000, "terns"]]);
    expect(track(seq, "Lead (phonemes)").kind).toBe("phonemes");
    expect(seq.calls).toContain(`syllablesFromWords:${track(seq, "Lead (words)").id}`);
    // Again: the same tracks, not copies.
    const syllables = track(seq, "Lead (syllables)").id;
    await user.click(screen.getByRole("button", { name: "Lead (words) menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Break into syllables" }));
    await waitFor(() => expect(seq.calls.filter((c) => c.startsWith("syllablesFromWords")).length).toBe(2));
    expect(seq.doc!.timingTracks).toHaveLength(6);
    expect(track(seq, "Lead (syllables)").id).toBe(syllables);
  });

  it("drags marks and their edges, labels them in place, adds them by double-click, and deletes them", async () => {
    const { seq, user } = await openScreen();
    await withLyrics([{ startMs: 6000, endMs: 9000, label: "Hello" }]);
    const canvas = timeline();
    const y = TRACK_Y[2];
    const before = seq.undoStack.length;
    // Alt turns snapping off: the move lands on the frame grid.
    drag(canvas, [x(7500), y], [x(8700), y], { altKey: true });
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[7200, 10_200, "Hello"]]));
    expect(seq.undoStack.length).toBe(before + 1);
    // Still selected at its new place.
    expect(useSequencer.getState().markSelection).toEqual({ track: track(seq, "Lyrics").id, starts: [7200] });
    expect(screen.getByTestId("timeline-announcer")).toHaveTextContent("Mark 'Hello' on Lyrics, 0:07.200 to 0:10.200, selected");

    // The end edge, snapping to the beat at 12 s.
    drag(canvas, [x(10_200) - 1, y], [x(11_950), y]);
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[7200, 12_000, "Hello"]]));

    fireEvent.doubleClick(canvas, { clientX: x(9000), clientY: y });
    const label = screen.getByRole("textbox", { name: "Mark label" });
    await user.clear(label);
    await user.type(label, "Hello there{Enter}");
    await waitFor(() => expect(track(seq, "Lyrics").marks[0].label).toBe("Hello there"));

    // Empty space: a new mark one beat long.
    fireEvent.doubleClick(canvas, { clientX: x(20_010), clientY: y });
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toContainEqual([20_000, 20_500, ""]));

    // Click one, Shift-click the other, Delete: both go in one step.
    fireEvent.pointerDown(canvas, { clientX: x(9000), clientY: y, button: 0, pointerId: 1 });
    fireEvent.pointerUp(canvas, { clientX: x(9000), clientY: y, pointerId: 1 });
    fireEvent.pointerDown(canvas, { clientX: x(20_250), clientY: y, button: 0, pointerId: 1, shiftKey: true });
    fireEvent.pointerUp(canvas, { clientX: x(20_250), clientY: y, pointerId: 1, shiftKey: true });
    expect(useSequencer.getState().markSelection?.starts).toEqual([7200, 20_000]);
    fireEvent.keyDown(window, { key: "Delete" });
    await waitFor(() => expect(track(seq, "Lyrics").marks).toEqual([]));
    await user.click(screen.getByRole("button", { name: "Undo (sequence)" }));
    await waitFor(() => expect(track(seq, "Lyrics").marks).toHaveLength(2));
  });

  it("moves the mark that was dragged, even when an earlier edit changes the marks first", async () => {
    const { seq } = await openScreen();
    await withLyrics([
      { startMs: 3000, endMs: 4000, label: "a" },
      { startMs: 6000, endMs: 7000, label: "b" },
      { startMs: 30_000, endMs: 31_000, label: "c" },
    ]);
    const canvas = timeline();
    const y = TRACK_Y[2];
    const release = holdNextEdit(seq);
    // Delete a, then drag b before the engine has answered.
    fireEvent.pointerDown(canvas, { clientX: x(3500), clientY: y, button: 0, pointerId: 1 });
    fireEvent.pointerUp(canvas, { clientX: x(3500), clientY: y, pointerId: 1 });
    fireEvent.keyDown(window, { key: "Delete" });
    drag(canvas, [x(6500), y], [x(9500), y], { altKey: true });
    release();
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[9000, 10_000, "b"], [30_000, 31_000, "c"]]));
    expect(useApp.getState().error).toBeNull();
  });

  it("drops a drag whose mark changed before it landed, and says so", async () => {
    const { seq } = await openScreen();
    const id = await withLyrics([
      { startMs: 3000, endMs: 4000, label: "a" },
      { startMs: 6000, endMs: 7000, label: "b" },
    ]);
    const release = holdNextEdit(seq);
    // A change to b on its way to the engine, then b dragged as it was before.
    const change = useSequencer.getState().edit([{ type: "setMark", track: id, index: 1, mark: { startMs: 6000, endMs: 8000, label: "b" } }]);
    drag(timeline(), [x(6500), TRACK_Y[2]], [x(12_500), TRACK_Y[2]], { altKey: true });
    release();
    await act(() => change);
    await waitFor(() => expect(useApp.getState().error).toBe("That mark changed before the move landed, so it stayed where it is. Drag it again."));
    expect(spans(track(seq, "Lyrics"))).toEqual([[3000, 4000, "a"], [6000, 8000, "b"]]);
  });

  it("moves the shared edge of two touching marks together", async () => {
    const { seq } = await openScreen();
    await withLyrics([
      { startMs: 6000, endMs: 9000, label: "a" },
      { startMs: 9000, endMs: 12_000, label: "b" },
    ]);
    drag(timeline(), [x(9000), TRACK_Y[2]], [x(10_500), TRACK_Y[2]], { altKey: true });
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[6000, 10_500, "a"], [10_500, 12_000, "b"]]));
    drag(timeline(), [x(10_500), TRACK_Y[2]], [x(7500), TRACK_Y[2]], { altKey: true });
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[6000, 7500, "a"], [7500, 12_000, "b"]]));
  });

  it("cancels a label typed in place with Escape", async () => {
    const { seq, user } = await openScreen();
    await withLyrics([{ startMs: 6000, endMs: 9000, label: "Hello" }]);
    fireEvent.doubleClick(timeline(), { clientX: x(7500), clientY: TRACK_Y[2] });
    const label = screen.getByRole("textbox", { name: "Mark label" });
    await user.clear(label);
    await user.type(label, "Oops{Escape}");
    expect(screen.queryByRole("textbox", { name: "Mark label" })).toBeNull();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(spans(track(seq, "Lyrics"))).toEqual([[6000, 9000, "Hello"]]);
  });

  it("says so when a mark moves while its label is being typed", async () => {
    const { seq, user } = await openScreen();
    const id = await withLyrics([{ startMs: 6000, endMs: 9000, label: "Hello" }]);
    fireEvent.doubleClick(timeline(), { clientX: x(7500), clientY: TRACK_Y[2] });
    const label = screen.getByRole("textbox", { name: "Mark label" });
    await act(() => useSequencer.getState().edit([{ type: "setMark", track: id, index: 0, mark: { startMs: 6500, endMs: 9000, label: "Hello" } }]));
    await user.type(label, "!{Enter}");
    await waitFor(() => expect(useApp.getState().error).toBe("That mark moved before its label was saved. Double-click it to type the label again."));
    expect(spans(track(seq, "Lyrics"))).toEqual([[6500, 9000, "Hello"]]);
  });

  it("doesn't tap while a track's menu is open", async () => {
    const { user } = await openScreen();
    await user.click(screen.getByRole("button", { name: "Beats menu" }));
    await user.keyboard("t");
    expect(useApp.getState().error).toBeNull();
  });

  it("keeps a mark from being dragged onto its neighbor", async () => {
    const { seq } = await openScreen();
    await withLyrics([
      { startMs: 6000, endMs: 9000, label: "a" },
      { startMs: 12_000, endMs: 15_000, label: "b" },
    ]);
    drag(timeline(), [x(7500), TRACK_Y[2]], [x(20_000), TRACK_Y[2]], { altKey: true });
    await waitFor(() => expect(spans(track(seq, "Lyrics"))).toEqual([[9000, 12_000, "a"], [12_000, 15_000, "b"]]));
  });

  it("leaves phoneme tracks as they are", async () => {
    const { seq } = await openScreen();
    const phonemes: TimingTrack = { id: crypto.randomUUID(), name: "Lyrics (phonemes)", kind: "phonemes", marks: [{ startMs: 6000, endMs: 9000, label: "AI" }] };
    await act(() => useSequencer.getState().edit([{ type: "addTimingTrack", track: phonemes }]));
    drag(timeline(), [x(7500), TRACK_Y[2]], [x(20_000), TRACK_Y[2]], { altKey: true });
    fireEvent.doubleClick(timeline(), { clientX: x(30_000), clientY: TRACK_Y[2] });
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(spans(track(seq, "Lyrics (phonemes)"))).toEqual([[6000, 9000, "AI"]]);
    await userEvent.setup().click(screen.getByRole("button", { name: "Lyrics (phonemes) menu" }));
    expect(screen.queryByRole("menuitem", { name: "Paste lyrics…" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Export…" })).toBeInTheDocument();
  });

  it("imports and exports timing files from the header menu", async () => {
    const { seq, user } = await openScreen();
    seq.timingFiles.set("/Shows/Vocals.xtiming", {
      tracks: [{ id: "v", name: "Vocals", kind: "lyrics", marks: [{ startMs: 0, endMs: 900, label: "Hi" }] }],
      notes: ["1 mark in Vocals.xtiming started after the end of the sequence and was left out."],
    });
    seq.nextTimingPath = "/Shows/Vocals.xtiming";
    await user.click(screen.getByRole("button", { name: "Beats menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Import timing file…" }));
    await waitFor(() => expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars", "Vocals"]));
    expect(await screen.findByText("Added 1 timing track from Vocals.xtiming: 'Vocals'.")).toBeInTheDocument();
    expect(screen.getByText("1 mark in Vocals.xtiming started after the end of the sequence and was left out.")).toBeInTheDocument();

    seq.nextSavePath = "/Shows/Beats.txt";
    await user.click(screen.getByRole("button", { name: "Beats menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Export…" }));
    expect(await screen.findByText("Exported 'Beats' (120 marks) to Beats.txt.")).toBeInTheDocument();
    expect(seq.exportedTimingFiles.get("/Shows/Beats.txt")![0].marks).toHaveLength(120);
  });
});
