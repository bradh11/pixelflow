import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { renderSequenceFrame } from "../../api/memoryRender";
import { MemorySequencer } from "../../api/memorySequencer";
import { newEffect, newRow } from "../../api/sequence";
import type { Show } from "../../api/types";
import { channelsPerPixel, nodeCount } from "../../lib/shows";
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

async function openScreen({ singing = false, prepare = (_show: Show) => {} } = {}) {
  const show = demoShow();
  prepare(show);
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000, { singing }));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  return { show, seq, user };
}

describe("submodels in sequences", () => {
  it("the add-row menu lists each prop's submodels under it, and their rows are named Prop / Submodel", async () => {
    const { show, seq, user } = await openScreen();
    await user.click(screen.getByRole("button", { name: "Add row" }));
    const menu = screen.getByRole("dialog", { name: "Add a row" });
    const names = within(menu)
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label") ?? b.textContent?.trim());
    const arch = names.findIndex((n) => n?.startsWith("Garage Arch"));
    expect(names.slice(arch, arch + 4)).toEqual([
      "Garage Arch (has a row)",
      "Garage Arch / Left half",
      "Garage Arch / Right half",
      "Garage Arch / Ends",
    ]);
    expect(names.some((n) => n?.includes("Singer")), "faces aren't rows").toBe(false);
    await user.click(within(menu).getByRole("button", { name: "Garage Arch / Left half" }));
    const prop = show.props.find((p) => p.name === "Garage Arch")!;
    const left = prop.regions.find((r) => r.name === "Left half")!;
    await waitFor(() => expect(seq.doc!.rows.at(-1)!.target).toEqual({ region: { prop: prop.id, region: left.id } }));
    expect(screen.getByRole("listitem", { name: "Garage Arch / Left half" })).toBeInTheDocument();
    // It now says so in the menu.
    await user.click(screen.getByRole("button", { name: "Add row" }));
    expect(within(screen.getByRole("dialog", { name: "Add a row" })).getByRole("button", { name: "Garage Arch / Left half" })).toHaveTextContent("(has a row)");
  });

  it("a Faces effect picks one of the row's faces and a timing track", async () => {
    const { show, seq, user } = await openScreen({ singing: true });
    const matrix = show.props.find((p) => p.name === "Window Matrix")!;
    const row = seq.doc!.rows.find((r) => "prop" in r.target && r.target.prop === matrix.id)!;
    const faces = row.layers[0].effects[0];
    expect(faces.params.kind).toBe("faces");
    act(() => useSequencer.getState().select([faces.id]));
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    expect(within(panel).getByRole("heading", { name: "Faces" })).toBeInTheDocument();
    const face = within(panel).getByRole("combobox", { name: "Face" });
    expect(face).toHaveValue("Singer");
    expect(within(face).getAllByRole("option").map((o) => o.textContent)).toEqual(["The first face (Singer)", "Singer"]);
    const track = within(panel).getByRole("combobox", { name: "Timing track" });
    expect(within(track).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "None (mouth at rest)",
      "Lyrics",
      "Lyrics (words)",
      "Lyrics (phonemes)",
      "Beats",
      "Bars",
    ]);
    const words = seq.doc!.timingTracks.find((t) => t.name === "Lyrics (words)")!;
    await user.selectOptions(track, words.id);
    await waitFor(() => expect(seq.doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects)).find((e) => e.id === faces.id)!.params).toMatchObject({ timingTrack: words.id }));
    expect(within(panel).getByText(/Mouth shapes are spread evenly over each word/)).toBeInTheDocument();
    const beats = seq.doc!.timingTracks.find((t) => t.name === "Beats")!;
    await user.selectOptions(track, beats.id);
    expect(await within(panel).findByText("This track has no words, so the mouth stays at rest. Pick a lyrics track to sing.")).toBeInTheDocument();
    await user.selectOptions(within(panel).getByRole("combobox", { name: "Eyes" }), "closed");
    await waitFor(() => expect(seq.doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects)).find((e) => e.id === faces.id)!.params).toMatchObject({ eyes: "closed" }));
  });

  it("a Faces effect on a group offers the faces of its submodel members' props", async () => {
    const group = { id: crypto.randomUUID(), name: "Windows", members: [] as Show["groups"][number]["members"] };
    const { show } = await openScreen({
      prepare: (show) => {
        const matrix = show.props.find((p) => p.name === "Window Matrix")!;
        group.members = [{ prop: matrix.id, region: matrix.regions[0].id }];
        show.groups.push(group);
      },
    });
    expect(show.groups.at(-1)!.name).toBe("Windows");
    const row = newRow({ group: group.id });
    const effect = newEffect("faces", 59_000, 59_500);
    row.layers[0].effects.push(effect);
    await act(() => useSequencer.getState().edit([{ type: "addRow", row }]));
    act(() => useSequencer.getState().select([effect.id]));
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    const face = within(panel).getByRole("combobox", { name: "Face" });
    expect(within(face).getAllByRole("option").map((o) => o.textContent)).toContain("Singer");
  });

  it("warns when the row's prop has no face", async () => {
    const { show, seq } = await openScreen();
    const star = show.props.find((p) => p.name === "Porch Star")!;
    const row = seq.doc!.rows.find((r) => "prop" in r.target && r.target.prop === star.id)!;
    const effect = newEffect("faces", 59_000, 59_500);
    await act(() => useSequencer.getState().edit([{ type: "addEffect", row: row.id, layer: 0, effect }]));
    act(() => useSequencer.getState().select([effect.id]));
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    expect(within(panel).getByText("This row's prop has no face. Import one from xLights, or pick another row.")).toBeInTheDocument();
  });
});

describe("the in-browser stand-in renderer", () => {
  it("lights only a submodel row's pixels, and a singing face's mouth", () => {
    const show = demoShow();
    const doc = demoSequence(show, 60_000, { singing: true });
    const arch = show.props.find((p) => p.name === "Garage Arch")!;
    // Only the Ends row: the arch's first and last six pixels.
    const ends = { ...doc, rows: doc.rows.filter((r) => "region" in r.target) };
    const frame = renderSequenceFrame(ends, show, 9_000);
    expect(show.props[0]).toBe(arch);
    const lit = Array.from({ length: 50 }, (_, n) => frame[n * 3] > 0);
    expect(lit.map((on, n) => (on ? n : -1)).filter((n) => n >= 0)).toEqual([0, 1, 2, 3, 4, 5, 44, 45, 46, 47, 48, 49]);
    expect(arch.regions.map((r) => r.name)).toContain("Ends");

    // The face sings "Jingle": its first sound lights a mouth in the face's red.
    const matrix = show.props.find((p) => p.name === "Window Matrix")!;
    const before = show.props.slice(0, show.props.indexOf(matrix)).reduce((n, p) => n + nodeCount(p.shape) * channelsPerPixel(p), 0);
    const singing = { ...doc, rows: doc.rows.filter((r) => "prop" in r.target && r.target.prop === matrix.id) };
    const sung = renderSequenceFrame(singing, show, 1_000);
    const red = [];
    for (let n = 0; n < 512; n++) if (sung[before + n * 3] === 0xff && sung[before + n * 3 + 1] === 0x2d) red.push(n);
    expect(red.length).toBeGreaterThan(0);
  });
});
