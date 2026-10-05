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
    const { seq, user, show } = await openScreen();
    useSequencer.getState().setPlayhead(57_000);
    act(() => useSequencer.getState().setActiveRow(seq.doc!.rows[3].id));
    screen.getByRole("button", { name: "Strobe effect" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(rowEffects(seq.doc!, "Porch Star", show).some((e) => e.params.kind === "strobe")).toBe(true));
  });

  it("moves effects to other rows, snapping, and resizes them by their edges", async () => {
    const { seq, show } = await openScreen();
    const wave = rowEffects(seq.doc!, "Garage Arch", show)[0];
    expect([wave.startMs, wave.endMs]).toEqual([0, 4000]);
    // Down onto Window Matrix, a little later: snaps to the half-second beat.
    drag(timeline(), [x(1000), LANE.archTop], [x(1000) + 30, LANE.matrix]);
    await waitFor(() => expect(rowEffects(seq.doc!, "Window Matrix", show).some((e) => e.id === wave.id)).toBe(true));
    const moved = rowEffects(seq.doc!, "Window Matrix", show).find((e) => e.id === wave.id)!;
    expect([moved.startMs, moved.endMs]).toEqual([2000, 6000]);
    expect(moved.startMs % 500).toBe(0);
    // Holding Alt turns snapping off.
    const chase = rowEffects(seq.doc!, "Garage Arch", show)[0];
    drag(timeline(), [x(chase.startMs + 1000), LANE.archTop], [x(chase.startMs + 1000) + 11, LANE.archTop], { altKey: true });
    await waitFor(() => expect(rowEffects(seq.doc!, "Garage Arch", show)[0].startMs).toBe(chase.startMs + 660));
    // Dragging the end edge changes the length, snapping to a beat.
    const target = rowEffects(seq.doc!, "Window Matrix", show).find((e) => e.id === wave.id)!;
    drag(timeline(), [x(target.endMs) - 2, LANE.matrix], [x(7_020), LANE.matrix]);
    await waitFor(() => expect(rowEffects(seq.doc!, "Window Matrix", show).find((e) => e.id === wave.id)!.endMs).toBe(7000));
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
    // Select the arch's first effect and nudge it a frame later.
    const first = rowEffects(seq.doc!, "Garage Arch", show)[0];
    act(() => useSequencer.getState().select([first.id]));
    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(rowEffects(seq.doc!, "Garage Arch", show)[0].startMs).toBe(25));
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
});
