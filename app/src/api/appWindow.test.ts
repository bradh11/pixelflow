// What the window is asked itself (filling its display), and what the desktop app's shell is
// asked for the preview's own window: the commands and arguments the shell expects (see
// src-tauri/src/preview_window.rs).

import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: "preview" }) }));

import { appWindow as browserWindow } from "./appWindow";
import { isPreviewWindow, tauriBackend } from "./tauri";

type Internals = Window & { __TAURI_INTERNALS__?: unknown };

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  invoke.mockReset();
  delete (window as Internals).__TAURI_INTERNALS__;
  for (const key of ["requestFullscreen", "exitFullscreen", "fullscreenElement"] as const) {
    delete (document.documentElement as unknown as Record<string, unknown>)[key];
    delete (document as unknown as Record<string, unknown>)[key];
  }
});

describe("the window, in a browser", () => {
  it("fills the display with the page, and puts it back", async () => {
    const request = vi.fn(async () => {});
    const exit = vi.fn(async () => {});
    let full: Element | null = null;
    Object.defineProperty(document.documentElement, "requestFullscreen", { configurable: true, value: request });
    Object.defineProperty(document, "exitFullscreen", { configurable: true, value: exit });
    Object.defineProperty(document, "fullscreenElement", { configurable: true, get: () => full });
    await browserWindow.setFullscreen(true);
    expect(request).toHaveBeenCalledTimes(1);
    // Not full screen (the browser said no, or the user already left it): nothing to leave.
    await browserWindow.setFullscreen(false);
    expect(exit).not.toHaveBeenCalled();
    full = document.documentElement;
    await browserWindow.setFullscreen(false);
    expect(exit).toHaveBeenCalledTimes(1);
  });

  it("does nothing where the page can't go full screen, or the browser refuses", async () => {
    await expect(browserWindow.setFullscreen(true)).resolves.toBeUndefined();
    Object.defineProperty(document.documentElement, "requestFullscreen", { configurable: true, value: () => Promise.reject(new Error("not from a click")) });
    await expect(browserWindow.setFullscreen(true)).resolves.toBeUndefined();
  });

  it("says when the page starts or stops filling the display, however that came about", () => {
    let full: Element | null = null;
    Object.defineProperty(document, "fullscreenElement", { configurable: true, get: () => full });
    const seen: boolean[] = [];
    const stop = browserWindow.onFullscreenChange((on) => seen.push(on));
    full = document.documentElement;
    document.dispatchEvent(new Event("fullscreenchange"));
    full = null;
    document.dispatchEvent(new Event("fullscreenchange"));
    stop();
    document.dispatchEvent(new Event("fullscreenchange"));
    expect(seen).toEqual([true, false]);
  });
});

describe("the window, in the desktop app", () => {
  /** The window as the desktop app has it (the module picks which when it loads). */
  async function desktopWindow() {
    (window as Internals).__TAURI_INTERNALS__ = {};
    vi.resetModules();
    return (await import("./appWindow")).appWindow;
  }

  it("asks the shell to fill the display with this window", async () => {
    const desktop = await desktopWindow();
    invoke.mockResolvedValue(null);
    await desktop.setFullscreen(true);
    expect(invoke).toHaveBeenLastCalledWith("set_fullscreen", { on: true });
    await desktop.setFullscreen(false);
    expect(invoke).toHaveBeenLastCalledWith("set_fullscreen", { on: false });
  });

  it("asks whether it fills the display once a change of size has settled, and says when that changed", async () => {
    const desktop = await desktopWindow();
    vi.useFakeTimers();
    const seen: boolean[] = [];
    const stop = desktop.onFullscreenChange((on) => seen.push(on));
    invoke.mockResolvedValue(true);
    // The system animates into full screen: many sizes, one question.
    for (let i = 0; i < 5; i++) window.dispatchEvent(new Event("resize"));
    expect(invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(500);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenLastCalledWith("is_fullscreen");
    expect(seen).toEqual([true]);
    // Still full screen after another resize: nothing new to say.
    window.dispatchEvent(new Event("resize"));
    await vi.advanceTimersByTimeAsync(500);
    expect(seen).toEqual([true]);
    invoke.mockResolvedValue(false);
    window.dispatchEvent(new Event("resize"));
    await vi.advanceTimersByTimeAsync(500);
    expect(seen).toEqual([true, false]);
    stop();
    window.dispatchEvent(new Event("resize"));
    await vi.advanceTimersByTimeAsync(500);
    expect(invoke).toHaveBeenCalledTimes(3);
  });
});

describe("the preview's own window, in the desktop app", () => {
  it("is made and closed by the shell", async () => {
    invoke.mockResolvedValue(null);
    expect(await tauriBackend.openPreviewWindow()).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith("open_preview_window");
    await tauriBackend.closePreviewWindow();
    expect(invoke).toHaveBeenLastCalledWith("close_preview_window");
    invoke.mockRejectedValue("PixelFlow couldn't open the preview window: no display");
    await expect(tauriBackend.openPreviewWindow()).rejects.toBe("PixelFlow couldn't open the preview window: no display");
  });

  it("leaves and reads notes through the shell, which tells the windows apart itself", async () => {
    invoke.mockResolvedValue({ theirs: { toggles: 1 }, open: true });
    expect(await tauriBackend.previewLink("main", { source: "live" })).toEqual({ theirs: { toggles: 1 }, open: true });
    expect(invoke).toHaveBeenLastCalledWith("preview_link", { note: { source: "live" } });
    // Which window is asking isn't sent: the shell goes by the window.
    await tauriBackend.previewLink("preview");
    expect(invoke).toHaveBeenLastCalledWith("preview_link", {});
  });

  it("knows it's the preview's window by its label, whatever its address says", () => {
    expect(isPreviewWindow("")).toBe(false);
    (window as Internals).__TAURI_INTERNALS__ = {};
    expect(isPreviewWindow("")).toBe(true);
  });
});
