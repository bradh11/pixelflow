import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreviewProp } from "../api/types";
import { PreviewCanvas } from "./PreviewCanvas";

const props: PreviewProp[] = [{ prop: "p1", frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 1, 1] }];

describe("PreviewCanvas", () => {
  const getContext = HTMLCanvasElement.prototype.getContext;
  const ResizeObserverBefore = globalThis.ResizeObserver;
  const sizes = {
    clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
    clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight"),
  };
  afterEach(() => {
    HTMLCanvasElement.prototype.getContext = getContext;
    globalThis.ResizeObserver = ResizeObserverBefore;
    for (const [key, d] of Object.entries(sizes)) if (d) Object.defineProperty(HTMLElement.prototype, key, d);
  });

  /**
   * A context for every canvas that does nothing but what `own` gives it, makes images to stamp
   * dots into, and records what's set on it.
   */
  function fakeContext<T extends object>(own: T) {
    const set: [string, unknown][] = [];
    const base = {
      get canvas() {
        return document.querySelector("canvas");
      },
      createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h }),
      ...own,
    };
    const ctx = new Proxy(base, { get: (t, k) => (k in t ? t[k as keyof typeof t] : () => {}), set: (_, k, v) => !!set.push([String(k), v]) });
    HTMLCanvasElement.prototype.getContext = (() => ctx) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    return set;
  }

  it("draws crisp dots, each in its own color: nothing is added up or blurred", () => {
    const drawImage = vi.fn();
    const putImageData = vi.fn<(image: { data: Uint8ClampedArray; width: number }) => void>();
    const set = fakeContext({ drawImage, putImageData });
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 600 });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
    render(<PreviewCanvas props={props} frame={new Uint8Array([255, 0, 0, 0, 255, 0])} />);
    // One picture, holding a dot for each pixel in its own color.
    expect(drawImage).toHaveBeenCalledTimes(1);
    expect(putImageData).toHaveBeenCalledTimes(1);
    const { data, width } = putImageData.mock.calls[0][0];
    const at = (x: number, y: number) => Array.from(data.subarray((y * width + x) * 4, (y * width + x) * 4 + 4));
    // The pixels are fitted into the canvas: one at its bottom left, the other at its top right.
    expect(at(124, 376)).toEqual([255, 0, 0, 255]);
    expect(at(476, 24)).toEqual([0, 255, 0, 255]);
    const colors = new Set<string>();
    for (let i = 0; i < data.length; i += 4) if (data[i + 3] > 0) colors.add(`${data[i]},${data[i + 1]},${data[i + 2]}`);
    expect([...colors].sort()).toEqual(["0,255,0", "255,0,0"]);
    expect(set.filter(([k]) => k === "fillStyle").map(([, v]) => v)).toEqual(["#050505"]);
    expect(set.some(([k, v]) => k === "globalCompositeOperation" && v !== "source-over")).toBe(false);
    expect(set.some(([k]) => k === "shadowBlur" || k === "filter")).toBe(false);
  });

  it("is an image with a name, and redraws when it is resized", () => {
    const fillRect = vi.fn();
    fakeContext({ fillRect });
    let resized: () => void = () => {};
    globalThis.ResizeObserver = class {
      constructor(callback: () => void) {
        resized = callback;
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;

    render(<PreviewCanvas props={props} frame={null} />);
    expect(screen.getByRole("img", { name: "Preview" })).toBeInTheDocument();
    const drawn = fillRect.mock.calls.length;
    expect(drawn).toBeGreaterThan(0);
    resized();
    expect(fillRect.mock.calls.length).toBeGreaterThan(drawn);
  });
});
