import { act, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { demoDevices, demoPlayers } from "../../api/demo";
import { MemoryBackend, emptyShow } from "../../api/memory";
import { useApp } from "../../state/store";
import { FppDevicePage } from "./FppDevicePage";
import { STATUS_POLL_MS } from "./useFppStatus";

describe("an FPP page's status reading", () => {
  afterEach(() => {
    vi.useRealTimers();
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  });

  it("reads about once a second while shown, stops while the window is hidden or the page is gone", async () => {
    const backend = new MemoryBackend(emptyShow("Home"));
    backend.deviceNetwork = demoDevices();
    backend.fppPlayers = demoPlayers();
    const reads = vi.spyOn(backend, "fppStatus");
    await useApp.getState().connect(backend);
    vi.useFakeTimers();
    const device = backend.deviceNetwork.details[0].device;
    const { unmount } = render(<FppDevicePage device={device} onBack={() => undefined} />);
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(reads).toHaveBeenCalledTimes(1);
    await act(() => vi.advanceTimersByTimeAsync(3 * STATUS_POLL_MS));
    expect(reads).toHaveBeenCalledTimes(4);

    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    await act(() => vi.advanceTimersByTimeAsync(10 * STATUS_POLL_MS));
    expect(reads).toHaveBeenCalledTimes(4);

    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(reads).toHaveBeenCalledTimes(5);

    unmount();
    await act(() => vi.advanceTimersByTimeAsync(10 * STATUS_POLL_MS));
    expect(reads).toHaveBeenCalledTimes(5);
    // Only the status endpoint is polled.
    expect(backend.calls.filter((c) => c.startsWith("fpp"))).toEqual([]);
  });
});
