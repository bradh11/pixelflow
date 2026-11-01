import { describe, expect, it } from "vitest";
import { MemoryBackend } from "../api/memory";
import { VIEW_DEFAULTS } from "../lib/viewOptions";
import { readFeed, readReply } from "./previewWindow";

const VIEW = { grid: false, names: true, unlit: "auto", dotSize: "small", boost: 2 };
const FEED = {
  source: "sequence",
  playheadMs: 1200,
  durationMs: 60_000,
  stamp: "1:4:0",
  only: { prop: "p1" },
  showRevision: 3,
  title: "Medley",
  transport: true,
  mode: "3d",
  glow: 0.5,
  view: VIEW,
  syncMs: -20,
};

describe("the notes between the main window and the preview's own", () => {
  it("are read back as what they say", () => {
    expect(readFeed(FEED)).toEqual(FEED);
    expect(readFeed({ ...FEED, source: "live", only: null, title: null, transport: false })).toMatchObject({ source: "live", only: null, title: null, transport: false });
    expect(readReply({ mode: "2d", glow: 0, view: VIEW, toggles: 4 })).toEqual({ mode: "2d", glow: 0, view: VIEW, toggles: 4 });
  });

  it("read the View options as the usual ones when a note has none, or ones not understood", () => {
    expect(readFeed({ ...FEED, view: undefined })?.view).toEqual(VIEW_DEFAULTS);
    expect(readReply({ mode: "2d", glow: 0, toggles: 0, view: { boost: 99 } })?.view).toEqual(VIEW_DEFAULTS);
  });

  it("are passed over when they aren't notes (nothing left yet, or something else's)", () => {
    for (const not of [null, undefined, "live", 3, [], {}, { ...FEED, source: "elsewhere" }, { ...FEED, mode: "4d" }, { ...FEED, playheadMs: "12" }, { ...FEED, glow: Number.NaN }, { ...FEED, stamp: 4 }]) {
      expect(readFeed(not), JSON.stringify(not)).toBeNull();
    }
    for (const not of [null, {}, { mode: "2d", glow: 0 }, { mode: "2d", glow: 0, toggles: "1" }, { mode: "side", glow: 0, toggles: 0 }]) {
      expect(readReply(not), JSON.stringify(not)).toBeNull();
    }
    // A row to show alone is a target or nothing.
    expect(readFeed({ ...FEED, only: "row-1" })?.only).toBeNull();
  });
});

describe("the in-memory engine's link between the two windows", () => {
  it("keeps each window's note for the other, like the shell", async () => {
    const backend = new MemoryBackend();
    // Left before the window opens, the main window's note waits for it.
    expect(await backend.previewLink("main", { source: "live" })).toEqual({ theirs: null, open: false });
    // Nothing is taken from a preview window that isn't open.
    await backend.previewLink("preview", { toggles: 9 });
    expect(await backend.openPreviewWindow()).toBe(true);
    expect(await backend.previewLink("preview")).toEqual({ theirs: { source: "live" }, open: true });
    expect((await backend.previewLink("main")).theirs).toBeNull();
    await backend.previewLink("preview", { toggles: 1 });
    expect(await backend.previewLink("main")).toEqual({ theirs: { toggles: 1 }, open: true });
    // Asked for again, it's the same window.
    const first = backend.previewWindow;
    await backend.openPreviewWindow();
    expect(backend.previewWindow).toBe(first);

    await backend.closePreviewWindow();
    expect(await backend.previewLink("main")).toEqual({ theirs: null, open: false });
    expect(backend.calls.filter((c) => c.includes("PreviewWindow"))).toEqual(["openPreviewWindow", "openPreviewWindow", "closePreviewWindow"]);
  });

  it("opens a browser window when the app runs in one, and says when the browser won't", async () => {
    const backend = new MemoryBackend();
    const popup = { closed: false, close: () => void (popup.closed = true), focus: () => void focused++ };
    let focused = 0;
    backend.openWindow = () => popup;
    expect(await backend.openPreviewWindow()).toBe(true);
    expect((await backend.previewLink("main")).open).toBe(true);
    // Open already: brought to the front, not opened twice.
    await backend.openPreviewWindow();
    expect(focused).toBe(1);
    // Closed with its own close button.
    popup.closed = true;
    expect((await backend.previewLink("main")).open).toBe(false);

    backend.openWindow = () => null;
    expect(await backend.openPreviewWindow()).toBe(false);
    expect((await backend.previewLink("main")).open).toBe(false);
  });
});
