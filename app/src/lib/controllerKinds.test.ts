import { describe, expect, it } from "vitest";
import { CONTROLLER_KINDS, controllerOfKind } from "./controllerKinds";

describe("controller kinds", () => {
  it("a Falcon V5 comes with its ports and their 1,024-pixel limit", () => {
    const c = controllerOfKind("f16v5", "Garage", "10.0.0.20", "ddp", 4);
    expect(c.adapter).toBe("falcon");
    expect(c.ports).toHaveLength(16);
    expect(c.ports.every((p) => p.maxPixels === 1024)).toBe(true);
  });

  it("other controllers use the port count given and don't guess a limit", () => {
    const c = controllerOfKind("other", "Mine", "10.0.0.21", "sacn", 3);
    expect(c.adapter).toBe("generic");
    expect(c.ports.map((p) => p.number)).toEqual([1, 2, 3]);
    expect(c.ports.every((p) => p.maxPixels === null)).toBe(true);
    expect(controllerOfKind("wled", "Porch", "10.0.0.22", "ddp", 1).adapter).toBe("wled");
  });

  it("lists other first, then every known board", () => {
    expect(CONTROLLER_KINDS[0].id).toBe("other");
    expect(CONTROLLER_KINDS.filter((k) => k.maxPixels === 1024).map((k) => k.label)).toEqual([
      "Falcon F16V5",
      "Falcon F32V5",
      "Falcon F48V5",
      "Falcon F16V4",
      "Falcon F48V4",
    ]);
  });
});
