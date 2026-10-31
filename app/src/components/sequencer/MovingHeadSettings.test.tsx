import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend, emptyShow } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import { type Effect, type EffectParams, type Sequence, type SequenceTarget, newEffect, newRow } from "../../api/sequence";
import type { Show } from "../../api/types";
import { newMovingHead, newServo } from "../../lib/dmx";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { EffectSettings } from "./EffectSettings";
import { PanTiltPad, headReach } from "./MovingHeadSettings";
import { usesColors } from "./effectControls";

// The pad is 100 × 100 px at the window's corner.
beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    width: 100,
    height: 100,
    right: 100,
    bottom: 100,
    toJSON: () => ({}),
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

/** The pad over pan -100 to 100 and tilt -50 to 50, keeping what it's sent, as the engine would. */
function Pad({ sent, following = false }: { sent: [number, number, string][]; following?: boolean }) {
  const [at, setAt] = useState<[number, number]>([0, 0]);
  return (
    <PanTiltPad
      pan={at[0]}
      tilt={at[1]}
      reach={{ pan: [-100, 100], tilt: [-50, 50] }}
      following={following}
      onPoint={async (pan, tilt, gesture) => {
        sent.push([pan, tilt, gesture]);
        setAt([pan, tilt]);
        return true;
      }}
    />
  );
}

describe("the pan and tilt pad", () => {
  it("points the head where it's dragged, pan and tilt together as one gesture", async () => {
    const sent: [number, number, string][] = [];
    render(<Pad sent={sent} />);
    const pad = screen.getByRole("slider", { name: "Pan and tilt" });
    fireEvent.pointerDown(pad, { clientX: 25, clientY: 25, button: 0, pointerId: 1 });
    fireEvent.pointerMove(pad, { clientX: 75, clientY: 90, pointerId: 1 });
    fireEvent.pointerUp(pad, { pointerId: 1 });
    await waitFor(() => expect(sent.at(-1)?.slice(0, 2)).toEqual([50, -40]));
    expect(sent[0].slice(0, 2)).toEqual([-50, 25]);
    expect(new Set(sent.map((s) => s[2])).size).toBe(1);
    await waitFor(() => expect(pad).toHaveAttribute("aria-valuetext", "Pan 50°, tilt -40°"));
  });

  it("nudges with the arrow keys (Shift: 10°), within its reach, and goes Home", async () => {
    const sent: [number, number, string][] = [];
    render(<Pad sent={sent} />);
    const pad = screen.getByRole("slider", { name: "Pan and tilt" });
    const press = async (key: string, shiftKey = false) => {
      fireEvent.keyDown(pad, { key, shiftKey });
      fireEvent.keyUp(pad, { key, shiftKey });
      await waitFor(() => expect(pad.getAttribute("aria-valuetext")).toBe(`Pan ${sent.at(-1)![0]}°, tilt ${sent.at(-1)![1]}°`));
    };
    await press("ArrowRight");
    expect(sent.at(-1)!.slice(0, 2)).toEqual([1, 0]);
    await press("ArrowUp", true);
    expect(sent.at(-1)!.slice(0, 2)).toEqual([1, 10]);
    await press("ArrowDown");
    expect(sent.at(-1)!.slice(0, 2)).toEqual([1, 9]);
    for (let i = 0; i < 12; i++) await press("ArrowLeft", true);
    expect(sent.at(-1)!.slice(0, 2)).toEqual([-100, 9]);
    await press("Home");
    expect(sent.at(-1)!.slice(0, 2)).toEqual([0, 0]);
    // Each press is a step of its own.
    expect(new Set(sent.map((s) => s[2])).size).toBe(sent.length);
  });

  it("leaves a head following a curve to it", () => {
    const sent: [number, number, string][] = [];
    render(<Pad sent={sent} following />);
    const pad = screen.getByRole("slider", { name: "Pan and tilt" });
    expect(pad).toHaveAttribute("aria-disabled", "true");
    fireEvent.pointerDown(pad, { clientX: 25, clientY: 25, button: 0, pointerId: 1 });
    fireEvent.pointerUp(pad, { pointerId: 1 });
    fireEvent.keyDown(pad, { key: "ArrowRight" });
    expect(sent).toEqual([]);
  });

  it("reaches as far as the row's head can point", () => {
    const show = emptyShow("s");
    const head = newMovingHead(show);
    expect(headReach(head.dmx)).toEqual({ pan: [-270, 270], tilt: [-135, 135] });
    head.dmx!.channels.find((c) => c.role === "tilt")!.motor = { rangeDeg: 270, homeDeg: 135, minDeg: -30, maxDeg: 60 };
    expect(headReach(head.dmx).tilt).toEqual([-30, 60]);
    expect(headReach(undefined)).toEqual({ pan: [-180, 180], tilt: [-90, 90] });
  });
});

