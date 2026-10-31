import { describe, expect, it } from "vitest";
import type { DmxChannel } from "../api/types";
import { armDirection, axes, axisValues, beamDirection, cosDeg, limits, motorDegrees, motorValues, restMotorsAt, sinDeg, UPRIGHT } from "./motion";

const near = (a: number[], b: number[]) => a.every((v, i) => Math.abs(v - b[i]) < 1e-9);

describe("Moving heads and servos, as the engine works them out (the same cases as crates/pf-model/src/motion.rs)", () => {
  it("turn a position into a 16-bit pan's values and back", () => {
    const pan = { rangeDeg: 540, homeDeg: 270 };
    expect(motorValues(pan, 0, true)).toEqual([128, 0]);
    expect(motorValues(pan, 90, true)).toEqual([170, 170]);
    expect(motorValues(pan, -270, true)).toEqual([0, 0]);
    expect(motorValues(pan, 999, true)).toEqual([255, 255]);
    expect(motorValues(pan, NaN, true)).toEqual([128, 0]);
    for (const degrees of [-270, -100.25, 0, 33.3, 270]) {
      const [c, f] = motorValues(pan, degrees, true);
      expect(Math.abs(motorDegrees(pan, c, f) - degrees)).toBeLessThanOrEqual(540 / 65535);
    }
  });

  it("turn a reversed 8-bit tilt the other way and hold a servo within its limits", () => {
    expect(motorValues({ rangeDeg: 270, homeDeg: 135 }, 45, false)).toEqual([170, 0]);
    expect(motorValues({ rangeDeg: 270, homeDeg: 135, reverse: true }, 45, false)).toEqual([85, 0]);
    const servo = { rangeDeg: 180, homeDeg: 90, minDeg: -45, maxDeg: 45 };
    expect(limits(servo)).toEqual([-45, 45]);
    expect(motorValues(servo, 90, false)).toEqual([191, 0]);
    expect(motorValues(servo, -90, false)).toEqual([64, 0]);
    expect(limits({ rangeDeg: 180, homeDeg: 30, minDeg: -90 })).toEqual([-30, 150]);
  });

  it("pair each fine channel with its own motor, wherever it is", () => {
    const channels: DmxChannel[] = [
      { name: "Pan", role: "pan" },
      { name: "Tilt", role: "tilt" },
      { name: "Pan fine", role: "panFine" },
      { name: "Tilt fine", role: "tiltFine" },
    ];
    const found = axes(channels);
    expect(found.map((a) => [a.role, a.coarse, a.fine])).toEqual([
      ["pan", 0, 2],
      ["tilt", 1, 3],
    ]);
    expect(found[0].motor).toEqual({ rangeDeg: 540, homeDeg: 270 });
    expect(axisValues(found[0], 90)).toEqual([170, 170]);
    expect(restMotorsAt(channels, 0).map((c) => c.value ?? 0)).toEqual([128, 128, 0, 0]);
  });

  it("work out the sine and cosine to two billionths, by the engine's steps", () => {
    for (let k = -2000; k <= 2000; k++) {
      const d = k * 0.37;
      expect(Math.abs(sinDeg(d) - Math.sin((d * Math.PI) / 180))).toBeLessThan(2e-9);
      expect(Math.abs(cosDeg(d) - Math.cos((d * Math.PI) / 180))).toBeLessThan(2e-9);
    }
    expect([sinDeg(0), cosDeg(0), sinDeg(90), cosDeg(180), sinDeg(Infinity)]).toEqual([0, 1, 1, -1, 0]);
  });

  it("point a head up at Home, toward the street when tilted, and to the right when panned", () => {
    expect(near(beamDirection(0, 0, UPRIGHT), [0, 1, 0])).toBe(true);
    expect(near(beamDirection(0, 90, UPRIGHT), [0, 0, 1])).toBe(true);
    expect(near(beamDirection(90, 90, UPRIGHT), [1, 0, 0])).toBe(true);
    const hung = [-1, 0, 0, 0, -1, 0, 0, 0, 1];
    expect(near(beamDirection(0, 0, hung), [0, -1, 0])).toBe(true);
    expect(near(beamDirection(90, 90, hung), [-1, 0, 0])).toBe(true);
    expect(near(armDirection(90, UPRIGHT), [1, 0, 0])).toBe(true);
  });
});
