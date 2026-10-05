import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import type { Effect, Sequence } from "../api/sequence";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

// The timeline is 1000 × 600 px at the window's corner: the demo's minute fits at 60 ms per pixel.
// Above the rows: ruler 24 + music 44 + two timing tracks of 18 = 104 px. Rows are 30 px a lane:
// Mega Tree (two layers) 104–164, Garage Arch 164–194, Window Matrix 194–224, Porch Star 224–254.
const MS_PER_PX = 60;
const x = (ms: number) => ms / MS_PER_PX;
const LANE = { treeTop: 119, archTop: 179, matrix: 209, star: 239 };

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

async function openScreen(withSequence = true) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  backend.nextAudioPath = DEMO_MUSIC;
  const seq = new MemorySequencer(backend);
  seq.nextSavePath = "/Shows/Medley.fseq";
  if (withSequence) {
    seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
    await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  }
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  return { backend, seq, user, show };
}

function rowEffects(doc: Sequence, name: string, show: ReturnType<typeof demoShow>): Effect[] {
  const prop = show.props.find((p) => p.name === name)!;
  const row = doc.rows.find((r) => "prop" in r.target && r.target.prop === prop.id)!;
  return row.layers.flatMap((l) => l.effects);
}

const timeline = () => screen.getByRole("application", { name: "Timeline" });

function drag(el: Element, from: [number, number], to: [number, number], init: Partial<PointerEventInit> = {}) {
  fireEvent.pointerDown(el, { clientX: from[0], clientY: from[1], button: 0, pointerId: 1, ...init });
  fireEvent.pointerMove(el, { clientX: (from[0] + to[0]) / 2, clientY: (from[1] + to[1]) / 2, pointerId: 1, ...init });
  fireEvent.pointerMove(el, { clientX: to[0], clientY: to[1], pointerId: 1, ...init });
  fireEvent.pointerUp(el, { clientX: to[0], clientY: to[1], pointerId: 1, ...init });
}