/** The selected effect's settings, drawn again as the sequence changes. */
function Settings() {
  const doc = useSequencer((s) => s.doc);
  return doc ? <EffectSettings doc={doc} /> : null;
}

/** A show of three moving heads in a group and a servo, a sequence with `effect` on `target`'s row, and its settings open. */
async function open(params: EffectParams, target: (show: Show) => SequenceTarget = (show) => ({ prop: show.props[0].id }), colors = ["#ff0000"]) {
  const show = emptyShow("s");
  for (const name of ["Left", "Middle", "Right"]) {
    const head = newMovingHead(show);
    head.name = name;
    show.props.push(head);
  }
  show.props.push(newServo(show));
  show.groups.push({ id: "g", name: "Heads", members: show.props.slice(0, 3).map((p) => p.id) } as Show["groups"][number]);
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  const effect: Effect = { ...newEffect(params.kind, 0, 4000, colors), params };
  const row = { ...newRow(target(show)), layers: [{ effects: [effect] }] };
  const doc: Sequence = { schemaVersion: 17, name: "s", audio: null, durationMs: 4000, frameMs: 25, timingTracks: [], rows: [row] };
  seq.files.set("/Shows/s.pfseq.json", doc);
  await seq.openSequenceDoc("/Shows/s.pfseq.json");
  await useApp.getState().connect(backend);
  await useSequencer.getState().connect(seq);
  act(() => useSequencer.getState().select([effect.id]));
  render(<Settings />);
  const panel = screen.getByRole("complementary", { name: "Effect settings" });
  const find = () => seq.doc!.rows[0].layers[0].effects[0];
  return { seq, panel, find };
}

const sliders = (panel: HTMLElement) => within(panel).queryAllByRole("slider").map((s) => s.getAttribute("aria-label") ?? (s.id && document.querySelector(`label[for='${s.id}']`)?.textContent));
const groups = (panel: HTMLElement) => within(panel).getAllByRole("region").map((r) => r.getAttribute("aria-label"));

describe("a Moving Head effect's settings", () => {
  it("groups them, with the pad pointing the head as one undo step", async () => {
    const { seq, panel, find } = await open({ kind: "movingHead" });
    expect(groups(panel)).toEqual(["Position", "Movement", "Fan", "Light", "Colors", "Fades"]);
    // A still head on its own: no sizes, speed or fan to set, and its light takes the first color.
    expect(sliders(panel)).toEqual(["Pan and tilt", "Pan", "Tilt", "Dimmer", "Strobe"]);
    expect(within(panel).getByRole("button", { name: "Fan for a group of heads" })).toHaveAttribute("aria-expanded", "false");
    expect(within(panel).getByText("This effect uses the first color.")).toBeInTheDocument();
    // Every control has a name.
    for (const role of ["button", "slider", "combobox", "spinbutton", "checkbox"] as const) {
      expect(within(panel).queryAllByRole(role, { name: (name) => name.trim() === "" })).toEqual([]);
    }
    const before = seq.undoStack.length;
    const pad = within(panel).getByRole("slider", { name: "Pan and tilt" });
    fireEvent.pointerDown(pad, { clientX: 50, clientY: 50, button: 0, pointerId: 1 });
    fireEvent.pointerMove(pad, { clientX: 60, clientY: 30, pointerId: 1 });
    fireEvent.pointerMove(pad, { clientX: 75, clientY: 20, pointerId: 1 });
    fireEvent.pointerUp(pad, { pointerId: 1 });
    // Its reach is the head's: pan -270 to 270, tilt -135 to 135.
    await waitFor(() => expect(find().params).toMatchObject({ pan: 135, tilt: 81 }));
    expect(seq.undoStack.length).toBe(before + 1);
    await act(() => useSequencer.getState().undo());
    expect(find().params).toEqual({ kind: "movingHead" });
  });

  it("shows the movement's sizes and speed only when it moves", async () => {
    const { panel } = await open({ kind: "movingHead", pattern: "sweep" });
    expect(sliders(panel)).toEqual(["Pan and tilt", "Pan", "Tilt", "Width", "Height", "Speed", "Dimmer", "Strobe"]);
  });

  it("shows the seed only for Wander", async () => {
    const { panel } = await open({ kind: "movingHead", pattern: "wander" });
    expect(sliders(panel)).toContain("Seed");
  });

  it("opens the fan on a group, and leaves the light to the layers below when asked", async () => {
    const { panel } = await open({ kind: "movingHead", color: "drawn" }, () => ({ group: "g" }));
    expect(within(panel).getByRole("button", { name: "Fan" })).toHaveAttribute("aria-expanded", "true");
    expect(sliders(panel)).toEqual(expect.arrayContaining(["Fan pan", "Fan tilt", "Delay per head"]));
    expect(sliders(panel)).not.toContain("Dimmer");
    expect(within(panel).getByText("The layers below light the head: this effect only moves it.")).toBeInTheDocument();
    expect(groups(panel)).not.toContain("Colors");
    expect(within(panel).getByRole("button", { name: "More: fans and mirror" })).toBeInTheDocument();
  });

  it("follows a curve on pan without fighting it", async () => {
    const { panel, find } = await open({ kind: "movingHead", pan: 30 });
    fireEvent.click(within(panel).getByRole("button", { name: "Change Pan over the effect" }));
    await waitFor(() => expect(within(panel).getByRole("slider", { name: "Pan and tilt" })).toHaveAttribute("aria-disabled", "true"));
    expect(within(panel).getByRole("button", { name: "Home" })).toBeDisabled();
    const pad = within(panel).getByRole("slider", { name: "Pan and tilt" });
    fireEvent.keyDown(pad, { key: "ArrowRight" });
    fireEvent.keyUp(pad, { key: "ArrowRight" });
    await act(async () => undefined);
    expect(find().params).toEqual({ kind: "movingHead", pan: 30 });
  });
});

