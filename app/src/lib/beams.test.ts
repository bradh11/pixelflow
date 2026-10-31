import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import { renderSequenceFrame } from "../api/memoryRender";
import { mapFixtures } from "../api/memoryMapping";
import { newEffect, newRow, type Sequence } from "../api/sequence";
import type { PreviewMotion, PreviewProp3d } from "../api/types";
import { motionOf } from "../components/layout3d/scene";
import { armAt, armShape, beamAt, beamShape, headDirection, previewMotionOf, rotationBasis } from "./beams";
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

  it("draw a beam bright along its middle, fading along it, with a round end", () => {
    const view = { zoom: 10, ratio: 1 };
    const b = beamShape(50.5, 80, [0, 1, 0], 6, 30, view.zoom, view.ratio);
    expect(beamAt(b, 50, 79)).toBeGreaterThan(0.95);
    expect(Math.abs(beamAt(b, 50, 49) - (1 - (0.6 * 30.5) / 60))).toBeLessThan(0.05);
    expect(beamAt(b, 50, 85)).toBe(0);
    expect(beamAt(b, 80, 49)).toBe(0);
    expect(beamAt(b, 50, 15)).toBeGreaterThan(0);
    const pool = beamShape(50, 50, [0, 0, 1], 6, 30, view.zoom, view.ratio);
    expect(beamAt(pool, 49, 49)).toBeGreaterThan(0.3);
    expect(beamAt(pool, 70, 49)).toBe(0);
    const arm = armShape(50, 50, [1, 0, 0], view.zoom, view.ratio);
    expect([armAt(arm, 55, 49), armAt(arm, 60, 52), armAt(arm, 50, 49), armAt(arm, 95, 49)]).toEqual([1, 0, 1, 0]);
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
    expect(lit[0]).toMatchObject({ kind: "beam", origin: { x: 1, y: 2, z: 3 }, color: [255, 0, 0], lit: true, length: 6, angleDeg: 30 });
    expect(lit[1]).toMatchObject({ kind: "arm", origin: { x: 1, y: 2, z: 3 } });
    expect(motionOf([prop], new Uint8Array(7)).map((m) => m.kind)).toEqual(["arm"]);
    expect(motionOf([prop], null)[0]).toMatchObject({ kind: "beam", lit: false });
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
