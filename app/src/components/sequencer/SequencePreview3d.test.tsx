import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { loadShowView, saveShowView, showViewKey, useView3d } from "../../state/view3d";
import type { Scene3d } from "../layout3d/scene";

// jsdom has no WebGL: the 3D view draws through a stand-in that records what it's given.
const fake = vi.hoisted(() => ({ made: 0, disposed: 0, pixels: [] as number[], colors: [] as { rgb: number[]; lit: boolean }[], orbits: [] as unknown[] }));

vi.mock("../layout3d/threeScene", () => ({
  createThreeScene: (): Scene3d => {
    fake.made++;
    return {
      resize: () => {},
      setPixels: (xyz) => fake.pixels.push(xyz.length / 3),
      updatePixels: () => {},
      setColors: (rgb, lit) => fake.colors.push({ rgb: Array.from(rgb), lit }),
      setBulbSize: () => {},
      setBackdrop: () => {},
      setModel: async () => null,
      placeModel: () => null,
      surfaceAt: () => null,
      setSelectionBox: () => {},
      setGizmo: () => {},
      setOptions: () => {},
      render: (orbit) => void fake.orbits.push(orbit),
      dispose: () => void fake.disposed++,
    };
  },
}));

const descriptors = {
  clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight"),
};

beforeEach(() => {
  Object.assign(fake, { made: 0, disposed: 0, pixels: [], colors: [], orbits: [] });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 600 });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 300 });
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, width: 1000, height: 600, right: 1000, bottom: 600, toJSON: () => ({}) });
});

afterEach(() => {
  for (const [key, d] of Object.entries(descriptors)) if (d) Object.defineProperty(HTMLElement.prototype, key, d);
  vi.restoreAllMocks();
});

async function openScreen() {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000, { singing: true }));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  const pixels = (await backend.previewProps3d()).props.reduce((n, p) => n + p.xyz.length / 3, 0);
  return { show, seq, backend, user, pixels };
}

const preview = () => screen.getByRole("region", { name: "Preview" });