describe("sequence screen", () => {
  it("starts a sequence from a song, then finds its beats", async () => {
    const { user, seq } = await openScreen(false);
    expect(screen.getByRole("heading", { name: "Sequence" })).toBeInTheDocument();
    await user.click(screen.getByText("Start from a song.").closest("button")!);
    const dialog = screen.getByRole("dialog", { name: "New sequence" });
    await user.click(within(dialog).getByRole("button", { name: "Choose music…" }));
    expect(await within(dialog).findByText(/Christmas Medley 2017.mp3 · 1:00/)).toBeInTheDocument();
    expect(within(dialog).getByRole("textbox", { name: "Name" })).toHaveValue("Christmas Medley 2017");
    await user.click(within(dialog).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(seq.doc?.audio).toBe(DEMO_MUSIC));
    expect(seq.doc?.durationMs).toBe(60_000);
    // A new sequence needs rows: add every prop at once.
    await user.click(screen.getByRole("button", { name: "Add a row" }));
    await user.click(screen.getByRole("button", { name: "Add every prop (4)" }));
    await waitFor(() => expect(useSequencer.getState().doc?.rows).toHaveLength(4));
    const banner = screen.getByText(/Find the beats and bars in this song/).closest("[role=status]")!;
    await user.click(within(banner as HTMLElement).getByRole("button", { name: "Detect beats" }));
    await waitFor(() => expect(useSequencer.getState().doc?.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars"]));
    expect(screen.getByText("Beats")).toBeInTheDocument();
  });

  it("drags an effect from the palette onto a row, as one undo step", async () => {
    const { seq, show } = await openScreen();
    const before = seq.undoStack.length;
    const fire = screen.getByRole("button", { name: "Fire effect" });
    fireEvent.pointerDown(fire, { clientX: 20, clientY: 20, button: 0, pointerId: 1 });
    fireEvent.pointerMove(fire, { clientX: 400, clientY: 300, pointerId: 1 });
    fireEvent.pointerMove(fire, { clientX: x(57_000), clientY: LANE.star, pointerId: 1 });
    fireEvent.pointerUp(fire, { clientX: x(57_000), clientY: LANE.star, pointerId: 1 });
    await waitFor(() => expect(rowEffects(seq.doc!, "Porch Star", show).some((e) => e.params.kind === "fire")).toBe(true));
    const added = rowEffects(seq.doc!, "Porch Star", show).find((e) => e.params.kind === "fire")!;
    // One bar long (2 s at 120 BPM), with the catalog's default settings.
    expect([added.startMs, added.endMs]).toEqual([57_000, 59_000]);
    expect(added.params).toMatchObject({ kind: "fire", height: expect.any(Number) });
    expect(seq.undoStack.length).toBe(before + 1);
    expect(useSequencer.getState().selection).toEqual([added.id]);
    // Dropped off the rows, nothing is added.
    fireEvent.pointerDown(fire, { clientX: 20, clientY: 20, button: 0, pointerId: 1 });
    fireEvent.pointerMove(fire, { clientX: 300, clientY: 50, pointerId: 1 });
    fireEvent.pointerUp(fire, { clientX: 300, clientY: 50, pointerId: 1 });
    expect(seq.undoStack.length).toBe(before + 1);
  });

  it("adds an effect at the playhead from the keyboard", async () => {
    const { seq, user, show, backend } = await openScreen();
    useSequencer.getState().setPlayhead(57_000);
    act(() => useSequencer.getState().setActiveRow(seq.doc!.rows[3].id));
    screen.getByRole("button", { name: "Strobe effect" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(rowEffects(seq.doc!, "Porch Star", show).some((e) => e.params.kind === "strobe")).toBe(true));
    // Space on a palette item adds it too, without starting playback.
    act(() => useSequencer.getState().setPlayhead(58_500));
    screen.getByRole("button", { name: "Fire effect" }).focus();
    await user.keyboard(" ");
    await waitFor(() => expect(rowEffects(seq.doc!, "Porch Star", show).some((e) => e.params.kind === "fire")).toBe(true));
    expect(backend.calls.some((c) => c.startsWith("playAuthored"))).toBe(false);
    expect(useSequencer.getState().status).toBeNull();
  });

  it("moves effects to other rows, snapping, and resizes them by their edges", async () => {
    const { seq, show } = await openScreen();
    const wave = rowEffects(seq.doc!, "Garage Arch", show)[0];
    expect([wave.startMs, wave.endMs]).toEqual([0, 4000]);
    // Down onto Window Matrix, a little later: snaps to the half-second beat. The matrix's only
    // layer is full there, so it lands on a new layer instead of covering what's there.
    drag(timeline(), [x(1000), LANE.archTop], [x(1000) + 30, LANE.matrix]);
    await waitFor(() => expect(rowEffects(seq.doc!, "Window Matrix", show).some((e) => e.id === wave.id)).toBe(true));
    const matrix = seq.doc!.rows[2];
    expect(matrix.layers).toHaveLength(2);
    expect(matrix.layers[1].effects.map((e) => [e.id, e.startMs, e.endMs])).toEqual([[wave.id, 2000, 6000]]);
    // Holding Alt turns snapping off; the move stays on the frame grid (11 px is 660 ms: 650).
    const chase = rowEffects(seq.doc!, "Garage Arch", show)[0];
    expect(chase.startMs).toBe(4000);
    drag(timeline(), [x(chase.startMs + 1000), LANE.archTop], [x(chase.startMs + 1000) - 11, LANE.archTop], { altKey: true });
    await waitFor(() => expect(rowEffects(seq.doc!, "Garage Arch", show)[0].startMs).toBe(chase.startMs - 650));
    // Dragging the end edge changes the length, snapping to a beat. The matrix now has two lanes:
    // its new layer is at 224–254.
    const target = rowEffects(seq.doc!, "Window Matrix", show).find((e) => e.id === wave.id)!;
    drag(timeline(), [x(target.endMs) - 2, 239], [x(7_020), 239]);
    await waitFor(() => expect(rowEffects(seq.doc!, "Window Matrix", show).find((e) => e.id === wave.id)!.endMs).toBe(7000));
    // An edge stops at the next effect on its layer.
    const next = rowEffects(seq.doc!, "Garage Arch", show)[1];
    drag(timeline(), [x(chase.endMs - 650) - 2, LANE.archTop], [x(next.startMs + 2000), LANE.archTop], { altKey: true });
    await waitFor(() => expect(rowEffects(seq.doc!, "Garage Arch", show)[0].endMs).toBe(next.startMs));
  });

  it("selects with a click, Shift-click, and a marquee, and deletes with the keyboard", async () => {
    const { seq, user, show } = await openScreen();
    const arch = rowEffects(seq.doc!, "Garage Arch", show);
    fireEvent.pointerDown(timeline(), { clientX: x(1000), clientY: LANE.archTop, button: 0, pointerId: 1 });
    fireEvent.pointerUp(timeline(), { clientX: x(1000), clientY: LANE.archTop, pointerId: 1 });
    expect(useSequencer.getState().selection).toEqual([arch[0].id]);
    fireEvent.pointerDown(timeline(), { clientX: x(5000), clientY: LANE.archTop, button: 0, pointerId: 1, shiftKey: true });
    fireEvent.pointerUp(timeline(), { clientX: x(5000), clientY: LANE.archTop, pointerId: 1 });
    expect(useSequencer.getState().selection).toEqual([arch[0].id, arch[1].id]);
    // A marquee from the empty second layer above, over the arch from 1 s to 10 s, picks its first three effects.
    drag(timeline(), [x(1000), 140], [x(9_900), 190]);
    expect(useSequencer.getState().selection.sort()).toEqual(arch.slice(0, 3).map((e) => e.id).sort());
    await user.keyboard("{Delete}");
    await waitFor(() => expect(rowEffects(seq.doc!, "Garage Arch", show)).toHaveLength(arch.length - 3));
    await user.keyboard("{Meta>}z{/Meta}");
    await waitFor(() => expect(rowEffects(seq.doc!, "Garage Arch", show)).toHaveLength(arch.length));
  });

  it("changes settings from the catalog, one undo step per slider drag", async () => {
    const { seq, user } = await openScreen();
    fireEvent.pointerDown(timeline(), { clientX: x(1000), clientY: LANE.archTop, button: 0, pointerId: 1 });
    fireEvent.pointerUp(timeline(), { clientX: x(1000), clientY: LANE.archTop, pointerId: 1 });
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    expect(within(panel).getByRole("heading", { name: "Wave" })).toBeInTheDocument();
    const before = seq.undoStack.length;
    const slider = within(panel).getByRole("slider", { name: "Waves" });
    fireEvent.change(slider, { target: { value: "2" } });
    fireEvent.change(slider, { target: { value: "3" } });
    fireEvent.pointerUp(slider);
    const id = useSequencer.getState().selection[0];
    const find = () => seq.doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects)).find((e) => e.id === id)!;
    await waitFor(() => expect(find().params).toMatchObject({ cycles: 3 }));
    expect(seq.undoStack.length).toBe(before + 1);
    // The next drag is a step of its own.
    fireEvent.change(slider, { target: { value: "4" } });
    await waitFor(() => expect(find().params).toMatchObject({ cycles: 4 }));
    expect(seq.undoStack.length).toBe(before + 2);
    // Lists, colors, and mixing.
    await user.selectOptions(within(panel).getByRole("combobox", { name: "Direction" }), "reverse");
    await waitFor(() => expect(find().params).toMatchObject({ direction: "reverse" }));
    await user.click(within(panel).getByRole("button", { name: "Add a color" }));
    await waitFor(() => expect(find().palette.colors).toHaveLength(3));
    fireEvent.change(within(panel).getByLabelText("Color 1"), { target: { value: "#123456" } });
    await waitFor(() => expect(find().palette.colors[0]).toBe("#123456"));
    await user.selectOptions(within(panel).getByRole("combobox", { name: "With the layers below" }), "add");
    await waitFor(() => expect(find().blend).toBe("add"));
  });

  it("moves the playhead and selected effects with the keyboard, and copies and pastes", async () => {
    const { seq, user, show } = await openScreen();
    timeline().focus();
    await user.keyboard("{ArrowRight}");
    expect(useSequencer.getState().playheadMs).toBe(25);
    await user.keyboard("{Shift>}{ArrowRight}{/Shift}");
    expect(useSequencer.getState().playheadMs).toBe(500);
    await user.keyboard("{End}");
    expect(useSequencer.getState().playheadMs).toBe(60_000);
    await user.keyboard("{Home}");
    // Select the star's first effect and nudge it a frame later.
    const first = rowEffects(seq.doc!, "Porch Star", show)[0];
    act(() => useSequencer.getState().select([first.id]));
    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(rowEffects(seq.doc!, "Porch Star", show)[0].startMs).toBe(25));
    // The arch's effects sit end to end: nudging one would run into the next, so it stays put.
    const arch = rowEffects(seq.doc!, "Garage Arch", show)[0];
    act(() => useSequencer.getState().select([arch.id]));
    await user.keyboard("{ArrowRight}");
    expect(rowEffects(seq.doc!, "Garage Arch", show)[0].startMs).toBe(0);
    // Copy, then paste at 58 s.
    await user.keyboard("{Meta>}c{/Meta}");
    act(() => useSequencer.getState().setPlayhead(56_000));
    const count = rowEffects(seq.doc!, "Garage Arch", show).length;
    await user.keyboard("{Meta>}v{/Meta}");
    await waitFor(() => expect(rowEffects(seq.doc!, "Garage Arch", show)).toHaveLength(count + 1));
    expect(rowEffects(seq.doc!, "Garage Arch", show).some((e) => e.startMs === 56_000 && e.endMs === 60_000)).toBe(true);
  });

  it("plays with Space in the preview only, unless sending is turned on", async () => {
    const { backend, seq, user } = await openScreen();
    expect(seq.calls).toContain("setSequenceDocOutput:false");
    timeline().focus();
    await user.keyboard(" ");
    await waitFor(() => expect(backend.calls).toContain("playAuthored@0"));
    expect(screen.getByRole("button", { name: "Pause" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Send to controllers while playing" }));
    expect(seq.calls).toContain("setSequenceDocOutput:true");
    await user.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(useSequencer.getState().status).toBeNull());
  });

  it("exports an .fseq and adds it to the show's playlist", async () => {
    const { seq, user } = await openScreen();
    await user.click(screen.getByRole("button", { name: "Export and add to the show's playlist" }));
    await waitFor(() => expect(useApp.getState().snapshot?.show.sequences).toHaveLength(1));
    expect(seq.calls).toContain("exportSequenceDoc");
    expect(useApp.getState().snapshot?.show.sequences[0]).toMatchObject({ name: "Christmas Medley 2017", path: "/Shows/Medley.fseq", audio: DEMO_MUSIC });
  });

  it("walks rows with Up and Down, saying what's selected", async () => {
    const { seq, user } = await openScreen();
    act(() => useSequencer.getState().setPlayhead(21_000));
    timeline().focus();
    await user.keyboard("{ArrowDown}");
    expect(useSequencer.getState().activeRow).toBe(seq.doc!.rows[0].id);
    expect(screen.getByTestId("timeline-announcer")).toHaveTextContent("Bars on Mega Tree, 0:16.000 to 0:24.000, selected");
    await user.keyboard("{ArrowDown}");
    expect(screen.getByTestId("timeline-announcer")).toHaveTextContent("Chase on Garage Arch, 0:20.000 to 0:24.000, selected");
    await user.keyboard("{ArrowUp}{Escape}");
    expect(screen.getByTestId("timeline-announcer")).toHaveTextContent("No effect selected");
  });

  it("stops at the song's end and lets go of the music", async () => {
    const { backend } = await openScreen();
    await act(() => useSequencer.getState().play());
    await act(() => useSequencer.getState().seek(59_980));
    await waitFor(() => expect(useSequencer.getState().status).toBeNull());
    expect(useSequencer.getState().playheadMs).toBe(60_000);
    expect(within(screen.getByRole("toolbar", { name: "Sequence" })).getByRole("button", { name: "Play" })).toBeInTheDocument();
    // Clicking the ruler now only moves the playhead.
    fireEvent.pointerDown(timeline(), { clientX: x(6000), clientY: 10, button: 0, pointerId: 1 });
    fireEvent.pointerUp(timeline(), { clientX: x(6000), clientY: 10, pointerId: 1 });
    expect(useSequencer.getState().playheadMs).toBe(6000);
    expect(await backend.playbackStatus()).toBeNull();
  });

  it("brings rows and effects picked from the keyboard or the problem list into view", async () => {
    const { seq, user } = await openScreen();
    // Many layers on the first row push the others below the fold.
    await act(() => useSequencer.getState().edit(Array.from({ length: 20 }, () => ({ type: "addLayer" as const, row: seq.doc!.rows[0].id }))));
    const rows = screen.getByRole("slider", { name: "Scroll rows" });
    expect(rows).toHaveValue("0");
    act(() => useSequencer.getState().setActiveRow(seq.doc!.rows[0].id));
    timeline().focus();
    await user.keyboard("{ArrowDown}");
    // Garage Arch's lane is at 660–690 in a 496 px view.
    expect(Number((rows as HTMLInputElement).value)).toBeGreaterThanOrEqual(690 - 496);
    expect(screen.getByRole("listitem", { name: "Garage Arch" })).toHaveAttribute("aria-current", "true");
    // Zoomed in on the start, a problem near the end scrolls the time to it.
    for (let i = 0; i < 5; i++) await user.click(screen.getByRole("button", { name: "Zoom in" }));
    const time = screen.getByRole("slider", { name: "Scroll in time" });
    fireEvent.change(time, { target: { value: "0" } });
    expect(time).toHaveValue("0");
    const late = seq.doc!.rows[1 + 0].layers[0].effects.find((e) => e.startMs >= 50_000)!;
    act(() =>
      useSequencer.setState({ issues: [{ severity: "warning", message: "Something about the effect at 0:52.000.", row: seq.doc!.rows[1].id, effect: late.id }] }),
    );
    await user.click(screen.getByRole("button", { name: "1 problem" }));
    await user.click(screen.getByRole("button", { name: /Something about/ }));
    const start = Number((time as HTMLInputElement).value);
    expect(start).toBeLessThanOrEqual(late.startMs);
    expect(start).toBeGreaterThan(late.startMs - 10_000);
  });

  it("keeps the zoom when a sequence is first saved", async () => {
    const { seq, user } = await openScreen();
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    const time = screen.getByRole("slider", { name: "Scroll in time" });
    expect(time).not.toBeDisabled();
    seq.nextSavePath = "/Shows/Copy.pfseq.json";
    await act(() => useSequencer.getState().saveAs());
    expect(useSequencer.getState().path).toBe("/Shows/Copy.pfseq.json");
    expect(time).not.toBeDisabled();
    // Opening a different sequence fits the whole song again.
    seq.nextOpenPath = DEMO_SEQUENCE_PATH;
    await act(() => useSequencer.getState().open(DEMO_SEQUENCE_PATH));
    expect(screen.getByRole("slider", { name: "Scroll in time" })).toBeDisabled();
  });

  it("drags from the palette honestly: the ghost shows a new layer, Alt skips snapping, Escape cancels", async () => {
    const texts = recordTimelineText();
    const { seq, show } = await openScreen();
    const fire = screen.getByRole("button", { name: "Fire effect" });
    // Over the arch's effects: there's no room on its layer, so a new layer is shown.
    fireEvent.pointerDown(fire, { clientX: 20, clientY: 20, button: 0, pointerId: 1 });
    texts.length = 0;
    fireEvent.pointerMove(fire, { clientX: x(10_000), clientY: LANE.archTop, pointerId: 1 });
    expect(texts.some((t) => t.text === "+ New layer")).toBe(true);
    // Escape lets go: releasing over a row adds nothing.
    const before = seq.undoStack.length;
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.pointerUp(fire, { clientX: x(10_000), clientY: LANE.archTop, pointerId: 1 });
    expect(seq.undoStack.length).toBe(before);
    expect(useSequencer.getState().selection).toEqual([]);
    // With Alt, a drop near a beat stays where it was dropped (on the frame grid): 57.1 s.
    fireEvent.pointerDown(fire, { clientX: 20, clientY: 20, button: 0, pointerId: 1 });
    fireEvent.pointerMove(fire, { clientX: x(57_120), clientY: LANE.matrix, pointerId: 1, altKey: true });
    fireEvent.pointerUp(fire, { clientX: x(57_120), clientY: LANE.matrix, pointerId: 1, altKey: true });
    await waitFor(() => expect(rowEffects(seq.doc!, "Window Matrix", show).some((e) => e.params.kind === "fire" && e.startMs === 57_125)).toBe(true));
  });

  it("lists the sequence's problems, and a click selects the effect", async () => {
    const { seq, user } = await openScreen();
    const effect = seq.doc!.rows[1].layers[0].effects[2];
    act(() =>
      useSequencer.setState({
        issues: [{ severity: "warning", message: "The Wave effect at 0:08.000 on 'Garage Arch' overlaps the Chase effect.", row: seq.doc!.rows[1].id, effect: effect.id }],
      }),
    );
    await user.click(screen.getByRole("button", { name: "1 problem" }));
    await user.click(within(screen.getByRole("dialog", { name: "Problems in this sequence" })).getByRole("button", { name: /overlaps the Chase effect/ }));
    expect(useSequencer.getState().selection).toEqual([effect.id]);
  });
});

/** Every effect in the engine's copy, by id. */
function effectIn(seq: MemorySequencer, id: string): Effect {
  return seq.doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects)).find((e) => e.id === id)!;
}

