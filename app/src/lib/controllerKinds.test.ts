import { describe, expect, it } from "vitest";
import { CONTROLLER_KINDS, controllerOfKind, guessKind } from "./controllerKinds";

describe("controller kinds", () => {
  it("a Falcon comes with xLights' port limit for the ports in use", () => {
    const c = controllerOfKind("f16v5", "Garage", "10.0.0.20", "ddp", 16);
    expect(c.adapter).toBe("falcon");
    expect(c.ports).toHaveLength(16);
    expect(c.ports.every((p) => p.maxPixels === 1024)).toBe(true);
    // With an expansion board: 32 ports still drive 1,024 each; past 32, 704.
    expect(controllerOfKind("f16v5", "G", "10.0.0.20", "ddp", 32).ports.every((p) => p.maxPixels === 1024)).toBe(true);
    expect(controllerOfKind("f48v5", "G", "10.0.0.20", "ddp", 48).ports.every((p) => p.maxPixels === 704)).toBe(true);
  });

  it("other controllers use the port count given and don't guess a limit", () => {
    const c = controllerOfKind("other", "Mine", "10.0.0.21", "sacn", 3);
    expect(c.adapter).toBe("generic");
    expect(c.ports.map((p) => p.number)).toEqual([1, 2, 3]);
    expect(c.ports.every((p) => p.maxPixels === null)).toBe(true);
    expect(controllerOfKind("wled", "Porch", "10.0.0.22", "ddp", 1).adapter).toBe("wled");
  });

  it("guesses a type from a scan of the address, else an FPP from the name or address, else nothing", () => {
    const found = [
      { address: "10.0.0.5", kind: "fpp" as const, name: "Show Pi", model: "Pi 4" },
      { address: "10.0.0.6", kind: "falcon" as const, name: "Garage", model: "F16v5" },
      { address: "10.0.0.7", kind: "falcon" as const, name: "Odd", model: "F2V4" },
      { address: "10.0.0.8", kind: "wled" as const, name: "Porch", model: "esp32" },
    ];
    expect(guessKind("Controller 1", " 10.0.0.5 ", found)).toEqual({ id: "fpp", short: "FPP", why: "A scan found an FPP at this address." });
    expect(guessKind("Controller 1", "10.0.0.6", found)).toMatchObject({ id: "f16v5", short: "Falcon F16V5" });
    expect(guessKind("Controller 1", "10.0.0.8", found)).toMatchObject({ id: "wled" });
    expect(guessKind("Main FPP", "10.0.0.9", found)).toEqual({ id: "fpp", short: "FPP", why: "Is it an FPP?" });
    expect(guessKind("Controller 1", "fpp.local")).toMatchObject({ id: "fpp" });
    // What a scan found goes before the name; a board it doesn't know, or nothing to go on: no guess.
    expect(guessKind("Old FPP", "10.0.0.8", found)?.id).toBe("wled");
    expect(guessKind("Controller 1", "10.0.0.7", found)).toBeNull();
    expect(guessKind("Mapper", "10.0.0.9", found)).toBeNull();
  });

  it("lists other first, then every known board", () => {
    expect(CONTROLLER_KINDS[0].id).toBe("other");
    expect(CONTROLLER_KINDS.filter((k) => k.maxPixels !== null).map((k) => k.label)).toEqual([
      "Falcon F16V5",
      "Falcon F32V5",
      "Falcon F48V5",
      "Falcon F16V4",
      "Falcon F48V4",
    ]);
  });
});