describe("the Sequence screen's 3D preview", () => {
  it("switches to a look-only 3D view lit by the frame at the playhead, remembers it, and lets go of it when switched back", async () => {
    const { user, pixels } = await openScreen();
    await user.click(within(preview()).getByRole("button", { name: "3D" }));
    expect(within(preview()).getByRole("application", { name: "3D preview" })).toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.sequenceMode")).toBe("3d");
    // Every pixel in one batch, colored from the frame the engine rendered for the playhead.
    await waitFor(() => expect(fake.pixels.at(-1)).toBe(pixels));
    await waitFor(() => expect(fake.colors.at(-1)?.lit).toBe(true));
    const atStart = fake.colors.at(-1)!.rgb;
    act(() => useSequencer.getState().setPlayhead(10_000));
    await waitFor(() => expect(fake.colors.at(-1)!.rgb).not.toEqual(atStart));
    expect(fake.made).toBe(1);

    await user.click(within(preview()).getByRole("button", { name: "2D" }));
    expect(within(preview()).getByRole("img", { name: "Preview of the show at the playhead" })).toBeInTheDocument();
    expect(fake.disposed, "the 3D view's renderer is let go").toBe(1);
    expect(localStorage.getItem("pixelflow.sequenceMode")).toBe("2d");
  });

  it("follows the playing sequence's live frames", async () => {
    const { user, backend } = await openScreen();
    await user.click(within(preview()).getByRole("button", { name: "3D" }));
    await waitFor(() => expect(fake.colors.at(-1)?.lit).toBe(true));
    const live = new Uint8Array(3 * 2000).fill(7);
    vi.spyOn(backend, "liveFrame").mockResolvedValue(live);
    await act(() => useSequencer.getState().play());
    await waitFor(() => expect(fake.colors.at(-1)?.rgb.slice(0, 3)).toEqual([7, 7, 7]), { timeout: 2000 });
    await act(() => useSequencer.getState().stop());
  });

  it("shows just the selected row's pixels: a whole prop, or only a submodel's", async () => {
    const { show, seq, user } = await openScreen();
    await user.click(within(preview()).getByRole("button", { name: "3D" }));
    const arch = show.props.find((p) => p.name === "Garage Arch")!;
    const ends = arch.regions.find((r) => r.name === "Ends")!;
    const archRow = seq.doc!.rows.find((r) => "prop" in r.target && r.target.prop === arch.id)!;
    const endsRow = seq.doc!.rows.find((r) => "region" in r.target && r.target.region.region === ends.id)!;
    act(() => useSequencer.getState().setActiveRow(archRow.id));
    await user.click(within(preview()).getByRole("checkbox", { name: "Selected row only" }));
    const archPixels = (await useApp.getState().backend!.previewProps3d()).props.find((p) => p.prop === arch.id)!.xyz.length / 3;
    await waitFor(() => expect(fake.pixels.at(-1)).toBe(archPixels));
    // The arch's Ends: six pixels at each end.
    act(() => useSequencer.getState().setActiveRow(endsRow.id));
    await waitFor(() => expect(fake.pixels.at(-1)).toBe(12));
    // Still lit from the frame, now only those pixels.
    await waitFor(() => expect(fake.colors.at(-1)?.rgb).toHaveLength(36));
    expect(fake.made, "the same renderer throughout").toBe(1);
  });

  it("shares the show's remembered camera with the Layout and Play screens", async () => {
    const show = demoShow();
    const orbit = { target: { x: 1, y: 2, z: 0 }, yaw: 0.3, pitch: 0.1, distance: 20 };
    saveShowView(showViewKey(null, show.name), { orbit });
    const { user } = await openScreen();
    await user.click(within(preview()).getByRole("button", { name: "3D" }));
    await waitFor(() => expect(fake.orbits.length).toBeGreaterThan(0));
    expect(fake.orbits.at(-1)).toEqual(orbit);
    expect(loadShowView(showViewKey(null, show.name)).orbit).toEqual(orbit);
  });

  it("opens in 3D when that was the last choice, leaves Space to play and pause, and lets go of the renderer when the screen is left", async () => {
    useView3d.getState().setSequenceMode("3d");
    const { user } = await openScreen();
    expect(within(preview()).getByRole("application", { name: "3D preview" })).toBeInTheDocument();
    await waitFor(() => expect(fake.made).toBe(1));
    fireEvent.keyDown(document.body, { key: " " });
    await waitFor(() => expect(useSequencer.getState().status?.state).toBe("playing"));
    await act(() => useSequencer.getState().stop());
    await user.click(screen.getByRole("button", { name: "Layout" }));
    expect(fake.disposed).toBe(1);
  });

  it("can be made bigger, and resized by dragging the divider, remembering the size", async () => {
    const { user } = await openScreen();
    const pane = () => preview().parentElement!;
    const divider = screen.getByRole("separator", { name: "Preview size" });
    // Dragging the divider down 100 px from the default size (34% of the 600 px column).
    expect(pane().style.height).toBe("34%");
    fireEvent.pointerDown(divider, { clientY: 200, pointerId: 1, button: 0 });
    fireEvent.pointerMove(divider, { clientY: 300, pointerId: 1 });
    fireEvent.pointerUp(divider, { clientY: 300, pointerId: 1 });
    expect(pane().style.height).toBe("304px");
    expect(JSON.parse(localStorage.getItem("pixelflow.sequencePreview")!)).toMatchObject({ height: 304 });
    // The keyboard works too; it never squeezes the timeline out.
    divider.focus();
    await user.keyboard("{ArrowDown}");
    expect(pane().style.height).toBe("324px");
    fireEvent.pointerDown(divider, { clientY: 324, pointerId: 1, button: 0 });
    fireEvent.pointerMove(divider, { clientY: 900, pointerId: 1 });
    fireEvent.pointerUp(divider, { clientY: 900, pointerId: 1 });
    expect(pane().style.height).toBe("360px");

    const bigger = within(preview()).getByRole("button", { name: "Bigger preview" });
    await user.click(bigger);
    expect(bigger).toHaveAttribute("aria-pressed", "true");
    expect(pane().style.height).toBe("75%");
    await user.click(bigger);
    expect(pane().style.height).toBe("360px");
  });

  it("still opens when this computer won't store the preview's size", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const { user } = await openScreen();
    await user.click(within(preview()).getByRole("button", { name: "Bigger preview" }));
    expect(preview().parentElement!.style.height).toBe("75%");
  });
});
