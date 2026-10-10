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

  it("draws crisp dots, each over what's under it: nothing is added up or blurred", () => {
    const set: [string, unknown][] = [];
    const drawImage = vi.fn();
    const arc = vi.fn();
    const rect = vi.fn();
    const ctx = new Proxy({ drawImage, arc, rect }, { get: (t, k) => (k in t ? t[k as keyof typeof t] : () => {}), set: (_, k, v) => !!set.push([String(k), v]) });
    HTMLCanvasElement.prototype.getContext = (() => ctx) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 600 });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
    render(<PreviewCanvas props={props} frame={new Uint8Array([255, 0, 0, 0, 255, 0])} />);
    // One dot per pixel, in its own color.
    expect(arc.mock.calls.length + rect.mock.calls.length).toBe(2);
    expect(set.filter(([k]) => k === "fillStyle").map(([, v]) => v)).toEqual(["#050505", "rgb(255, 0, 0)", "rgb(0, 255, 0)"]);
    expect(set.some(([k, v]) => k === "globalCompositeOperation" && v !== "source-over")).toBe(false);
    expect(set.some(([k]) => k === "shadowBlur" || k === "filter")).toBe(false);
    expect(drawImage).not.toHaveBeenCalled();
  });

  it("is an image with a name, and redraws when it is resized", () => {
    const fillRect = vi.fn();
    const ctx = new Proxy({ fillRect }, { get: (t, k) => (k in t ? t[k as keyof typeof t] : () => {}), set: () => true });
    HTMLCanvasElement.prototype.getContext = (() => ctx) as unknown as typeof HTMLCanvasElement.prototype.getContext;
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
