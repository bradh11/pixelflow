import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import { renderSequenceFrame } from "../api/memoryRender";
import { type Effect, type EffectParams, newEffect, newRow, type Sequence, type SequenceTarget } from "../api/sequence";
import type { Show } from "../api/types";
import { DMX_LAYOUTS, layoutChannels, newMovingHead, newServo } from "./dmx";
import { type FanPlace, chance, fanPlace, hash32, headOffset, servoOffset, strobeLit } from "./headEffects";
import { defaultMotor, motorDegrees } from "./motion";

// The same cases as the engine's (crates/pf-render/src/heads.rs and tests/heads.rs).

const close = (a: [number, number], b: [number, number], within = 1e-9) => Math.abs(a[0] - b[0]) < within && Math.abs(a[1] - b[1]) < within;
const at = (index: number, place: number, right: boolean): FanPlace => ({ index, place, right });

describe("where the Moving Head and Servo effects point (the engine's heads.rs)", () => {
  it("fans heads about the middle of their fan", () => {
    expect([0, 1, 2, 3].map((i) => fanPlace(i, 4, 1).place)).toEqual([-1.5, -0.5, 0.5, 1.5]);
    expect(fanPlace(1, 3, 1)).toEqual(at(1, 0, false));
    expect(fanPlace(2, 3, 1)).toEqual(at(2, 1, true));
    // Eight heads in two fans: heads 1 and 5 move alike.
    expect(fanPlace(0, 8, 2)).toEqual(fanPlace(4, 8, 2));
    expect(fanPlace(4, 8, 2)).toEqual(at(0, -1.5, false));
    // Five in two fans: three, then two.
    expect(fanPlace(3, 5, 2)).toEqual(at(0, -0.5, false));
    expect(fanPlace(4, 5, 2)).toEqual(at(1, 0.5, true));
    // More fans than heads: each head alone.
    expect(fanPlace(2, 3, 9)).toEqual(at(0, 0, false));
    expect(fanPlace(0, 1, 1)).toEqual(at(0, 0, false));
  });

  it("takes the engine's hash and chances", () => {
    expect(hash32(12345)).toBe(2435775735);
    expect(chance(7, 2, 3)).toBeCloseTo(0.7754901414737105, 12);
    expect(chance(9999, 15, 7)).toBeCloseTo(0.720943747786805, 12);
  });

  it("moves about the centre in each pattern", () => {
    expect(close(headOffset("still", 0.3, 90, 40, 1, 0), [0, 0])).toBe(true);
    // A quarter of the way round: a sweep at its right end, a circle at its right.
    expect(close(headOffset("sweep", 0.25, 90, 40, 1, 0), [45, 20])).toBe(true);
    expect(close(headOffset("circle", 0, 90, 40, 1, 0), [0, 20])).toBe(true);
    expect(close(headOffset("circle", 0.25, 90, 40, 1, 0), [45, 0])).toBe(true);
    expect(close(headOffset("circle", 1.25, 90, 40, 1, 0), [45, 0])).toBe(true);
    // A figure eight is at its top an eighth of the way round, and its middle at half.
    expect(headOffset("eight", 0.125, 90, 40, 1, 0)[1]).toBeCloseTo(20, 9);
    expect(close(headOffset("eight", 0.5, 90, 40, 1, 0), [0, 0])).toBe(true);
    // A wave retraces its line on the way back.
    expect(close(headOffset("wave", 0.1, 90, 40, 1, 0), headOffset("wave", 0.4, 90, 40, 1, 0))).toBe(true);
    // Wander stays within its size, differs from head to head, and comes back the same.
    for (let k = 0; k < 200; k++) {
      const [p, t] = headOffset("wander", k * 0.37, 90, 40, 7, 2);
      expect(Math.abs(p) <= 45 && Math.abs(t) <= 20).toBe(true);
    }
    const a = headOffset("wander", 3.3, 90, 40, 7, 0);
    expect(close(a, headOffset("wander", 3.3, 90, 40, 7, 1))).toBe(false);
    expect(close(a, headOffset("wander", 3.3, 90, 40, 8, 0))).toBe(false);
    expect(close(a, headOffset("wander", 3.3, 90, 40, 7, 0))).toBe(true);
  });

  it("sweeps, waves and steps a servo", () => {
    expect(servoOffset("still", 0.3, 90, 4)).toBe(0);
    expect([0, 0.125, 0.25, 0.5, 0.75, 1].map((c) => servoOffset("sweep", c, 90, 4))).toEqual([0, 22.5, 45, 0, -45, 0]);
    expect(servoOffset("wave", 0.25, 90, 4)).toBeCloseTo(45, 9);
    expect([0, 0.3, 0.6, 0.9].map((c) => servoOffset("steps", c, 90, 4))).toEqual([-45, -15, 15, 45]);
  });

  it("lights a strobe for half of each flash", () => {
    expect(strobeLit(0, 0.7)).toBe(true);
    expect(strobeLit(2, 0.1)).toBe(true);
    expect(strobeLit(2, 0.3)).toBe(false);
    expect(strobeLit(2, 0.5)).toBe(true);
  });
});

