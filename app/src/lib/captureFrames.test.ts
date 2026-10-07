import { describe, expect, it, vi } from "vitest";
import { MemoryBackend } from "../api/memory";
import { demoShow } from "../api/demo";
import { cameraOwners } from "../api/memoryCameraMap";
import { readCapture } from "./captureFrames";
import { slotCount } from "./cameraMap";

describe("reading a camera-mapping capture", () => {
  it("finds the sequence, averages each slot, and sends the frames to be decoded", async () => {
    const backend = new MemoryBackend(demoShow());
    const arch = backend.show.props[0];
    const target = { type: "prop" as const, id: arch.id };
    const info = await backend.cameraMapTarget(target, "four");
    const source = backend.sampleCapture(target, "four");
    const decode = vi.spyOn(backend, "cameraMapDecode");
    const steps = new Set<string>();
    const read = await readCapture(source, backend, info.pixels, "four", (p) => steps.add(p.step));
    expect([...steps]).toEqual(["sync", "frames", "decode"]);
    const [frames, sent] = decode.mock.calls[0];
    expect(sent).toEqual({ width: 960, height: 540, pixels: info.pixels, base: "four" });
    expect(frames.length).toBe(slotCount(info.pixels, "four") * 960 * 540 * 3);
    // The review picture has the pixels lit: brighter where the first pixel is than in a corner.
    const first = read.decoded.pixels[0];
    const at = (x: number, y: number) => read.picture[(Math.round(y) * 960 + Math.round(x)) * 4];
    expect(at(first.x, first.y)).toBeGreaterThan(at(5, 5) + 100);
    // The second slot (white) lights the pixel in the averaged frames sent.
    const each = 960 * 540 * 3;
    const lit = frames[each + (Math.round(first.y) * 960 + Math.round(first.x)) * 3];
    const dark = frames[(Math.round(first.y) * 960 + Math.round(first.x)) * 3];
    expect(lit).toBeGreaterThan(dark + 100);
  });

  it("stops when cancelled", async () => {
    const backend = new MemoryBackend(demoShow());
    const target = { type: "show" as const };
    const source = backend.sampleCapture(target, "four");
    const abort = new AbortController();
    abort.abort();
    await expect(readCapture(source, backend, cameraOwners(backend.show, target).length, "four", () => undefined, abort.signal)).rejects.toThrow(
      /cancelled/,
    );
  });
});
