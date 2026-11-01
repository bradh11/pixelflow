import { act, render, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { describe, expect, it, vi } from "vitest";
import { emptyShow } from "../../api/memory";
import { type FloorGrid, type LightLook, SPREAD_SIZE, floorGrid, packSpread, v3 } from "../../lib/layout3d";
import { pixelSpread3d } from "../../lib/pixelBatches";
import { LAYOUT_LOOK, PREVIEW_LOOK, lightLook } from "../../lib/pixelLooks";
import { VIEW_DEFAULTS, boostFrame } from "../../lib/viewOptions";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useView3d } from "../../state/view3d";
import { Layout3dView } from "./Layout3dView";
import type { Scene3d, SceneFactory } from "./scene";

const stub: Scene3d = {
  resize: () => {},
  setPixels: () => {},
  updatePixels: () => {},
  setFloods: () => {},
  setMotion: () => {},
  setColors: () => {},
  setLook: () => {},
  setBackdrop: () => {},
  setModel: async () => null,
  placeModel: () => null,
  surfaceAt: () => null,
  setSelectionBox: () => {},
  setGizmo: () => {},
  setOptions: () => {},
  setFloor: () => {},
  render: () => {},
  dispose: () => {},
};

describe("the 3D view's renderer", () => {
  it("is made once per canvas even when React mounts the view twice (as it does in development), and let go of once the view is gone", async () => {
    // A real renderer on a canvas takes the canvas's one WebGL context: a second one made on the
    // same canvas shares it, and throwing the first away would lose the context for both.
    const made: { canvas: HTMLCanvasElement; disposed: number }[] = [];
    const factory: SceneFactory = async (canvas) => {
      await Promise.resolve();
      const record = { canvas, disposed: 0 };
      made.push(record);
      return { ...stub, dispose: () => void record.disposed++ };
    };
    const view = (
      <StrictMode>
        <Layout3dView preview={{ revision: 0, props: [] }} show={emptyShow("Home")} photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }} storageKey="k" frame={null} sceneFactory={factory} />
      </StrictMode>
    );
    const { unmount } = render(view);
    await waitFor(() => expect(made).toHaveLength(1));
    await act(async () => {});
    expect(made).toHaveLength(1);
    expect(made[0].disposed, "still in use").toBe(0);
    unmount();
    await waitFor(() => expect(made[0].disposed).toBe(1));
  });

  it("is told how much the viewer has the lights glow, and again whenever that changes", async () => {
    const options: Parameters<Scene3d["setOptions"]>[0][] = [];
    useView3d.getState().setGlow(0.35);
    render(
      <Layout3dView
        preview={{ revision: 0, props: [] }}
        show={emptyShow("Home")}
        photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }}
        storageKey="k"
        frame={null}
        sceneFactory={async () => ({ ...stub, setOptions: (o) => void options.push(o) })}
      />,
    );
    await waitFor(() => expect(options).toEqual([{ glow: 0.35, ground: true, grid: true, dotScale: 1 }]));
    act(() => useView3d.getState().setGlow(0));
    expect(options.at(-1)).toEqual({ glow: 0, ground: true, grid: true, dotScale: 1 });
    act(() => useView3d.getState().setGround(false));
    expect(options.at(-1)).toEqual({ glow: 0, ground: false, grid: true, dotScale: 1 });
    expect(options).toHaveLength(3);
    // The grid and the dot size, too.
    act(() => useView3d.getState().setViewOptions({ grid: false }));
    expect(options.at(-1)).toEqual({ glow: 0, ground: false, grid: false, dotScale: 1 });
    act(() => useView3d.getState().setViewOptions({ dotSize: "large" }));
    expect(options.at(-1)).toEqual({ glow: 0, ground: false, grid: false, dotScale: 1.5 });
    act(() => useView3d.getState().setViewOptions({ dotSize: "small" }));
    expect(options.at(-1)).toMatchObject({ dotScale: 0.6 });
    // Names only redraw.
    const count = options.length;
    act(() => useView3d.getState().setViewOptions({ names: true }));
    expect(options).toHaveLength(count);
    act(() => {
      useView3d.getState().setGround(true);
      useView3d.getState().setViewOptions(VIEW_DEFAULTS);
    });
  });

  it("measures the grid on the ground from the display, and says how far apart its lines are", async () => {
    const floors: FloorGrid[] = [];
    const preview = { revision: 1, props: [{ prop: "wide", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([-12, 0, 0, 12, 4, 0]) }] };
    const { unmount } = render(
      <Layout3dView
        preview={preview}
        show={emptyShow("Home")}
        photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }}
        storageKey="k"
        frame={null}
        sceneFactory={async () => ({ ...stub, setFloor: (f) => void floors.push(f) })}
      />,
    );
    await waitFor(() => expect(floors.at(-1)).toEqual(floorGrid({ min: v3(-12, 0, 0), max: v3(12, 4, 0) })));
    expect(floors.at(-1)).toMatchObject({ minor: 1, major: 5 });
    expect(useView3d.getState().floor).toMatchObject({ minor: 1, major: 5 });
    unmount();
    // With no 3D view open, there's no grid to speak of.
    expect(useView3d.getState().floor).toBeNull();
  });

  it("leaves pixels that are off out when the viewer hides them, and with Auto only while the show plays", async () => {
    const looks: LightLook[] = [];
    const factory: SceneFactory = async () => ({ ...stub, setLook: (look) => void looks.push(look) });
    const photo = { image: null, aspect: 0.75, problem: null, reload: () => {} };
    const view = (playing: boolean) => (
      <Layout3dView preview={{ revision: 0, props: [] }} show={emptyShow("Home")} photo={photo} storageKey="k" frame={null} playing={playing} sceneFactory={factory} />
    );
    const shown = lightLook(PREVIEW_LOOK).look;
    const { rerender } = render(view(false));
    await waitFor(() => expect(looks).toEqual([shown]));
    act(() => useView3d.getState().setViewOptions({ unlit: "hidden" }));
    expect(looks.at(-1)).toEqual({ ...shown, dark: [0, 0, 0, 0] });
    // Nothing playing still shows every prop in its quiet color.
    expect(looks.at(-1)!.unlit).toEqual(shown.unlit);
    act(() => useView3d.getState().setViewOptions({ unlit: "auto" }));
    expect(looks.at(-1)).toEqual(shown);
    rerender(view(true));
    await waitFor(() => expect(looks.at(-1)).toEqual({ ...shown, dark: [0, 0, 0, 0] }));
    rerender(view(false));
    await waitFor(() => expect(looks.at(-1)).toEqual(shown));
    // Told only when it changes.
    const count = looks.length;
    rerender(view(false));
    await act(async () => {});
    expect(looks).toHaveLength(count);
    act(() => useView3d.getState().setViewOptions({ unlit: "dim" }));
  });

  it("shows dim levels lifted when the viewer boosts them, and the colors as they are otherwise", async () => {
    const colors: number[][] = [];
    const factory: SceneFactory = async () => ({ ...stub, setColors: (rgb) => void colors.push(Array.from(rgb)) });
    const preview = { revision: 1, props: [{ prop: "one", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([0, 0, 0]) }] };
    const frame = new Uint8Array([20, 10, 0]);
    render(<Layout3dView preview={preview} show={emptyShow("Home")} photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }} storageKey="k" frame={frame} sceneFactory={factory} />);
    await waitFor(() => expect(colors.at(-1)).toEqual([20, 10, 0]));
    act(() => useView3d.getState().setViewOptions({ boost: 2 }));
    expect(colors.at(-1)).toEqual(Array.from(boostFrame(frame, preview.props, 2)));
    expect(colors.at(-1)![0]).toBeGreaterThan(20);
    // The frame itself is left as it was.
    expect(Array.from(frame)).toEqual([20, 10, 0]);
    act(() => useView3d.getState().setViewOptions({ boost: 0 }));
    expect(colors.at(-1)).toEqual([20, 10, 0]);
  });

  it("is handed every pixel's place with how its prop is spread out, and the look of the screen it's on", async () => {
    const preview = {
      revision: 1,
      props: [
        { prop: "string", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([0, 0, 0, 0.5, 0, 0, 1, 0, 0]) },
        { prop: "one", frameOffset: 9, channelsPerPixel: 3, xyz: new Float32Array([4, 4, -2]) },
      ],
    };
    const seen = { pixels: [] as { xyz: number[]; spread: number[] }[], looks: [] as LightLook[], colors: [] as number[][] };
    const factory: SceneFactory = async () => ({
      ...stub,
      setPixels: (xyz, spread) => void seen.pixels.push({ xyz: Array.from(xyz), spread: Array.from(spread) }),
      setLook: (look) => void seen.looks.push(look),
      setColors: (rgb) => void seen.colors.push(Array.from(rgb)),
    });
    const photo = { image: null, aspect: 0.75, problem: null, reload: () => {} };
    const looking = render(<Layout3dView preview={preview} show={emptyShow("Home")} photo={photo} storageKey="k" frame={null} sceneFactory={factory} />);
    await waitFor(() => expect(seen.pixels).toHaveLength(1));
    expect(seen.pixels[0].xyz).toEqual([0, 0, 0, 0.5, 0, 0, 1, 0, 0, 4, 4, -2]);
    expect(seen.pixels[0].spread).toEqual(Array.from(packSpread(preview.props)));
    // The string's pixels half a unit apart, the lone pixel with no neighbors: each sized by its own prop.
    expect(seen.pixels[0].spread.filter((_, i) => i % SPREAD_SIZE === 0)).toEqual([0.5, 0.5, 0.5, 1e6]);
    // A preview: the flat previews' quiet colors and dot sizes.
    expect(seen.looks).toEqual([lightLook(PREVIEW_LOOK).look]);
    // Nothing playing and nothing selected: no pixel has a color of its own.
    expect(seen.colors.at(-1)).toEqual(new Array(12).fill(0));
    looking.unmount();

    // On the Layout screen: its look, and the selected prop's pixels in the selection color.
    seen.looks.length = 0;
    useLayoutEditor.getState().select(["one"]);
    render(<Layout3dView preview={preview} show={emptyShow("Home")} photo={photo} storageKey="k" editable sceneFactory={factory} />);
    await waitFor(() => expect(seen.looks).toEqual([lightLook(LAYOUT_LOOK).look]));
    await waitFor(() => expect(seen.colors.at(-1)).toEqual([...new Array(9).fill(0), ...lightLook(LAYOUT_LOOK).selected]));
    useLayoutEditor.getState().select([]);
  });

  it("measures again only the props that changed when the layout does", async () => {
    // A layout as the backend sends it: every prop's pixels in a new array each time.
    const layout = (revision: number, step: number) => ({
      revision,
      props: [
        { prop: "string", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([0, 0, 0, step, 0, 0, 2 * step, 0, 0]) },
        { prop: "rows", frameOffset: 9, channelsPerPixel: 3, xyz: new Float32Array(Array.from({ length: 36 }, (_, i) => [(i % 6) * 0.25, Math.floor(i / 6) * 0.25, -1]).flat()) },
      ],
    });
    const [first, second] = [layout(1, 0.5), layout(2, 0.75)];
    const spreads: number[][] = [];
    const factory: SceneFactory = async () => ({ ...stub, setPixels: (_, spread) => void spreads.push(Array.from(spread)) });
    const view = (preview: typeof first) => (
      <Layout3dView preview={preview} show={emptyShow("Home")} photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }} storageKey="k" frame={null} sceneFactory={factory} />
    );
    const { rerender } = render(view(first));
    await waitFor(() => expect(spreads).toHaveLength(1));
    rerender(view(second));
    await waitFor(() => expect(spreads).toHaveLength(2));
    // The stretched string's new spacing; the rows, which are where they were, as they were measured.
    expect([spreads[0][0], spreads[1][0]]).toEqual([0.5, 0.75]);
    expect(spreads[1][3 * SPREAD_SIZE]).toBe(0.25);
    expect(pixelSpread3d(second.props[1].xyz)).toBe(pixelSpread3d(first.props[1].xyz));
  });

  it("tells the renderer while the camera is on its way somewhere, and when it has arrived", async () => {
    const frames: boolean[] = [];
    const raf = vi.spyOn(window, "requestAnimationFrame").mockImplementation((step) => setTimeout(() => step(performance.now()), 0) as unknown as number);
    const view = render(
      <Layout3dView
        preview={{ revision: 1, props: [{ prop: "p", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array([0, 0, 0, 1, 2, 0]) }] }}
        show={emptyShow("Home")}
        photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }}
        storageKey="moving"
        frame={null}
        sceneFactory={async () => ({ ...stub, render: (_, moving) => void frames.push(!!moving) })}
      />,
    );
    const canvas = view.container.querySelector("canvas")!;
    Object.defineProperties(canvas, { clientWidth: { value: 800 }, clientHeight: { value: 500 } });
    act(() => void window.dispatchEvent(new Event("resize")));
    // Placed where it starts, it's still.
    await waitFor(() => expect(frames.length).toBeGreaterThan(0));
    expect(frames.at(-1)).toBe(false);
    // Asked for another view, it glides there: moving until the last frame, which is still.
    frames.length = 0;
    act(() => useView3d.getState().camera({ kind: "preset", preset: "top" }));
    await waitFor(() => expect(frames.at(-1)).toBe(false), { timeout: 3000 });
    expect(frames.length).toBeGreaterThan(2);
    expect(frames.slice(0, -1).every((moving) => moving)).toBe(true);
    raf.mockRestore();
  });

  it("says so plainly when the graphics card drops the 3D view", async () => {
    const { container, findByText } = render(
      <Layout3dView preview={{ revision: 0, props: [] }} show={emptyShow("Home")} photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }} storageKey="k" frame={null} sceneFactory={async () => stub} />,
    );
    const canvas = container.querySelector("canvas")!;
    act(() => void canvas.dispatchEvent(new Event("webglcontextlost")));
    expect(await findByText("The 3D view stopped (the graphics card let go of it). Switch to 2D and back to start it again.")).toBeInTheDocument();
  });

  it("is kept when the view's code is hot-reloaded while it's shown (development)", async () => {
    // A hot reload keeps the canvas: the old code lets go of the renderer and the new code takes it
    // straight away. Two copies of the module stand in for before and after.
    const canvas = document.createElement("canvas");
    let [made, disposed] = [0, 0];
    const factory: SceneFactory = async () => {
      made++;
      return { ...stub, dispose: () => void disposed++ };
    };
    const before = await import("./Layout3dView");
    vi.resetModules();
    const after = await import("./Layout3dView");
    expect(after).not.toBe(before);
    await before.takeRenderer(canvas, factory);
    before.releaseRenderer(canvas);
    await after.takeRenderer(canvas, factory);
    await act(async () => {});
    expect([made, disposed]).toEqual([1, 0]);
    after.releaseRenderer(canvas);
    await waitFor(() => expect(disposed).toBe(1));
  });
});