describe("a Servo effect's settings", () => {
  it("groups them, with steps only for Steps, which servo under More, and no colors", async () => {
    const { panel } = await open({ kind: "servo", pattern: "steps" }, (show) => ({ prop: show.props[3].id }));
    expect(groups(panel)).toEqual(["Position", "Movement", "Fan", "Fades"]);
    expect(sliders(panel)).toEqual(["Position", "Size", "Speed", "Steps"]);
    fireEvent.click(within(panel).getByRole("button", { name: "Fan for several servos" }));
    fireEvent.click(within(panel).getByRole("button", { name: "More: which servo" }));
    expect(sliders(panel)).toEqual(["Position", "Size", "Speed", "Steps", "Fan", "Delay per servo", "Which servo"]);
  });

  it("drags onto a servo's row from the effects, like any effect", async () => {
    // The timeline is 1000 × 600 px; its first row's lane is 104-134 px down (see the sequence screen's tests).
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, width: 1000, height: 600, right: 1000, bottom: 600, toJSON: () => ({}) });
    const show = demoShow();
    const arm = newServo(show);
    show.props.push(arm);
    const backend = new MemoryBackend(show);
    backend.nextAudioPath = DEMO_MUSIC;
    const seq = new MemorySequencer(backend);
    seq.files.set(DEMO_SEQUENCE_PATH, { ...demoSequence(show, 60_000), rows: [newRow({ prop: arm.id })] });
    await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
    await useApp.getState().connect(backend);
    useApp.setState({ started: true, screen: "sequence" });
    await useSequencer.getState().connect(seq);
    render(<App />);
    const servoEffect = screen.getByRole("button", { name: "Servo effect" });
    fireEvent.pointerDown(servoEffect, { clientX: 20, clientY: 20, button: 0, pointerId: 1 });
    fireEvent.pointerMove(servoEffect, { clientX: 400, clientY: 300, pointerId: 1 });
    fireEvent.pointerMove(servoEffect, { clientX: 500, clientY: 119, pointerId: 1 });
    fireEvent.pointerUp(servoEffect, { clientX: 500, clientY: 119, pointerId: 1 });
    await waitFor(() => expect(seq.doc!.rows[0].layers[0].effects.map((e) => e.params.kind)).toEqual(["servo"]));
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    expect(within(panel).getByRole("heading", { name: "Servo" })).toBeInTheDocument();
    expect(within(panel).getByRole("slider", { name: "Position" })).toBeInTheDocument();
  });

  it("uses no palette colors, nor does a head lit by the layers below", () => {
    const effect = (params: EffectParams) => ({ ...newEffect(params.kind, 0, 1000), params });
    expect(usesColors(effect({ kind: "servo" }))).toBe(false);
    expect(usesColors(effect({ kind: "movingHead", color: "drawn" }))).toBe(false);
    expect(usesColors(effect({ kind: "movingHead" }))).toBe(true);
  });
});
