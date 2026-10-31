import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import { renderSequenceFrame } from "../api/memoryRender";
import { mapFixtures } from "../api/memoryMapping";
import { newEffect, newRow, type Sequence } from "../api/sequence";
import type { PreviewMotion, PreviewProp3d } from "../api/types";
import { motionOf } from "../components/layout3d/scene";
import { aimAt, aimOutline, armAt, armShape, beamAt, beamLight, beamShape, beamWidth, headDirection, previewMotionOf, rotationBasis } from "./beams";
import { newMovingHead, newServo } from "./dmx";

const near = (a: number[], b: number[], within = 1e-9) => a.every((v, i) => Math.abs(v - b[i]) < within);

/** A 16-bit head whose pan is at frame byte 3 (fine 4) and tilt at 5 (fine 6). */
const motion: PreviewMotion = {
  basis: [1, 0, 0, 0, 1, 0, 0, 0, 1],
  beam: {
    pan: { at: 3, fineAt: 4, rest: [128, 0], motor: { rangeDeg: 540, homeDeg: 270 } },
    tilt: { at: 5, fineAt: 6, rest: [128, 0], motor: { rangeDeg: 270, homeDeg: 135 } },
    angleDeg: 30,
    length: 6,
  },
};

describe("moving heads' beams and servos' arms in the previews (the same cases as crates/pf-video's beam.rs)", () => {
  it("point a head where its pan and tilt in the frame say, and up at rest", () => {
    expect(near(headDirection(motion, null), [0, 1, 0], 1e-4)).toBe(true);
    // Pan 90°, tilt 90° (54613 from 135° of 270°): straight right.
    expect(near(headDirection(motion, [0, 0, 0, 170, 170, 213, 85]), [1, 0, 0], 1e-4)).toBe(true);
  });

  it("draw a beam strong where it is narrow, softer where wide, fading out at its end", () => {
    const view = { zoom: 10, ratio: 1 };
    const b = beamShape(50.5, 80, 1, [0, 1, 0], 6, 30, view.zoom, view.ratio);
    const near = beamAt(b, 50, 79);
    expect(near).toBeGreaterThan(0.7);
    const middle = beamAt(b, 50, 49);
    expect(middle).toBeGreaterThan(0.3);
    expect(middle).toBeLessThan(near);
    expect(beamAt(b, 50, 24)).toBeLessThan(middle * 0.8);
    expect([beamAt(b, 50, 85), beamAt(b, 80, 49), beamAt(b, 50, 15)]).toEqual([0, 0, 0]);
    const glow = beamShape(50, 50, 1, [0, 0, 1], 6, 30, view.zoom, view.ratio);
    expect(beamAt(glow, 49, 49)).toBeGreaterThan(0.2);
    expect(beamAt(glow, 70, 49)).toBe(0);
    const arm = armShape(50, 50, [1, 0, 0], view.zoom, view.ratio);
    expect([armAt(arm, 55, 49), armAt(arm, 60, 52), armAt(arm, 50, 49), armAt(arm, 95, 49)]).toEqual([1, 0, 1, 0]);
  });

  it("stop a beam pointing down at the ground, in a flat pool of light", () => {
    const b = beamShape(50, 50, 2, [0, -1, 0], 6, 30, 10, 1);
    expect(beamAt(b, 49, 65)).toBeGreaterThan(0.3);
    expect(beamAt(b, 49, 69)).toBeGreaterThan(0.4);
    expect(beamAt(b, 53, 69)).toBeGreaterThan(0);
    expect(beamAt(b, 49, 75)).toBe(0);
    expect(beamShape(50, 50, 0, [0, 1, 0], 6, 30, 10, 1).pool).toBeNull();
  });

  it("draw a light in its hue, as bright as it is, and nothing when it's dark", () => {
    expect(beamLight(0, 0, 0)).toBeNull();
    expect(beamLight(255, 128, 0)).toEqual({ rgb: (0 << 16) | (128 << 8) | 255, level: 1 });
    expect(beamLight(100, 50, 0)).toEqual({ rgb: (0 << 16) | (128 << 8) | 255, level: 100 / 255 });
  });

  it("widen a beam as its zoom channel says, at rest as it holds", () => {
    const zoomed: PreviewMotion = { ...motion, beam: { ...motion.beam!, zoom: { at: 7, rest: 0, fromDeg: 5, toDeg: 45 } } };
    expect(beamWidth(zoomed, null)).toBe(5);
    expect(beamWidth(zoomed, [0, 0, 0, 0, 0, 0, 0, 255])).toBe(45);
    expect(beamWidth(motion, null)).toBe(30);
  });

  it("outline where a selected head points, in dashes, never as light", () => {
    const b = beamShape(50, 80, 1, [0, 1, 0], 6, 30, 10, 1);
    const path = aimOutline(b);
    expect(path).toHaveLength(3);
    // Along its left edge, dashes and gaps.
    const edge = Array.from({ length: 40 }, (_, k) => aimAt(path, Math.round(50 - (k / 60) * 16), 79 - k, 1) > 0);
    expect(edge.some((on) => on)).toBe(true);
    expect(edge.some((on) => !on)).toBe(true);
    expect(aimAt(path, 50, 50, 1)).toBe(0);
    expect(aimOutline(beamShape(50, 50, 1, [0, 0, 1], 6, 30, 10, 1)).length).toBeGreaterThan(3);
  });

  it("are worked out from a fixture as the engine's preview does, turned as the prop is", () => {
    const show = emptyShow("x");
    const head = newMovingHead(show);
    head.transform.rotationDeg.z = 180;
    show.props.push(head);
    const fixture = mapFixtures(show, 3)[0];
    const found = previewMotionOf(head, fixture)!;
    expect(found.basis).toEqual(rotationBasis(head.transform));
    expect(near(found.basis, [-1, 0, 0, 0, -1, 0, 0, 0, 1])).toBe(true);
    expect(found.beam?.pan).toEqual({ at: 3, fineAt: 4, rest: [128, 0], motor: { rangeDeg: 540, homeDeg: 270 } });
    expect(found.beam?.angleDeg).toBe(15);
    expect(previewMotionOf(newServo(show), { ...fixture, channels: newServo(show).dmx!.channels })?.servos).toHaveLength(1);
  });

  it("go to the 3D view as cones and arms from the cells, a dark head showing none", () => {
    const prop: PreviewProp3d = { prop: "h", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([1, 2, 3]), motion: { ...motion, servos: [motion.beam!.pan!] } };
    const lit = motionOf([prop], new Uint8Array([255, 0, 0, 128, 0, 128, 0]));
    expect(lit[0]).toMatchObject({ kind: "beam", origin: { x: 1, y: 2, z: 3 }, color: [255, 0, 0], level: 1, lands: false, length: 6, angleDeg: 30 });
    expect(lit[1]).toMatchObject({ kind: "arm", origin: { x: 1, y: 2, z: 3 } });
    // Dimmed: the same hue, less bright.
    expect(motionOf([prop], new Uint8Array([100, 50, 0, 128, 0, 128, 0]))[0]).toMatchObject({ color: [255, 128, 0], level: 100 / 255 });
    expect(motionOf([prop], new Uint8Array(7)).map((m) => m.kind)).toEqual(["arm"]);
  });

  it("show no light in 3D while nothing plays, only a selected head's aim", () => {
    const prop: PreviewProp3d = { prop: "h", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([1, 2, 3]), motion };
    expect(motionOf([prop], null)).toEqual([]);
    expect(motionOf([prop], null, new Set(["h"]))).toMatchObject([{ kind: "aim", origin: { x: 1, y: 2, z: 3 }, length: 6, angleDeg: 30 }]);
  });

  it("stop a 3D beam pointing down at the ground, with a pool of light there", () => {
    // Hung 2 up, at Home: straight down.
    const hung: PreviewMotion = { ...motion, basis: [-1, 0, 0, 0, -1, 0, 0, 0, 1] };
    const prop: PreviewProp3d = { prop: "h", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([1, 2, 3]), motion: hung };
    const [beam, pool] = motionOf([prop], new Uint8Array([0, 0, 200, 128, 0, 128, 0]));
    expect(beam).toMatchObject({ kind: "beam", lands: true });
    expect(beam.kind === "beam" && beam.length).toBeCloseTo(2, 3);
    expect(pool).toMatchObject({ kind: "pool", color: [0, 0, 255], level: 200 / 255 });
    expect(pool.kind === "pool" && [pool.at.x, pool.at.y, pool.at.z]).toEqual([expect.closeTo(1, 3), 0, expect.closeTo(3, 3)]);
  });

  it("move in the browser stand-in's frames as a DMX effect sets the head's channels", () => {
    const show = emptyShow("x");
    const head = newMovingHead(show);
    show.props.push(head);
    const effect = newEffect("dmx", 0, 1000);
    effect.params = { kind: "dmx", channels: { "1": 170, "2": 170, "6": 255 } };
    const lit = newEffect("on", 0, 1000, ["#00ff00"]);
    const row = newRow({ prop: head.id });
    row.layers = [{ effects: [lit] }, { effects: [effect] }];
    const doc = { ...({} as Sequence), durationMs: 1000, rows: [row], timingTracks: [] } as Sequence;
    const frame = renderSequenceFrame(doc, show, 500);
    // The cell (3 bytes), then the head's 11 channels: pan as set, tilt at Home, the dimmer full, green.
    expect(Array.from(frame.slice(3, 14))).toEqual([170, 170, 128, 0, 0, 255, 0, 0, 255, 0, 0]);
    expect(Array.from(frame.slice(0, 3))).toEqual([0, 255, 0]);
  });
});
