import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

/** The Sequence screen with the demo minute, zoomed in so a page shows about 15 s. */
async function openZoomed() {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, width: 1000, height: 600, right: 1000, bottom: 600, toJSON: () => ({}) });
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  for (let i = 0; i < 3; i++) fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
  return { backend, seq, user };
}

const timeInput = () => screen.getByRole("slider", { name: "Scroll in time" });
const startMs = () => Number((timeInput() as HTMLInputElement).value);
const scrollTo = (ms: number) => fireEvent.change(timeInput(), { target: { value: String(ms) } });
/** An effect in view from 30 s on, so revealing the selection wouldn't move the view. */
const effectAt30s = () =>
  useSequencer
    .getState()
    .doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects))
    .find((e) => e.startMs < 40_000 && e.endMs > 32_000)!;

afterEach(async () => {
  await act(() => useSequencer.getState().stop());
  act(() => useSequencer.getState().setLooping(false));
  vi.restoreAllMocks();
  localStorage.removeItem("pixelflow.sequenceLoop");
});

describe("sequence transport", () => {
  it("stops where it is, then a second Stop goes back to the start of the timeline and the preview", async () => {
    const { user, seq } = await openZoomed();
    const stop = () => screen.getByRole("button", { name: /^(Stop|Back to the start)$/ });
    expect(stop()).toHaveAccessibleName("Stop");
    expect(stop()).toBeDisabled();

    await user.click(within(screen.getByRole("toolbar", { name: "Sequence" })).getByRole("button", { name: "Play" }));
    await waitFor(() => expect(useSequencer.getState().status?.state).toBe("playing"));
    expect(stop()).toHaveAccessibleName("Stop");
    expect(stop()).toBeEnabled();
    await act(() => useSequencer.getState().seek(40_000));
    await user.click(stop());
    expect(useSequencer.getState().status).toBeNull();
    // The playhead stays where playback was.
    expect(useSequencer.getState().playheadMs).toBeGreaterThanOrEqual(40_000);
    expect(useSequencer.getState().playheadMs).toBeLessThan(41_000);

    // Stopped away from the start: the button says what pressing it again does.
    expect(stop()).toHaveAccessibleName("Back to the start");
    expect(stop()).toHaveAttribute("title", "Back to the start");
    // Even with an effect selected and the view elsewhere, it goes back to the very start.
    act(() => useSequencer.getState().select([effectAt30s().id]));
    scrollTo(30_000);
    const frames = vi.spyOn(seq, "sequenceDocFrame");
    await user.click(stop());
    expect(useSequencer.getState().playheadMs).toBe(0);
    await waitFor(() => expect(startMs()).toBe(0));
    await waitFor(() => expect(frames).toHaveBeenLastCalledWith(0));
    expect(screen.getByText(/^0:00\.000/)).toBeInTheDocument();
    expect(stop()).toHaveAccessibleName("Stop");
    expect(stop()).toBeDisabled();
  });

  it("Home goes back to the start of the timeline even with an effect selected", async () => {
    const { user } = await openZoomed();
    act(() => useSequencer.getState().setPlayhead(40_000));
    act(() => useSequencer.getState().select([effectAt30s().id]));
    scrollTo(30_000);
    await user.keyboard("{Home}");
    expect(useSequencer.getState().playheadMs).toBe(0);
    await waitFor(() => expect(startMs()).toBe(0));
  });

  it("loops with the Loop button or L, remembering the choice", async () => {
    const { user, seq } = await openZoomed();
    const loop = screen.getByRole("button", { name: "Loop playback" });
    expect(loop).toHaveAttribute("aria-pressed", "false");
    expect(loop).toHaveAttribute("title", "Loop playback (L)");
    await user.click(loop);
    expect(loop).toHaveAttribute("aria-pressed", "true");
    expect(localStorage.getItem("pixelflow.sequenceLoop")).toBe("true");
    await waitFor(() => expect(seq.calls).toContain("setSequenceDocLoop:true"));

    // From the keyboard, but not while typing or with ⌘.
    screen.getByRole("application", { name: "Timeline" }).focus();
    await user.keyboard("{Meta>}l{/Meta}");
    expect(loop).toHaveAttribute("aria-pressed", "true");
    await user.keyboard("l");
    expect(loop).toHaveAttribute("aria-pressed", "false");
    expect(localStorage.getItem("pixelflow.sequenceLoop")).toBe("false");
    await user.keyboard("L");
    expect(loop).toHaveAttribute("aria-pressed", "true");

    // Playing past the end goes round again instead of stopping.
    await user.click(within(screen.getByRole("toolbar", { name: "Sequence" })).getByRole("button", { name: "Play" }));
    await waitFor(() => expect(useSequencer.getState().status?.looping).toBe(true));
  });
});
