import { act, render } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useElementWidth, useWindowBand } from "./useWidth";

function resize(width: number) {
  act(() => {
    window.innerWidth = width;
    window.dispatchEvent(new Event("resize"));
  });
}

describe("the window's band", () => {
  it("is narrow, medium, laptop, or wide", () => {
    let band = "";
    function Probe() {
      band = useWindowBand();
      return null;
    }
    render(<Probe />);
    for (const [width, expected] of [
      [1100, "narrow"],
      [1200, "medium"],
      [1280, "laptop"],
      [1439, "laptop"],
      [1440, "wide"],
    ] as const) {
      resize(width);
      expect(band).toBe(expected);
    }
  });

  it("re-renders only when the window crosses into another band, not for every pixel", () => {
    let renders = 0;
    function Probe() {
      renders++;
      useWindowBand();
      return null;
    }
    resize(1500);
    render(<Probe />);
    const before = renders;
    for (let w = 1501; w < 1900; w += 7) resize(w);
    expect(renders).toBe(before);
    resize(1300);
    expect(renders).toBe(before + 1);
  });
});

describe("an element's measured arrangement", () => {
  let observed: (() => void) | null = null;
  const original = globalThis.ResizeObserver;
  afterEach(() => {
    globalThis.ResizeObserver = original;
    vi.restoreAllMocks();
  });

  it("re-renders only when what's derived from the width changes", () => {
    let width = 1000;
    globalThis.ResizeObserver = class {
      constructor(cb: () => void) {
        observed = cb;
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(() => ({ width, height: 10 }) as DOMRect);
    let renders = 0;
    let wide: boolean | null = null;
    function Probe() {
      renders++;
      const ref = useRef<HTMLDivElement>(null);
      wide = useElementWidth(ref, (w) => (w === null ? null : w >= 800));
      return <div ref={ref} />;
    }
    render(<Probe />);
    expect(wide).toBe(true);
    const settled = renders;
    for (width = 1001; width < 1400; width += 13) act(() => observed?.());
    expect(renders).toBe(settled);
    width = 700;
    act(() => observed?.());
    expect(wide).toBe(false);
    expect(renders).toBe(settled + 1);
  });
});
