import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
    const frames: (Uint8Array | null)[] = [];
    renderHook(() => useLiveFrame((f) => frames.push(f)));
    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(liveFrame.mock.calls.length).toBeLessThanOrEqual(4);
    expect(frames.every((f) => f === null)).toBe(true);
    lit = true;
    await act(() => vi.advanceTimersByTimeAsync(1000));
    const afterIdle = liveFrame.mock.calls.length;
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(liveFrame.mock.calls.length - afterIdle).toBeGreaterThanOrEqual(9);
    expect(frames.at(-1)).toHaveLength(3);
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
    expect(result.current.props.map((p) => p.prop)).toEqual(["new"]);
    expect(result.current.revision).toBe(2);
  });

  it("says when the props' positions can't be read, instead of showing an empty canvas", async () => {
    const backend = new MemoryBackend();
    backend.previewProps = () => Promise.reject(new Error("The props' positions came back damaged. Try again."));
    await connect(backend);
    const { result } = renderHook(() => usePreviewProps());
    await waitFor(() => expect(useApp.getState().error).toBe("The props' positions came back damaged. Try again."));
    expect(result.current.props).toEqual([]);
  });

  it("reports a photo that can't be read, in plain words", async () => {
    const backend = new MemoryBackend();
    await connect(backend);
    const { result } = renderHook(() => useBackgroundImage("/photos/gone.jpg"));
    await act(async () => {});
    expect(result.current.problem).toBe("This photo was moved or deleted. Choose it again with Replace…");
    expect(result.current.image).toBeNull();
  });

  describe("the background photo", () => {
    let created: string[];
    let revoked: string[];
    /** Images "load" with this width (and half as tall). */
    let width: number;

    beforeEach(() => {
      created = [];
      revoked = [];
      width = 800;
      vi.stubGlobal("URL", {
        ...URL,
        createObjectURL: () => {
          const url = `blob:${created.length}`;
          created.push(url);
          return url;
        },
        revokeObjectURL: (url: string) => revoked.push(url),
      });
      vi.stubGlobal(
        "Image",
        class {
          naturalWidth = 0;
          naturalHeight = 0;
          onload: (() => void) | null = null;
          onerror: (() => void) | null = null;
          set src(_: string) {
            this.naturalWidth = width;
            this.naturalHeight = width / 2;
            queueMicrotask(() => this.onload?.());
          }
        },
      );
    });
    afterEach(() => vi.unstubAllGlobals());

    it("loads the photo, then lets go of its temporary address", async () => {
      const backend = new MemoryBackend();
      backend.images.set("/house.png", new Uint8Array([1]));
      await connect(backend);
      const { result } = renderHook(() => useBackgroundImage("/house.png"));
      await waitFor(() => expect(result.current.image).not.toBeNull());
      expect(result.current.aspect).toBe(0.5);
      expect(revoked).toEqual(created);
    });

    it("makes no temporary address for a photo that arrives after it's no longer wanted", async () => {
      const backend = new MemoryBackend();
      let finish!: (bytes: Uint8Array<ArrayBuffer>) => void;
      backend.readImage = () => new Promise((resolve) => (finish = resolve));
      await connect(backend);
      const { unmount } = renderHook(() => useBackgroundImage("/house.png"));
      unmount();
      await act(async () => finish(new Uint8Array([1])));
      expect(created).toEqual([]);
    });

    it("tries again when asked, and when the same file is chosen again", async () => {
      const backend = new MemoryBackend();
      const reads = vi.spyOn(backend, "readImage");
      await connect(backend);
      const { result } = renderHook(() => useBackgroundImage("/house.png"));
      await waitFor(() => expect(result.current.problem).toMatch(/moved or deleted/));
      backend.images.set("/house.png", new Uint8Array([1]));
      act(() => result.current.reload());
      await waitFor(() => expect(result.current.image).not.toBeNull());
      expect(result.current.problem).toBeNull();
      expect(reads).toHaveBeenCalledTimes(2);
    });

    it("scales a huge photo down once", async () => {
      width = 12000;
      const bitmap = { width: 4096 } as ImageBitmap;
      const createImageBitmap = vi.fn(async () => bitmap);
      vi.stubGlobal("createImageBitmap", createImageBitmap);
      const backend = new MemoryBackend();
      backend.images.set("/huge.jpg", new Uint8Array([1]));
      await connect(backend);
      const { result } = renderHook(() => useBackgroundImage("/huge.jpg"));
      await waitFor(() => expect(result.current.image).toBe(bitmap));
      expect(createImageBitmap).toHaveBeenCalledWith(expect.anything(), expect.objectContaining({ resizeWidth: 4096 }));
      expect(result.current.aspect).toBe(0.5);
    });
  });

  it("knows image types by file name", () => {
    expect(imageType("/a/House.JPG")).toBe("image/jpeg");
    expect(imageType("x.svg")).toBe("image/svg+xml");
    expect(imageType("x")).toBe("application/octet-stream");
  });
});
