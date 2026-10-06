import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Profiler } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { EffectSettings } from "../components/sequencer/EffectSettings";
import { demoPlayers, demoShow } from "../api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import type { Effect, Sequence } from "../api/sequence";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { runMenuAction } from "../state/menuActions";

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
    await user.click(within(dialog).getByRole("radio", { name: /Start empty/ }));
    await user.click(within(dialog).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(seq.doc?.audio).toBe(DEMO_MUSIC));
    expect(seq.doc?.durationMs).toBe(60_000);
    // It starts clean: nothing to save, and undo doesn't take the music away.
    expect(useSequencer.getState()).toMatchObject({ dirty: false, canUndo: false });
    expect(screen.getByRole("button", { name: "Undo (sequence)" })).toBeDisabled();
    // Started empty, it needs rows: add every prop at once, first in the picker.
    await user.click(screen.getByRole("button", { name: "Add a row" }));
    const picker = screen.getByRole("dialog", { name: "Add a row" });
    expect(within(picker).getAllByRole("button")[0]).toHaveAccessibleName("Add every prop (4)");
    await user.click(within(picker).getByRole("button", { name: "Add every prop (4)" }));
    await waitFor(() => expect(useSequencer.getState().doc?.rows).toHaveLength(4));
    const banner = screen.getByText(/Find the beats and bars in this song/).closest("[role=status]")!;
    await user.click(within(banner as HTMLElement).getByRole("button", { name: "Detect beats" }));
    await waitFor(() => expect(useSequencer.getState().doc?.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars"]));
    expect(screen.getByRole("group", { name: "Timing track Beats" })).toBeInTheDocument();
  });

  it("starts a new sequence with a row for every group and prop, in layout order, as xLights does", async () => {
    const { user, seq, backend } = await openScreen(false);
    const props = backend.show.props;
    await act(() => useApp.getState().apply([{ type: "addGroup", group: { id: "g", name: "Arches", members: [props[0].id] } }]));
    await user.click(screen.getByText("Start from a song.").closest("button")!);
    const dialog = screen.getByRole("dialog", { name: "New sequence" });
    expect(within(dialog).getByRole("radio", { name: /A row for every prop and group \(5\)/ })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(seq.doc?.rows).toHaveLength(5));
    expect(seq.doc!.rows.map((r) => r.target)).toEqual([{ group: "g" }, ...props.map((p) => ({ prop: p.id }))]);
    // Still clean, with nothing to undo.
    expect(useSequencer.getState()).toMatchObject({ dirty: false, canUndo: false });
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

  it("moves an effect dragged below the last row onto the last row, and above the first onto the first", async () => {
    const { seq, show } = await openScreen();
    const wave = rowEffects(seq.doc!, "Garage Arch", show)[0];
    // Below the rows (they end at 254 px): the last row, Porch Star.
    drag(timeline(), [x(1000), LANE.archTop], [x(1000), 450]);
    await waitFor(() => expect(rowEffects(seq.doc!, "Porch Star", show).some((e) => e.id === wave.id)).toBe(true));
    // It went on a new layer of the star's row (254–284 px, the star's own effects being in the way).
    // Up over the timing tracks: the first row, Mega Tree.
    drag(timeline(), [x(1000), 269], [x(1000), 80]);
    await waitFor(() => expect(rowEffects(seq.doc!, "Mega Tree", show).some((e) => e.id === wave.id)).toBe(true));
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
    await user.click(screen.getByRole("button", { name: "Show it on my lights while editing" }));
    expect(seq.calls).toContain("setSequenceDocOutput:true");
    await user.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(useSequencer.getState().status).toBeNull());
  });

  it("says what the live toggle does: it lights the real lights", async () => {
    await openScreen();
    const toggle = screen.getByRole("button", { name: "Show it on my lights while editing" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(toggle).toHaveTextContent("Show on my lights");
    expect(toggle.getAttribute("title")).toMatch(/sends each frame to your controllers live/);
    expect(screen.queryByRole("button", { name: /Send to controllers/ })).not.toBeInTheDocument();
  });

  it("sends the sequence to an FPP from the toolbar", async () => {
    const { backend } = await openScreen();
    backend.fppPlayers = demoPlayers();
    const fpp = useApp.getState().snapshot!.show.controllers[0];
    await useApp.getState().apply([{ type: "updateController", controller: { ...fpp, adapter: "fpp", address: "192.0.2.10" } }]);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Send to FPP…" }));
    const dialog = screen.getByRole("dialog", { name: "Send to FPP" });
    expect(within(dialog).getByText("Christmas Medley 2017.mp3")).toBeInTheDocument();
    await within(dialog).findByText(/free/);
    // The demo FPP already has this sequence: nothing is chosen for the user.
    const send = within(dialog).getByRole("button", { name: /^Send$/ });
    expect(send).toBeDisabled();
    await user.click(within(dialog).getByRole("radio", { name: /Keep both/ }));
    // Nothing is sent until Send.
    expect(backend.calls.some((c) => c.startsWith("fppSend:"))).toBe(false);
    await user.click(send);
    expect(await within(dialog).findByRole("button", { name: "Play it now on the FPP" })).toBeInTheDocument();
    expect(backend.calls).toContain("fppSend:192.0.2.10:Christmas Medley 2017 (2).fseq:none");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Send to FPP" })).not.toBeInTheDocument();
  });

  it("moves through the export menu with the arrow keys", async () => {
    const { user } = await openScreen();
    await user.click(screen.getByRole("button", { name: "More ways to export" }));
    const items = screen.getAllByRole("menuitem");
    await waitFor(() => expect(items[0]).toHaveFocus());
    await user.keyboard("{ArrowDown}");
    expect(items[1]).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(items[0]).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(items[1]).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "More ways to export" })).toHaveFocus();
  });

  it("exports an .fseq and adds it to the show's playlist, saying what happened", async () => {
    const { backend, seq, user } = await openScreen();
    const more = async (item: string) => {
      await user.click(screen.getByRole("button", { name: "More ways to export" }));
      await user.click(screen.getByRole("menuitem", { name: item }));
    };
    await more("Export .fseq…");
    expect(await screen.findByText("Exported 2,400 frames (1:00) to Medley.fseq.")).toBeInTheDocument();
    expect(useApp.getState().snapshot?.show.sequences).toHaveLength(0);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await more("Export and add to this show's playlist…");
    await waitFor(() => expect(useApp.getState().snapshot?.show.sequences).toHaveLength(1));
    expect(seq.calls).toContain("exportSequenceDoc");
    expect(useApp.getState().snapshot?.show.sequences[0]).toMatchObject({ name: "Christmas Medley 2017", path: "/Shows/Medley.fseq", audio: DEMO_MUSIC });
    // The show changed (one undo step on the show): the top bar and the notice say it needs saving.
    expect(await screen.findByText(/It's on the show's playlist; save the show to keep it there/)).toBeInTheDocument();
    expect(screen.getByText("Show not saved")).toBeInTheDocument();
    expect(useApp.getState().snapshot?.canUndo).toBe(true);
    // Adding the same file again doesn't list it twice.
    await more("Export and add to this show's playlist…");
    await waitFor(() => expect(seq.calls.filter((c) => c.startsWith("addSequenceDocToShow"))).toHaveLength(2));
    expect(useApp.getState().snapshot?.show.sequences).toHaveLength(1);
    backend.nextSavePath = "/Shows/House.pixelflow.json";
    await user.click(screen.getByRole("button", { name: "Save show" }));
    await waitFor(() => expect(useApp.getState().snapshot?.dirty).toBe(false));
    expect(screen.queryByText("Show not saved")).not.toBeInTheDocument();
  });

  it("says plainly that a cancelled export wrote nothing", async () => {
    const { seq } = await openScreen();
    const real = seq.exportSequenceDoc.bind(seq);
    vi.spyOn(seq, "exportSequenceDoc").mockImplementation((path, onProgress) =>
      real(path, (p) => {
        onProgress?.(p);
        if (p.percent === 50) void useSequencer.getState().cancelExport();
      }),
    );
    await act(() => useSequencer.getState().exportFseq(true));
    expect(screen.getByText("Export cancelled. No file was written.")).toBeInTheDocument();
    expect(useApp.getState().error).toBeNull();
    expect(useApp.getState().snapshot?.show.sequences).toHaveLength(0);
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
    seq.issues = [{ severity: "warning", message: "Something about the effect at 0:52.000.", row: seq.doc!.rows[1].id, effect: late.id }];
    await act(() => useSequencer.getState().refreshIssues());
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
    expect(screen.queryByRole("button", { name: /^\d+ problems?$/ })).not.toBeInTheDocument();
    // The engine finds a problem once the show changes: the list is checked again.
    seq.issues = [{ severity: "warning", message: "The Wave effect at 0:08.000 on 'Garage Arch' overlaps the Chase effect.", row: seq.doc!.rows[1].id, effect: effect.id }];
    await act(() => useApp.getState().apply([{ type: "renameShow", name: "Renamed" }]));
    const trigger = await screen.findByRole("button", { name: "1 problem" });
    await user.click(trigger);
    // The list takes the focus, and Escape gives it back.
    const item = within(screen.getByRole("dialog", { name: "Problems in this sequence" })).getByRole("button", { name: /overlaps the Chase effect/ });
    expect(item).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Problems in this sequence" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    await user.click(trigger);
    await user.click(within(screen.getByRole("dialog", { name: "Problems in this sequence" })).getByRole("button", { name: /overlaps the Chase effect/ }));
    expect(useSequencer.getState().selection).toEqual([effect.id]);
  });

  it("opens the add-row menu with the focus in it, and Escape closes it", async () => {
    const { user } = await openScreen();
    const add = screen.getByRole("button", { name: "Add row" });
    await user.click(add);
    const menu = screen.getByRole("dialog", { name: "Add a row" });
    expect(within(menu).getAllByRole("button").find((b) => !(b as HTMLButtonElement).disabled)).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Add a row" })).not.toBeInTheDocument();
    expect(add).toHaveFocus();
  });

  it("names the playhead and the snapping key in words", async () => {
    await openScreen();
    const toolbar = screen.getByRole("toolbar", { name: "Sequence" });
    expect(toolbar).toHaveTextContent("Playhead at 0:00.000");
    expect(within(toolbar).getByRole("button", { name: /hold (Alt|Option) while dragging/ })).toBeInTheDocument();
  });

  it("scrubbing asks for one preview frame at a time, then the latest", async () => {
    const { seq } = await openScreen();
    // Let the first still frame (at 0) come back.
    await act(async () => undefined);
    const pending: (() => void)[] = [];
    const asked: number[] = [];
    const real = seq.sequenceDocFrame.bind(seq);
    vi.spyOn(seq, "sequenceDocFrame").mockImplementation((ms) => {
      asked.push(ms);
      return new Promise((resolve) => pending.push(() => void real(ms).then(resolve)));
    });
    for (let ms = 1000; ms <= 10_000; ms += 1000) act(() => useSequencer.getState().setPlayhead(ms));
    expect(asked).toEqual([1000]);
    await act(async () => pending.shift()!());
    await waitFor(() => expect(asked).toEqual([1000, 10_000]));
    await act(async () => pending.shift()!());
    expect(asked).toEqual([1000, 10_000]);
  });

  it("doesn't redraw the settings panel as the playhead moves", async () => {
    const { seq } = await openScreen();
    const wave = seq.doc!.rows[1].layers[0].effects[0];
    act(() => useSequencer.getState().select([wave.id]));
    const renders = vi.fn();
    render(
      <Profiler id="settings" onRender={renders}>
        <EffectSettings doc={useSequencer.getState().doc!} />
      </Profiler>,
    );
    const before = renders.mock.calls.length;
    for (let ms = 0; ms < 2000; ms += 100) act(() => useSequencer.getState().setPlayhead(ms));
    act(() => useSequencer.setState({ exporting: 40 }));
    expect(renders.mock.calls.length).toBe(before);
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

describe("unsaved work", () => {
  it("offers back a sequence kept from last time, asking before it replaces unsaved changes", async () => {
    const show = demoShow();
    const backend = new MemoryBackend(show);
    const seq = new MemorySequencer(backend);
    const kept = demoSequence(show, 60_000);
    seq.recoveries = [{ id: "r1", name: "Christmas Medley 2017", path: DEMO_SEQUENCE_PATH, savedAtMs: Date.now() - 5 * 60_000, doc: kept }];
    await useApp.getState().connect(backend);
    useApp.setState({ started: true, screen: "sequence" });
    await useSequencer.getState().connect(seq);
    const user = userEvent.setup();
    render(<App />);
    const offer = screen.getByRole("region", { name: "Unsaved sequences from last time" });
    expect(offer).toHaveTextContent("PixelFlow kept unsaved changes to Christmas Medley 2017 from 5 min ago (Christmas Medley 2017.pfseq.json).");
    await user.click(within(offer).getByRole("button", { name: "Recover unsaved sequence Christmas Medley 2017" }));
    await waitFor(() => expect(useSequencer.getState().doc?.rows).toHaveLength(4));
    expect(useSequencer.getState()).toMatchObject({ dirty: true, path: DEMO_SEQUENCE_PATH });
    expect(screen.queryByRole("region", { name: "Unsaved sequences from last time" })).not.toBeInTheDocument();
    expect(screen.getByText("Sequence not saved")).toBeInTheDocument();

    // Another kept one, while this one has unsaved changes: asked first; Escape cancels.
    seq.recoveries = [{ id: "r2", name: "Older", path: null, savedAtMs: Date.now() - 3 * 86_400_000, doc: { ...kept, name: "Older" } }];
    await act(() => useSequencer.getState().connect(seq));
    await user.click(screen.getByRole("button", { name: "Recover unsaved sequence Older" }));
    expect(screen.getByRole("dialog", { name: "Unsaved changes" })).toBeInTheDocument();
    expect(within(screen.getByRole("dialog", { name: "Unsaved changes" })).getByRole("button", { name: "Save" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Unsaved changes" })).not.toBeInTheDocument();
    expect(useSequencer.getState().doc?.name).toBe("Christmas Medley 2017");
    // Thrown away instead.
    await user.click(screen.getByRole("button", { name: "Discard unsaved sequence Older" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Unsaved sequences from last time" })).not.toBeInTheDocument());
    expect(seq.calls).toContain("discardSequenceRecovery:r2");
  });

  it("asks before the window closes with an unsaved show or sequence", async () => {
    const { backend, seq, user } = await openScreen();
    // Nothing unsaved: the window just closes.
    expect(backend.requestClose()).toBe(true);
    backend.calls.length = 0;
    await act(() => useSequencer.getState().edit((doc) => [{ type: "removeRow", id: doc.rows[3].id }]));
    expect(backend.requestClose()).toBe(false);
    const dialog = await screen.findByRole("dialog", { name: "Save your changes before closing?" });
    expect(dialog).toHaveTextContent("There are unsaved changes to the sequence “Christmas Medley 2017”.");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Save your changes before closing?" })).not.toBeInTheDocument();
    expect(backend.calls).not.toContain("closeWindow");

    // Save saves both the sequence and the show, then closes.
    await act(() => useApp.getState().apply([{ type: "renameShow", name: "House" }]));
    act(() => void backend.requestClose());
    const both = await screen.findByRole("dialog", { name: "Save your changes before closing?" });
    expect(both).toHaveTextContent("the show “House” and the sequence “Christmas Medley 2017”");
    backend.nextSavePath = "/Shows/House.pixelflow.json";
    await user.click(within(both).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(backend.calls).toContain("closeWindow"));
    expect(useSequencer.getState().dirty).toBe(false);
    expect(useApp.getState().snapshot?.dirty).toBe(false);
    expect(seq.files.get(DEMO_SEQUENCE_PATH)!.rows).toHaveLength(3);
  });

  it("closes without saving when asked, and a cancelled save keeps the window open", async () => {
    const { backend, user } = await openScreen();
    await act(() => useSequencer.getState().edit((doc) => [{ type: "removeRow", id: doc.rows[3].id }]));
    await act(() => useApp.getState().apply([{ type: "renameShow", name: "House" }]));
    act(() => void backend.requestClose());
    // The show has no file yet; closing the save dialog keeps the question up.
    backend.nextSavePath = null;
    const dialog = await screen.findByRole("dialog", { name: "Save your changes before closing?" });
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(useSequencer.getState().dirty).toBe(false));
    expect(screen.getByRole("dialog", { name: "Save your changes before closing?" })).toBeInTheDocument();
    expect(backend.calls).not.toContain("closeWindow");
    await user.click(within(dialog).getByRole("button", { name: "Don't save" }));
    await waitFor(() => expect(backend.calls).toContain("closeWindow"));
  });

  it("takes a recent sequence that can't be opened off the list", async () => {
    const { user } = await openScreen(false);
    act(() => useSequencer.setState({ recent: [{ path: "/Shows/Gone.pfseq.json", show: null }] }));
    await user.click(screen.getByRole("button", { name: "Gone.pfseq.json" }));
    await waitFor(() => expect(useApp.getState().error).toMatch(/Gone.pfseq.json.*It's been taken off your recent sequences\./));
    expect(useSequencer.getState().recent).toEqual([]);
    expect(screen.queryByRole("region", { name: "Recent sequences" })).not.toBeInTheDocument();
  });
});

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

describe("saving", () => {
  const toolbar = () => screen.getByRole("toolbar", { name: "Sequence" });
  const removeFirstRow = () => act(() => useSequencer.getState().edit([{ type: "removeRow", id: useSequencer.getState().doc!.rows[0].id }]));

  it("⌘S saves the sequence and the show when both have changes, and says what it saved", async () => {
    const { backend, user } = await openScreen();
    backend.nextSavePath = "/Shows/Demo House.pixelflow.json";
    await removeFirstRow();
    await act(() => useApp.getState().apply([{ type: "setFrameRate", fps: 30 }]));
    expect(within(toolbar()).getByText("Sequence not saved")).toBeInTheDocument();
    await user.keyboard("{Meta>}s{/Meta}");
    await waitFor(() => expect(useApp.getState().snapshot!.dirty).toBe(false));
    expect(useSequencer.getState().dirty).toBe(false);
    expect(backend.calls).toContain("saveShowAs:/Shows/Demo House.pixelflow.json");
    expect(screen.getByTestId("toast")).toHaveTextContent("Saved Demo House and Christmas Medley 2017");
    expect(within(toolbar()).queryByText("Sequence not saved")).not.toBeInTheDocument();
  });

  it("⌘S saves the show first, and says plainly when the sequence couldn't be saved", async () => {
    const { backend, seq, user } = await openScreen();
    backend.nextSavePath = "/Shows/Demo House.pixelflow.json";
    const order: string[] = [];
    const saveShowAs = backend.saveShowAs.bind(backend);
    backend.saveShowAs = async (path) => (order.push("show"), saveShowAs(path));
    seq.saveSequenceDoc = async () => {
      order.push("sequence");
      throw new Error("The folder is read-only.");
    };
    await removeFirstRow();
    await act(() => useApp.getState().apply([{ type: "setFrameRate", fps: 30 }]));
    await user.keyboard("{Meta>}s{/Meta}");
    await waitFor(() => expect(useApp.getState().error).toBe("Christmas Medley 2017 wasn't saved: The folder is read-only."));
    expect(order).toEqual(["show", "sequence"]);
    expect(useApp.getState().snapshot!.dirty).toBe(false);
    expect(useSequencer.getState().dirty).toBe(true);
    const toasts = screen.getAllByTestId("toast").map((t) => t.textContent);
    expect(toasts).toEqual([expect.stringContaining("Saved Demo House")]);
    expect(toasts.join()).not.toContain("Christmas");
  });

  it("⌘S says plainly when the show couldn't be saved, and still saves the sequence", async () => {
    const { backend, user } = await openScreen();
    backend.nextSavePath = "/Shows/Demo House.pixelflow.json";
    backend.saveShowAs = async () => {
      throw new Error("The disk is full.");
    };
    await removeFirstRow();
    await act(() => useApp.getState().apply([{ type: "setFrameRate", fps: 30 }]));
    await user.keyboard("{Meta>}s{/Meta}");
    await waitFor(() => expect(useSequencer.getState().dirty).toBe(false));
    await waitFor(() => expect(useApp.getState().error).toBe("Demo House wasn't saved: The disk is full."));
    expect(screen.getAllByTestId("toast").map((t) => t.textContent)).toEqual([expect.stringContaining("Saved Christmas Medley 2017")]);
  });

  it("⌘S with the show's save cancelled saves the sequence without claiming the show", async () => {
    const { backend, user } = await openScreen();
    backend.nextSavePath = null;
    await removeFirstRow();
    await act(() => useApp.getState().apply([{ type: "setFrameRate", fps: 30 }]));
    await user.keyboard("{Meta>}s{/Meta}");
    await waitFor(() => expect(useSequencer.getState().dirty).toBe(false));
    expect(useApp.getState().error).toBeNull();
    expect(screen.getAllByTestId("toast").map((t) => t.textContent)).toEqual([expect.stringContaining("Saved Christmas Medley 2017")]);
  });

  it("⌘S saves only the sequence when the show has no changes", async () => {
    const { backend, user } = await openScreen();
    await removeFirstRow();
    await user.keyboard("{Meta>}s{/Meta}");
    await waitFor(() => expect(useSequencer.getState().dirty).toBe(false));
    expect(backend.calls.some((c) => c.startsWith("saveShow"))).toBe(false);
    expect(screen.getByTestId("toast")).toHaveTextContent("Saved Christmas Medley 2017");
  });

  it.each([
    ["File → Save in the menu bar", () => runMenuAction({ action: "save" })],
    ["the top bar's Save", () => userEvent.click(screen.getByRole("button", { name: "Save (the sequence, and the show if it changed)" }))],
    ["the Sequence toolbar's Save", () => userEvent.click(within(screen.getByRole("toolbar", { name: "Sequence" })).getByRole("button", { name: "Save" }))],
  ])("%s saves the show and the sequence, like ⌘S, with one toast", async (_how, save) => {
    const { backend } = await openScreen();
    backend.nextSavePath = "/Shows/Demo House.pixelflow.json";
    await removeFirstRow();
    await act(() => useApp.getState().apply([{ type: "setFrameRate", fps: 30 }]));
    expect(screen.getByText("Show not saved")).toBeInTheDocument();
    await act(async () => void (await save()));
    await waitFor(() => expect(useSequencer.getState().dirty).toBe(false));
    expect(useApp.getState().snapshot!.dirty).toBe(false);
    expect(screen.getAllByTestId("toast").map((t) => t.textContent)).toEqual([expect.stringContaining("Saved Demo House and Christmas Medley 2017")]);
    expect(screen.queryByText("Show not saved")).not.toBeInTheDocument();
  });

  it("File → Save As in the menu bar still saves the sequence under a new name only", async () => {
    const { seq } = await openScreen();
    seq.nextSavePath = "/Shows/Medley copy.pfseq.json";
    await act(() => useApp.getState().apply([{ type: "setFrameRate", fps: 30 }]));
    await act(() => runMenuAction({ action: "saveAs" }));
    await waitFor(() => expect(useSequencer.getState().path).toBe("/Shows/Medley copy.pfseq.json"));
    expect(useApp.getState().snapshot!.dirty).toBe(true);
  });
});
