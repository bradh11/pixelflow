import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import type { Effect } from "../../api/sequence";
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
  /** The effects on a prop's row, as the engine has them now. */
  const effects = (name: string): Effect[] => {
    const prop = show.props.find((p) => p.name === name)!;
    const row = seq.doc!.rows.find((r) => "prop" in r.target && r.target.prop === prop.id)!;
    return row.layers.flatMap((l) => l.effects);
  };
  const byId = (id: string) => seq.doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects)).find((e) => e.id === id)!;
  return { seq, user, effects, byId };
}

const panel = () => screen.getByRole("complementary", { name: "Effect settings" });
const edits = (seq: MemorySequencer) => seq.calls.filter((c) => c === "editSequence").length;

describe("editing several effects at once", () => {
  it("shows a kind's settings for effects of one kind, saying which values differ, and changes them all as one undo step", async () => {
    const { seq, user, effects, byId } = await openScreen();
    // The arch alternates Wave and Chase: take two of its Chase effects, with different speeds.
    const [a, b] = effects("Garage Arch").filter((e) => e.params.kind === "chase");
    await useSequencer.getState().edit([{ type: "updateEffect", effect: { ...a, params: { ...a.params, speed: 3 } as Effect["params"] } }]);
    act(() => useSequencer.getState().select([a.id, b.id]));

    const p = panel();
    expect(within(p).getByRole("heading", { name: "2 Chase effects" })).toBeInTheDocument();
    expect(within(p).queryByText("Showing settings these effects share.")).not.toBeInTheDocument();
    // Speed differs: the box is empty and says so. Bands are the same on both.
    const speed = within(p).getByRole("spinbutton", { name: "Speed value" });
    expect(speed).toHaveValue(null);
    expect(speed).toHaveAttribute("placeholder", "Mixed");
    expect(within(p).getByRole("slider", { name: /^Speed/ })).toHaveAttribute("aria-valuetext", "Mixed");
    expect(within(p).getByRole("spinbutton", { name: "Bands value" })).toHaveValue(3);

    // Changing Bands sets it on both, leaves their different speeds alone, and is one step to undo.
    const [steps, sent] = [seq.undoStack.length, edits(seq)];
    await user.clear(within(p).getByRole("spinbutton", { name: "Bands value" }));
    await user.type(within(p).getByRole("spinbutton", { name: "Bands value" }), "5{Enter}");
    await waitFor(() => expect([byId(a.id).params, byId(b.id).params]).toMatchObject([{ bands: 5, speed: 3 }, { bands: 5, speed: 1.5 }]));
    expect(seq.undoStack.length).toBe(steps + 1);
    expect(edits(seq), "one batched edit").toBe(sent + 1);

    // Typing a speed into the mixed box gives both that speed.
    await user.type(within(p).getByRole("spinbutton", { name: "Speed value" }), "2{Enter}");
    await waitFor(() => expect([byId(a.id).params, byId(b.id).params]).toMatchObject([{ speed: 2 }, { speed: 2 }]));
    expect(within(p).getByRole("spinbutton", { name: "Speed value" })).toHaveValue(2);

    // A list and a checkbox: the same on both, then changed on both.
    await user.selectOptions(within(p).getByRole("combobox", { name: "Direction" }), "reverse");
    await waitFor(() => expect([byId(a.id).params, byId(b.id).params]).toMatchObject([{ direction: "reverse" }, { direction: "reverse" }]));
    await user.click(within(p).getByRole("checkbox", { name: "Bounce" }));
    await waitFor(() => expect([byId(a.id).params, byId(b.id).params]).toMatchObject([{ bounce: true }, { bounce: true }]));

    // One undo takes back the last change on both.
    await useSequencer.getState().undo();
    expect([byId(a.id).params, byId(b.id).params].map((x) => ("bounce" in x ? x.bounce : undefined) ?? false)).toEqual([false, false]);
    expect([byId(a.id).params, byId(b.id).params]).toMatchObject([{ bands: 5, direction: "reverse" }, { bands: 5, direction: "reverse" }]);
  });

  it("shows a mixed list or checkbox as mixed until it's changed", async () => {
    const { seq, user, effects, byId } = await openScreen();
    const [a, b] = effects("Garage Arch").filter((e) => e.params.kind === "chase");
    await useSequencer.getState().edit([{ type: "updateEffect", effect: { ...a, params: { ...a.params, direction: "reverse", bounce: true } as Effect["params"] } }]);
    act(() => useSequencer.getState().select([a.id, b.id]));
    const direction = within(panel()).getByRole("combobox", { name: "Direction" });
    expect(within(direction).getByRole("option", { name: "Mixed" })).toBeDisabled();
    expect((within(direction).getByRole("option", { name: "Mixed" }) as HTMLOptionElement).selected).toBe(true);
    expect(within(panel()).getByRole("checkbox", { name: "Bounce" })).toBePartiallyChecked();
    const sent = edits(seq);
    await user.selectOptions(direction, "forward");
    await waitFor(() => expect([byId(a.id).params, byId(b.id).params]).toMatchObject([{ direction: "forward" }, { direction: "forward" }]));
    expect(edits(seq)).toBe(sent + 1);
    expect(within(direction).queryByRole("option", { name: "Mixed" })).not.toBeInTheDocument();
  });

  it("shows only what effects of different kinds share, and changes colors, mixing, and fades on all of them", async () => {
    const { seq, user, effects, byId } = await openScreen();
    const [wave, chase] = effects("Garage Arch");
    expect([wave.params.kind, chase.params.kind]).toEqual(["wave", "chase"]);
    act(() => useSequencer.getState().select([chase.id, wave.id]));

    const p = panel();
    expect(within(p).getByRole("heading", { name: "2 effects" })).toBeInTheDocument();
    expect(within(p).getByText("Showing settings these effects share.")).toBeInTheDocument();
    expect(within(p).queryByRole("region", { name: "Settings" })).not.toBeInTheDocument();
    // Their colors differ: the first one's are shown, and a change gives both the result.
    expect(within(p).getByText(/These effects have different colors/)).toBeInTheDocument();
    expect(within(p).getByLabelText("Color 1")).toHaveValue(wave.palette.colors[0]);
    const steps = seq.undoStack.length;
    await user.click(within(p).getByRole("button", { name: "Add a color" }));
    await waitFor(() => expect(byId(chase.id).palette.colors).toEqual([...wave.palette.colors, wave.palette.colors.at(-1)]));
    expect(byId(wave.id).palette.colors).toEqual(byId(chase.id).palette.colors);
    expect(seq.undoStack.length).toBe(steps + 1);
    expect(within(p).queryByText(/These effects have different colors/)).not.toBeInTheDocument();

    await user.selectOptions(within(p).getByRole("combobox", { name: "With the layers below" }), "add");
    await waitFor(() => expect([byId(wave.id).blend, byId(chase.id).blend]).toEqual(["add", "add"]));

    await user.clear(within(p).getByRole("spinbutton", { name: "Fade in (ms)" }));
    await user.type(within(p).getByRole("spinbutton", { name: "Fade in (ms)" }), "300{Enter}");
    await waitFor(() => expect([byId(wave.id).fadeInMs, byId(chase.id).fadeInMs]).toEqual([300, 300]));
    expect(seq.undoStack.length).toBe(steps + 3);
  });

  it("moves them all and sets their lengths, saying so when they can't move", async () => {
    const { seq, user, effects, byId } = await openScreen();
    // The star's effects are half a second long, a second apart.
    const [a, b] = effects("Porch Star");
    act(() => useSequencer.getState().select([a.id, b.id]));
    const p = panel();
    expect(within(p).getByRole("spinbutton", { name: "Length of each (ms)" })).toHaveValue(500);

    const steps = seq.undoStack.length;
    await user.type(within(p).getByRole("spinbutton", { name: "Move all by (ms)" }), "200{Enter}");
    await waitFor(() => expect([byId(a.id).startMs, byId(b.id).startMs]).toEqual([200, 1200]));
    expect(seq.undoStack.length).toBe(steps + 1);
    // The box is ready for the next move.
    expect(within(p).getByRole("spinbutton", { name: "Move all by (ms)" })).toHaveValue(null);

    await user.clear(within(p).getByRole("spinbutton", { name: "Length of each (ms)" }));
    await user.type(within(p).getByRole("spinbutton", { name: "Length of each (ms)" }), "800{Enter}");
    await waitFor(() => expect([byId(a.id).endMs, byId(b.id).endMs]).toEqual([1000, 2000]));
    expect(seq.undoStack.length).toBe(steps + 2);

    // Against the next effect: they can't move later.
    await user.type(within(p).getByRole("spinbutton", { name: "Move all by (ms)" }), "100{Enter}");
    await waitFor(() => expect(useApp.getState().error).toMatch(/can't move that way/));
    expect(seq.undoStack.length).toBe(steps + 2);

    // Lengths that differ show as mixed.
    const tree = effects("Mega Tree").slice(0, 2);
    act(() => useSequencer.getState().select(tree.map((e) => e.id)));
    expect(within(panel()).getByRole("spinbutton", { name: "Length of each (ms)" })).toHaveAttribute("placeholder", "Mixed");
  });

  it("deletes them all", async () => {
    const { user, effects } = await openScreen();
    const [a, b] = effects("Porch Star");
    act(() => useSequencer.getState().select([a.id, b.id]));
    const count = effects("Porch Star").length;
    await user.click(within(panel()).getByRole("button", { name: "Delete 2 effects" }));
    await waitFor(() => expect(effects("Porch Star")).toHaveLength(count - 2));
  });
});
