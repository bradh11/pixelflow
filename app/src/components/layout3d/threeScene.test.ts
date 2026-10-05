import { Vector3 } from "three";
import { describe, expect, it } from "vitest";
import { modelPoint, v3 } from "../../lib/layout3d";
import { modelEuler } from "./threeScene";

describe("the three.js scene", () => {
  it("turns the house model in the same order as props (X, then Y, then Z, about the layout's axes)", () => {
    const rotationDeg = v3(-90, 30, 0);
    // A model made Z-up, stood up with a tilt of -90 and turned 30: its up stays up.
    const up = new Vector3(0, 0, 1).applyEuler(modelEuler(rotationDeg));
    expect(up.x).toBeCloseTo(0);
    expect(up.y).toBeCloseTo(1);
    expect(up.z).toBeCloseTo(0);
    // Any turn agrees with how props' pixels are placed.
    for (const r of [v3(-90, 30, 0), v3(20, -45, 70), v3(180, 90, -30)]) {
      const p = new Vector3(1.5, -2, 0.75).applyEuler(modelEuler(r));
      const q = modelPoint(v3(1.5, -2, 0.75), { position: v3(0, 0, 0), rotationDeg: r, scale: 1 });
      expect([p.x, p.y, p.z].map((n) => n.toFixed(6))).toEqual([q.x, q.y, q.z].map((n) => n.toFixed(6)));
    }
  });
});
