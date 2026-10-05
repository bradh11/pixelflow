import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreviewProp } from "../api/types";
import { PreviewCanvas } from "./PreviewCanvas";

const props: PreviewProp[] = [{ prop: "p1", frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 1, 1] }];

describe("PreviewCanvas", () => {
  const getContext = HTMLCanvasElement.prototype.getContext;
  const ResizeObserverBefore = globalThis.ResizeObserver;
  afterEach(() => {
    HTMLCanvasElement.prototype.getContext = getContext;
    globalThis.ResizeObserver = ResizeObserverBefore;
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
