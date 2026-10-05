import { act, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend } from "../api/memory";
import type { PreviewProp } from "../api/types";
import { useApp } from "../state/store";
import { LayoutPreview } from "./LayoutPreview";

const prop = (id: string): PreviewProp => ({ prop: id, frameOffset: 0, channelsPerPixel: 3, points: [0, 0] });

async function mount(backend: MemoryBackend) {
  const snapshot = await backend.getSnapshot();
  useApp.setState({ backend, snapshot });
  return render(<LayoutPreview />);
}

describe("LayoutPreview", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("checks for frames slowly while nothing plays and quickly while frames arrive", async () => {
    vi.useFakeTimers();
    const backend = new MemoryBackend();
    backend.previewProps = async () => [prop("p1")];
    let lit = false;
    const liveFrame = vi.fn(async () => new Uint8Array(lit ? 3 : 0));
    backend.liveFrame = liveFrame;
    await mount(backend);
    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(liveFrame.mock.calls.length).toBeLessThanOrEqual(4);
    lit = true;
    await act(() => vi.advanceTimersByTimeAsync(1000));
    const afterIdle = liveFrame.mock.calls.length;
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(liveFrame.mock.calls.length - afterIdle).toBeGreaterThanOrEqual(9);
  });

  it("ignores an outline that arrives after a newer one was asked for", async () => {
    const backend = new MemoryBackend();
    backend.liveFrame = async () => new Uint8Array(0);
    const answers: ((p: PreviewProp[]) => void)[] = [];
    backend.previewProps = () => new Promise((resolve) => answers.push(resolve));
    const view = await mount(backend);
    const snapshot = useApp.getState().snapshot!;
    act(() => useApp.setState({ snapshot: { ...snapshot, revision: snapshot.revision + 1 } }));
    expect(answers).toHaveLength(2);
    await act(async () => answers[1]([prop("new")]));
    await act(async () => answers[0]([]));
    // The newer answer (one prop) stands, so the preview stays drawn.
    expect(view.container.querySelector("canvas")).not.toBeNull();
  });
});
