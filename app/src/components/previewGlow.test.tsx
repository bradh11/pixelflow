// Every 2D preview draws its pixels with the viewer's glow level (state/view3d.ts): none unless
// it's turned up. What a level looks like (crisp dots at none, halos above) is pixelGlow.test.ts's.

import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import type { ProposalView } from "../api/assistant";
import { demoShow } from "../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend, emptyShow } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import type { PreviewProp } from "../api/types";
import { drawPixels } from "../lib/pixelBatches";
import { newProp } from "../lib/shows";
import { LayoutScreen } from "../screens/LayoutScreen";
import { useAssistant } from "../state/assistant";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { useView3d } from "../state/view3d";
import { DraftPreview } from "./assistant/DraftPreview";
import { LivePreview } from "./layout3d/LivePreview";

vi.mock("../lib/pixelBatches", async (original) => ({ ...(await original<typeof import("../lib/pixelBatches")>()), drawPixels: vi.fn() }));

/** The frame and glow level each call to draw a preview's pixels was given, latest last. */
const draws = () => vi.mocked(drawPixels).mock.calls.map((call) => ({ frame: call[2], glow: call[9] }));
const lastDraw = () => draws().at(-1);

/** A canvas context that takes any drawing and does nothing with it. */
const blank: unknown = new Proxy(function () {}, { get: (_, key) => (key === Symbol.toPrimitive ? () => 0 : blank), apply: () => blank, set: () => true });

const getContext = HTMLCanvasElement.prototype.getContext;
const descriptors = {
  clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight"),
};

beforeEach(() => {
  vi.mocked(drawPixels).mockClear();
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 600 });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 300 });
  // Only the previews' own canvases draw: the rest of each screen stays as in every other test.
  HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement) {
    return /^(Preview|Layout canvas)/.test(this.getAttribute("aria-label") ?? "") ? blank : null;
  } as typeof HTMLCanvasElement.prototype.getContext;
});

afterEach(() => {
  HTMLCanvasElement.prototype.getContext = getContext;
  for (const [key, d] of Object.entries(descriptors)) if (d) Object.defineProperty(HTMLElement.prototype, key, d);
});

/** Slides the Glow control inside `scope` to `percent`. */
function slideGlow(scope: HTMLElement, percent: number) {
  const button = within(scope).getByRole("button", { name: "Glow" });
  if (button.getAttribute("aria-expanded") !== "true") fireEvent.click(button);
  fireEvent.change(within(scope).getByRole("slider", { name: "Glow" }), { target: { value: String(percent) } });
}

const LIT = new Uint8Array([255, 0, 0, 0, 255, 0]);
const props: PreviewProp[] = [{ prop: "p1", frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 1, 0] }];

describe("the previews' glow", () => {
  it("is none on the Play screen's preview until it's turned up there", async () => {
    await useApp.getState().connect(new MemoryBackend(emptyShow("Home")));
    const { container } = render(<LivePreview props={props} frame={LIT} />);
    expect(lastDraw()).toEqual({ frame: LIT, glow: 0 });
    slideGlow(container, 50);
    expect(lastDraw()).toEqual({ frame: LIT, glow: 0.5 });
    expect(useView3d.getState().glow).toBe(0.5);
    slideGlow(container, 0);
    expect(lastDraw()).toEqual({ frame: LIT, glow: 0 });
  });

  it("is on a draft sequence the assistant plays, but not on its picture of what a draft changes", async () => {
    act(() => useView3d.getState().setGlow(0.5));
    const proposal = { id: "d1", summary: "A draft", diff: { changes: [] }, changedProps: ["p1"], changesShow: true, changesSequence: false, sections: [], timeline: null, lockedEdges: 0, cues: null, review: null };
    const api = { previewFrame: async () => LIT } as unknown as NonNullable<ReturnType<typeof useAssistant.getState>["api"]>;
    useAssistant.setState({ api, preview: { revision: 1, props }, proposal: proposal as ProposalView });
    render(<DraftPreview />);
    expect(lastDraw()?.glow).toBe(0);

    const timeline = { durationMs: 1000 } as unknown as NonNullable<ProposalView["timeline"]>;
    act(() => useAssistant.setState({ proposal: { ...proposal, changesSequence: true, timeline } as ProposalView }));
    await waitFor(() => expect(lastDraw()).toEqual({ frame: LIT, glow: 0.5 }));
  });

  it("is on the Sequence screen's preview, from the control beside its 2D | 3D switch", async () => {
    const show = demoShow();
    const backend = new MemoryBackend(show);
    const seq = new MemorySequencer(backend);
    seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000, { singing: true }));
    await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
    await useApp.getState().connect(backend);
    useApp.setState({ started: true, screen: "sequence" });
    await useSequencer.getState().connect(seq);
    render(<App />);
    const preview = await screen.findByRole("region", { name: "Preview" });
    // The frame at the playhead, with no glow.
    await waitFor(() => expect(lastDraw()?.frame).toBeInstanceOf(Uint8Array));
    expect(lastDraw()?.glow).toBe(0);
    slideGlow(preview, 75);
    await waitFor(() => expect(lastDraw()?.glow).toBe(0.75));
    expect(lastDraw()?.frame).toBeInstanceOf(Uint8Array);
  });

  it("is on the Layout canvas's lit pixels (a test pattern, say), from the tool bar", async () => {
    const show = emptyShow("Home");
    show.props = [newProp("line", show)];
    const backend = new MemoryBackend(show);
    backend.liveFrame = async () => LIT;
    await useApp.getState().connect(backend);
    useApp.setState({ started: true });
    render(<LayoutScreen />);
    await waitFor(() => expect(lastDraw()?.frame).toBe(LIT));
    expect(lastDraw()?.glow).toBe(0);
    slideGlow(screen.getByRole("toolbar", { name: "Layout tools" }), 30);
    await waitFor(() => expect(lastDraw()).toEqual({ frame: LIT, glow: 0.3 }));
  });
});
