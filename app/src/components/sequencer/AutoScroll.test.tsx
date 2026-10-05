import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";

// Above the rows: ruler 24 + music 44 + Beats 18 + Bars 18 = 104 px. Rows are 30 px a lane:
// Mega Tree (two layers) 104–164, Garage Arch 164–194, Window Matrix 194–224, Porch Star 224–254.
const TICK_MS = 16;

/** The timeline `height` px tall, zoomed in so a page shows about 15 s, from the start of the song. */
async function openZoomed(height = 600) {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, width: 1000, height, right: 1000, bottom: height, toJSON: () => ({}) });
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  render(<App />);
  // Only the timeline's scroll ticks run on the fake clock.
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
  for (let i = 0; i < 3; i++) fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
  fireEvent.change(timeInput(), { target: { value: "0" } });
  const effects = (name: string) => {
    const prop = show.props.find((p) => p.name === name)!;
    return seq.doc!.rows.find((r) => "prop" in r.target && r.target.prop === prop.id)!.layers.flatMap((l) => l.effects);
  };
  return { seq, effects };
}

const timeline = () => screen.getByRole("application", { name: "Timeline" });
const timeInput = () => screen.getByRole("slider", { name: "Scroll in time" });
const startMs = () => Number((timeInput() as HTMLInputElement).value);
const ticks = (n: number) => act(() => vi.advanceTimersByTime(n * TICK_MS));

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("scrolling the timeline while dragging", () => {
  it("scrolls on in time while an effect is held near the right edge, faster further in, and stops when it's let go", async () => {
    const { seq, effects } = await openZoomed();
    const first = effects("Porch Star")[0];
    expect(startMs()).toBe(0);
    fireEvent.pointerDown(timeline(), { clientX: 10, clientY: 239, button: 0, pointerId: 1 });
    fireEvent.pointerMove(timeline(), { clientX: 500, clientY: 239, pointerId: 1 });
    ticks(10);
    expect(startMs(), "not near an edge").toBe(0);
    // 10 px into the edge.
    fireEvent.pointerMove(timeline(), { clientX: 986, clientY: 239, pointerId: 1 });
    ticks(10);
    const slow = startMs();
    expect(slow).toBeGreaterThan(0);
    // Past the edge: faster.
    fireEvent.pointerMove(timeline(), { clientX: 1040, clientY: 239, pointerId: 1 });
    ticks(10);
    expect(startMs() - slow).toBeGreaterThan(slow * 2);
    const scrolled = startMs();
    fireEvent.pointerUp(timeline(), { clientX: 1040, clientY: 239, pointerId: 1 });
    ticks(20);
    expect(startMs(), "let go: it stops").toBe(scrolled);
    // The effect went with the page: it's dropped where the pointer was, well past the first page.
    await act(async () => {});
    const moved = seq.doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects)).find((e) => e.id === first.id)!;
    expect(moved.startMs).toBeGreaterThan(15_000);
  });

  it("doesn't scroll for a press near the edge that hasn't become a drag yet", async () => {
    await openZoomed();
    // A page further on, so the view could scroll back to the start.
    fireEvent.change(timeInput(), { target: { value: "900" } });
    const from = startMs();
    expect(from).toBeGreaterThan(0);
    // 3 zoom-ins of 1.6 from the whole minute in 1000 px.
    const x = (ms: number) => (ms - from) * (1000 / 60_000) * 1.6 ** 3;
    // The star's effect at 1 s and the beat at 1 s both start just inside the left edge.
    for (const y of [239, 77]) {
      fireEvent.pointerDown(timeline(), { clientX: x(1000) + 10, clientY: y, button: 0, pointerId: 1 });
      fireEvent.pointerMove(timeline(), { clientX: x(1000) + 11, clientY: y, pointerId: 1 });
      ticks(10);
      expect(startMs(), "a press with a little jitter").toBe(from);
      fireEvent.pointerUp(timeline(), { clientX: x(1000) + 11, clientY: y, pointerId: 1 });
    }
  });

  it("stops scrolling on Escape, which also calls the drag off", async () => {
    const { effects } = await openZoomed();
    const first = effects("Porch Star")[0];
    act(() => useSequencer.getState().select([first.id]));
    fireEvent.pointerDown(timeline(), { clientX: 10, clientY: 239, button: 0, pointerId: 1 });
    fireEvent.pointerMove(timeline(), { clientX: 995, clientY: 239, pointerId: 1 });
    ticks(10);
    const scrolled = startMs();
    expect(scrolled).toBeGreaterThan(0);
    fireEvent.keyDown(window, { key: "Escape" });
    ticks(20);
    expect(startMs()).toBe(scrolled);
    fireEvent.pointerUp(timeline(), { clientX: 995, clientY: 239, pointerId: 1 });
    await act(async () => {});
    expect(effects("Porch Star")[0].startMs, "nothing moved").toBe(first.startMs);
    expect(useSequencer.getState().selection, "Escape only called off the drag").toEqual([first.id]);
  });

  it("scrolls the rows while an effect is held near the bottom", async () => {
    // 220 px tall: the rows show from 104 to 220, so the star's row is out of sight.
    await openZoomed(220);
    const rows = () => Number((screen.getByRole("slider", { name: "Scroll rows" }) as HTMLInputElement).value);
    expect(rows()).toBe(0);
    fireEvent.pointerDown(timeline(), { clientX: 10, clientY: 119, button: 0, pointerId: 1 });
    fireEvent.pointerMove(timeline(), { clientX: 10, clientY: 214, pointerId: 1 });
    ticks(5);
    expect(rows()).toBeGreaterThan(0);
    fireEvent.pointerUp(timeline(), { clientX: 10, clientY: 214, pointerId: 1 });
  });

  it("scrolls in time while a timing mark is dragged near an edge", async () => {
    await openZoomed();
    // A beat on the Beats track (68–86 px).
    fireEvent.pointerDown(timeline(), { clientX: 15, clientY: 77, button: 0, pointerId: 1 });
    fireEvent.pointerMove(timeline(), { clientX: 990, clientY: 77, pointerId: 1 });
    ticks(10);
    expect(startMs()).toBeGreaterThan(0);
    fireEvent.pointerUp(timeline(), { clientX: 990, clientY: 77, pointerId: 1 });
  });
});