/** A stand-in for the timeline's 2D canvas that remembers the text drawn on it. */
function recordTimelineText() {
  const texts: { text: string; x: number }[] = [];
  const ctx = new Proxy({} as Record<string | symbol, unknown>, {
    get(target, prop) {
      if (prop === "fillText") return (text: string, x: number) => texts.push({ text, x });
      return prop in target ? target[prop] : () => undefined;
    },
    set(target, prop, value) {
      target[prop] = value;
      return true;
    },
  });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement) {
    return (this.getAttribute("aria-label") === "Timeline" ? ctx : null) as never;
  });
  return texts;
}

describe("sequence screen with a slow engine", () => {
  it("types a time into a field without the field fighting back, as one undo step", async () => {
    const { seq, user, show } = await openScreen();
    seq.replyDelayMs = 30;
    const arch = rowEffects(seq.doc!, "Garage Arch", show);
    const last = arch[arch.length - 1];
    expect([last.startMs, last.endMs]).toEqual([52_000, 56_000]);
    act(() => useSequencer.getState().select([last.id]));
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    const ends = within(panel).getByLabelText("Ends (ms)");
    const before = seq.undoStack.length;
    const sent = seq.calls.length;
    await user.clear(ends);
    await user.type(ends, "58000");
    // Nothing is sent, or kept in range, while typing.
    expect(ends).toHaveValue(58_000);
    expect(seq.calls.length).toBe(sent);
    await user.keyboard("{Enter}");
    await waitFor(() => expect(effectIn(seq, last.id).endMs).toBe(58_000));
    expect(ends).toHaveValue(58_000);
    expect(seq.undoStack.length).toBe(before + 1);
    // Past the song's end it stops at the end; before the effect ahead of it, at that effect's end.
    await user.clear(ends);
    await user.type(ends, "70000{Enter}");
    await waitFor(() => expect(effectIn(seq, last.id).endMs).toBe(60_000));
    const starts = within(panel).getByLabelText("Starts (ms)");
    await user.clear(starts);
    await user.type(starts, "40000{Enter}");
    await waitFor(() => expect(effectIn(seq, last.id).startMs).toBe(arch[arch.length - 2].endMs));
    // Escape puts the value back without sending anything.
    await waitFor(() => expect(useSequencer.getState().doc).toEqual(seq.doc));
    const count = seq.undoStack.length;
    await user.clear(ends);
    await user.type(ends, "123{Escape}");
    expect(ends).toHaveValue(60_000);
    act(() => ends.blur());
    expect(seq.undoStack.length).toBe(count);
  });

  it("keeps every quick change, and adds up quick nudges, while replies are on their way", async () => {
    const { seq, user, show } = await openScreen();
    seq.replyDelayMs = 40;
    const wave = rowEffects(seq.doc!, "Garage Arch", show)[0];
    act(() => useSequencer.getState().select([wave.id]));
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    await user.selectOptions(within(panel).getByRole("combobox", { name: "Direction" }), "reverse");
    await user.selectOptions(within(panel).getByRole("combobox", { name: "With the layers below" }), "add");
    const slider = within(panel).getByRole("slider", { name: "Waves" });
    fireEvent.change(slider, { target: { value: "3" } });
    fireEvent.pointerUp(slider);
    await waitFor(() => expect(effectIn(seq, wave.id)).toMatchObject({ blend: "add", params: { direction: "reverse", cycles: 3 } }));
    await waitFor(() => expect(useSequencer.getState().doc).toEqual(seq.doc));

    const star = rowEffects(seq.doc!, "Porch Star", show)[0];
    act(() => useSequencer.getState().select([star.id]));
    timeline().focus();
    const steps = seq.undoStack.length;
    await user.keyboard("{ArrowRight}{ArrowRight}{ArrowRight}");
    await waitFor(() => expect(effectIn(seq, star.id).startMs).toBe(75));
    expect(seq.undoStack.length).toBe(steps + 3);
    // Holding the key is one undo step, however many times it repeats.
    fireEvent.keyDown(timeline(), { key: "ArrowLeft" });
    fireEvent.keyDown(timeline(), { key: "ArrowLeft", repeat: true });
    fireEvent.keyDown(timeline(), { key: "ArrowLeft", repeat: true });
    await waitFor(() => expect(effectIn(seq, star.id).startMs).toBe(0));
    expect(seq.undoStack.length).toBe(steps + 4);
  });

  it("keeps a dropped effect where it was dropped while the engine answers", async () => {
    const texts = recordTimelineText();
    const { seq, show } = await openScreen();
    seq.replyDelayMs = 50;
    const arch = rowEffects(seq.doc!, "Garage Arch", show);
    const last = arch[arch.length - 1];
    expect([last.startMs, last.endMs, last.params.kind]).toEqual([52_000, 56_000, "chase"]);
    // Drag it a second later, into the free space at the end (Alt: no snapping).
    fireEvent.pointerDown(timeline(), { clientX: x(54_000), clientY: LANE.archTop, button: 0, pointerId: 1, altKey: true });
    fireEvent.pointerMove(timeline(), { clientX: x(54_500), clientY: LANE.archTop, pointerId: 1, altKey: true });
    fireEvent.pointerMove(timeline(), { clientX: x(55_000), clientY: LANE.archTop, pointerId: 1, altKey: true });
    texts.length = 0;
    fireEvent.pointerUp(timeline(), { clientX: x(55_000), clientY: LANE.archTop, pointerId: 1, altKey: true });
    const localStart = () => useSequencer.getState().doc!.rows[1].layers[0].effects[13].startMs;
    const drawnAt = (ms: number) => texts.some((t) => t.text === "Chase" && Math.abs(t.x - (x(ms) + 5)) < 0.01);
    // Drawn at its new place straight away, before the reply has come back.
    expect(localStart()).toBe(52_000);
    expect(drawnAt(53_000)).toBe(true);
    await waitFor(() => expect(effectIn(seq, last.id).startMs).toBe(53_000));
    await waitFor(() => expect(localStart()).toBe(53_000));
    texts.length = 0;
    act(() => useSequencer.getState().setPlayhead(1));
    expect(drawnAt(53_000)).toBe(true);
    expect(drawnAt(52_000)).toBe(false);
  });

  it("finds beats without holding up edits made meanwhile", async () => {
    const { seq, user, show } = await openScreen();
    seq.analysisDelayMs = 400;
    const wave = rowEffects(seq.doc!, "Garage Arch", show)[0];
    act(() => useSequencer.getState().select([wave.id]));
    await user.click(screen.getByRole("button", { name: "Detect beats" }));
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    await user.selectOptions(within(panel).getByRole("combobox", { name: "Direction" }), "reverse");
    await waitFor(() => expect(useSequencer.getState().detecting && effectIn(seq, wave.id).params).toMatchObject({ direction: "reverse" }), { timeout: 300 });
    await waitFor(() => expect(useSequencer.getState().detecting).toBe(false), { timeout: 2000 });
    expect(useSequencer.getState().doc).toEqual(seq.doc);
  });

  it("makes each slider pull its own undo step, even after the screen is reopened", async () => {
    const { seq } = await openScreen();
    const wave = seq.doc!.rows[1].layers[0].effects[0];
    act(() => useSequencer.getState().select([wave.id]));
    const before = seq.undoStack.length;
    const pull = async (value: string) => {
      const slider = within(screen.getByRole("complementary", { name: "Effect settings" })).getByRole("slider", { name: "Waves" });
      fireEvent.change(slider, { target: { value } });
      fireEvent.pointerUp(slider);
      await waitFor(() => expect(effectIn(seq, wave.id).params).toMatchObject({ cycles: Number(value) }));
    };
    await pull("2");
    act(() => useApp.getState().setScreen("layout"));
    act(() => useApp.getState().setScreen("sequence"));
    await pull("3");
    expect(seq.undoStack.length).toBe(before + 2);
  });
});