/**
 * Three 16-bit RGBW heads (11 channels each: pan, pan fine, tilt, tilt fine, speed, dimmer,
 * strobe, red, green, blue, white) and a fixture of two 16-bit servos, the heads in a group.
 * The frame holds the cells first (a cell each head, two for the servos), then the channels.
 */
function rig(): { show: Show; heads: SequenceTarget; spot: SequenceTarget; arms: SequenceTarget } {
  const show = emptyShow("heads");
  ["Left", "Middle", "Right"].forEach((name, i) => {
    const head = newMovingHead(show);
    head.name = name;
    head.transform.position.x = i * 2;
    show.props.push(head);
  });
  const arms = newServo(show);
  arms.shape = { source: "generator", type: "line", nodes: 2, length: 1 };
  arms.dmx!.channels = layoutChannels(DMX_LAYOUTS.find((l) => l.id === "servo-16")!, 2);
  show.props.push(arms);
  show.groups.push({ id: "g", name: "Heads", members: show.props.slice(0, 3).map((p) => p.id) } as Show["groups"][number]);
  return { show, heads: { group: "g" }, spot: { prop: show.props[0].id }, arms: { prop: arms.id } };
}

function head(params: Omit<Extract<EffectParams, { kind: "movingHead" }>, "kind"> = {}, colors = ["#ff0000"]): Effect {
  const effect = newEffect("movingHead", 0, 4000, colors);
  effect.params = { kind: "movingHead", ...params };
  return effect;
}

function servo(params: Omit<Extract<EffectParams, { kind: "servo" }>, "kind"> = {}): Effect {
  const effect = newEffect("servo", 0, 4000);
  effect.params = { kind: "servo", ...params };
  return effect;
}

function sequence(...rows: [SequenceTarget, Effect[][]][]): Sequence {
  return {
    ...({} as Sequence),
    durationMs: 4000,
    timingTracks: [],
    rows: rows.map(([target, layers]) => ({ ...newRow(target), layers: layers.map((effects) => ({ effects })) })),
  };
}

const headChannels = (frame: Uint8Array, k: number) => Array.from(frame.slice(15 + k * 11, 26 + k * 11));
const servoChannels = (frame: Uint8Array) => Array.from(frame.slice(48, 52));

/** Where a head points, in degrees from Home: its pan and tilt from their channels. */
function pointing(frame: Uint8Array, k: number): [number, number] {
  const c = headChannels(frame, k);
  return [motorDegrees(defaultMotor("pan"), c[0], c[1]), motorDegrees(defaultMotor("tilt"), c[2], c[3])];
}

const near = (a: [number, number], b: [number, number]) => close(a, b, 0.05);
const servoDegrees = (frame: Uint8Array, k: number) => {
  const c = servoChannels(frame);
  return motorDegrees(defaultMotor("servo"), c[k * 2], c[k * 2 + 1]);
};

