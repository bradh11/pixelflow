import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend } from "../../api/memory";
import type { PreviewProp, PreviewSet } from "../../api/types";
import { useApp } from "../../state/store";
import { imageType, useBackgroundImage, useLiveFrame, usePreviewProps } from "./useLayoutData";

const prop = (id: string): PreviewProp => ({ prop: id, frameOffset: 0, channelsPerPixel: 3, points: [0, 0] });

async function connect(backend: MemoryBackend) {
  useApp.setState({ backend, snapshot: await backend.getSnapshot() });
}

describe("layout data", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("checks for frames slowly while nothing plays and quickly while frames arrive", async () => {
    vi.useFakeTimers();
    const backend = new MemoryBackend();
    let lit = false;
    const liveFrame = vi.fn(async () => new Uint8Array(lit ? 3 : 0));
    backend.liveFrame = liveFrame;
    await connect(backend);
    const { result } = renderHook(() => useLiveFrame());
    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(liveFrame.mock.calls.length).toBeLessThanOrEqual(4);
    expect(result.current).toBeNull();
    lit = true;
    await act(() => vi.advanceTimersByTimeAsync(1000));
    const afterIdle = liveFrame.mock.calls.length;
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(liveFrame.mock.calls.length - afterIdle).toBeGreaterThanOrEqual(9);
    expect(result.current).toHaveLength(3);
  });

  it("ignores pixel positions that arrive after newer ones were asked for", async () => {
    const backend = new MemoryBackend();
    const answers: ((p: PreviewSet) => void)[] = [];
    backend.previewProps = () => new Promise((resolve) => answers.push(resolve));
    await connect(backend);
    const { result } = renderHook(() => usePreviewProps());
    const snapshot = useApp.getState().snapshot!;
    act(() => useApp.setState({ snapshot: { ...snapshot, revision: snapshot.revision + 1 } }));
    expect(answers).toHaveLength(2);
    await act(async () => answers[1]({ revision: 2, props: [prop("new")] }));
    await act(async () => answers[0]({ revision: 1, props: [] }));
    expect(result.current.map((p) => p.prop)).toEqual(["new"]);
  });

  it("reports a photo that can't be read, in plain words", async () => {
    const backend = new MemoryBackend();
    await connect(backend);
    const { result } = renderHook(() => useBackgroundImage("/photos/gone.jpg"));
    await act(async () => {});
    expect(result.current.problem).toBe("This photo was moved or deleted. Choose it again with Replace…");
    expect(result.current.image).toBeNull();
  });

  it("knows image types by file name", () => {
    expect(imageType("/a/House.JPG")).toBe("image/jpeg");
    expect(imageType("x.svg")).toBe("image/svg+xml");
    expect(imageType("x")).toBe("application/octet-stream");
  });
});