describe("the browser stand-in's Moving Head and Servo effects (the engine's tests/heads.rs)", () => {
  it("points a head and lights it in the first palette color", () => {
    const { show, spot } = rig();
    const frame = renderSequenceFrame(sequence([spot, [[head({ pan: 90, tilt: 45, dimmer: 50 })]]]), show, 1000);
    const c = headChannels(frame, 0);
    expect(c.slice(0, 4)).toEqual([170, 170, 170, 170]);
    expect(c[4]).toBe(0);
    // Red at half: the dimmer carries the brightness, red is full.
    expect(c.slice(5)).toEqual([128, 0, 255, 0, 0, 0]);
    expect(Array.from(frame.slice(0, 3))).toEqual([128, 0, 0]);
    // The other heads aren't on its row: at Home and dark.
    expect(headChannels(frame, 1).slice(0, 6)).toEqual([128, 0, 128, 0, 0, 0]);
  });

  it("fans a group out about its middle head, mirrors it, and ripples it with a delay", () => {
    const { show, heads } = rig();
    let frame = renderSequenceFrame(sequence([heads, [[head({ tilt: 30, fanPan: 40 })]]]), show, 500);
    expect([0, 1, 2].map((k) => near(pointing(frame, k), [[-40, 30], [0, 30], [40, 30]][k] as [number, number]))).toEqual([true, true, true]);
    frame = renderSequenceFrame(sequence([heads, [[head({ pan: 30, mirror: true })]]]), show, 500);
    expect(near(pointing(frame, 0), [30, 0])).toBe(true);
    expect(near(pointing(frame, 1), [30, 0])).toBe(true);
    expect(near(pointing(frame, 2), [-30, 0])).toBe(true);
    // A sweep with a delay: each head is where the one before it was a moment ago.
    const seq = sequence([heads, [[head({ pattern: "sweep", width: 100, height: 0, speed: 0.5, delay: 0.25 })]]]);
    const later = renderSequenceFrame(seq, show, 1500);
    expect(near(pointing(later, 1), pointing(renderSequenceFrame(seq, show, 1250), 0))).toBe(true);
    expect(near(pointing(later, 2), pointing(renderSequenceFrame(seq, show, 1000), 0))).toBe(true);
    // A second and a half in, the first head is three quarters round: at its left end.
    expect(near(pointing(later, 0), [-50, 0])).toBe(true);
    // A head running behind waits at the start.
    expect(near(pointing(renderSequenceFrame(seq, show, 100), 2), [0, 0])).toBe(true);
  });

  it("speeds the movement up smoothly with a speed curve", () => {
    const { show, spot } = rig();
    const circling = head({ pattern: "circle", width: 90, height: 90 });
    // From 0 to 1 a second over four seconds: half way round by 2 s, at the bottom of the circle.
    circling.curves = { speed: { shape: "ramp", from: 0, to: 1 } };
    const seq = sequence([spot, [[circling]]]);
    expect(near(pointing(renderSequenceFrame(seq, show, 2000), 0), [0, -45])).toBe(true);
    let last = pointing(renderSequenceFrame(seq, show, 0), 0);
    for (let t = 40; t < 4000; t += 40) {
      const now = pointing(renderSequenceFrame(seq, show, t), 0);
      expect(Math.abs(now[0] - last[0]) < 12 && Math.abs(now[1] - last[1]) < 12).toBe(true);
      last = now;
    }
  });

  it("gives each head a color, strobes, fades only the light, and leaves the color to the layers below", () => {
    const { show, heads } = rig();
    let frame = renderSequenceFrame(sequence([heads, [[head({ color: "perHead" }, ["#ff0000", "#00ff00"])]]]), show, 1000);
    expect(headChannels(frame, 0).slice(7)).toEqual([255, 0, 0, 0]);
    expect(headChannels(frame, 1).slice(7)).toEqual([0, 255, 0, 0]);
    expect(headChannels(frame, 2).slice(7)).toEqual([255, 0, 0, 0]);
    const strobing = sequence([heads, [[head({ strobe: 2 })]]]);
    expect(headChannels(renderSequenceFrame(strobing, show, 100), 0)[5]).toBe(255);
    expect(headChannels(renderSequenceFrame(strobing, show, 300), 0)[5]).toBe(0);
    const faded = head({ pan: 90 });
    faded.fadeInMs = 2000;
    frame = renderSequenceFrame(sequence([heads, [[faded]]]), show, 1000);
    expect(headChannels(frame, 0)[5]).toBe(128);
    expect(near(pointing(frame, 0), [90, 0])).toBe(true);
    const on = newEffect("on", 0, 4000, ["#0000ff"]);
    frame = renderSequenceFrame(sequence([heads, [[on], [head({ color: "drawn", tilt: 45 })]]]), show, 1000);
    expect(headChannels(frame, 0).slice(5)).toEqual([255, 0, 0, 0, 255, 0]);
    expect(near(pointing(frame, 0), [0, 45])).toBe(true);
  });

  it("lets the later row or higher layer win a channel, with the DMX effect", () => {
    const { show, spot } = rig();
    const dmx = (channels: Record<string, number>) => {
      const effect = newEffect("dmx", 0, 4000);
      effect.params = { kind: "dmx", channels };
      return effect;
    };
    let c = headChannels(renderSequenceFrame(sequence([spot, [[head({ tilt: 45 })], [dmx({ "1": 0, "2": 0 })]]]), show, 500), 0);
    expect(c.slice(0, 4)).toEqual([0, 0, 170, 170]);
    expect(c[7]).toBe(255);
    c = headChannels(renderSequenceFrame(sequence([spot, [[dmx({ "1": 0, "2": 0 })], [head({ tilt: 45 })]]]), show, 500), 0);
    expect(c.slice(0, 4)).toEqual([128, 0, 170, 170]);
    const frame = renderSequenceFrame(sequence([spot, [[head({ pan: 90 })]]], [spot, [[head({ pan: -90 })]]]), show, 500);
    expect(near(pointing(frame, 0), [-90, 0])).toBe(true);
  });

  it("turns servos, fans them, picks one, and steps them", () => {
    const { show, arms } = rig();
    let frame = renderSequenceFrame(sequence([arms, [[servo({ position: 30, fan: 20 })]]]), show, 500);
    expect(servoDegrees(frame, 0)).toBeCloseTo(20, 1);
    expect(servoDegrees(frame, 1)).toBeCloseTo(40, 1);
    // Only the second servo: the first rests at Home.
    frame = renderSequenceFrame(sequence([arms, [[servo({ position: -45, servo: 2 })]]]), show, 500);
    expect(servoChannels(frame).slice(0, 2)).toEqual([128, 0]);
    expect(servoDegrees(frame, 1)).toBeCloseTo(-45, 1);
    // Steps: four places across 90 degrees, a quarter of a cycle each.
    const stepping = sequence([arms, [[servo({ pattern: "steps", size: 90, speed: 1, servo: 1 })]]]);
    expect([0, 300, 600, 900].map((t) => Math.round(servoDegrees(renderSequenceFrame(stepping, show, t), 0)))).toEqual([-45, -15, 15, 45]);
    // A curve on which servo moves: the first, then the second.
    const handing = servo({ position: 45 });
    handing.curves = { servo: { shape: "ramp", from: 1, to: 2 } };
    const seq = sequence([arms, [[handing]]]);
    const early = renderSequenceFrame(seq, show, 500);
    expect(servoDegrees(early, 0)).toBeCloseTo(45, 1);
    expect(servoChannels(early).slice(2)).toEqual([128, 0]);
    const late = renderSequenceFrame(seq, show, 3500);
    expect(servoChannels(late).slice(0, 2)).toEqual([128, 0]);
    expect(servoDegrees(late, 1)).toBeCloseTo(45, 1);
    // The servos make no light.
    expect(Array.from(renderSequenceFrame(seq, show, 500).slice(9, 15))).toEqual([0, 0, 0, 0, 0, 0]);
  });
});
