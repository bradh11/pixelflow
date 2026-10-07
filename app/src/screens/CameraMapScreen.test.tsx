import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";
import { CameraMapScreen, toggleAnchor } from "./CameraMapScreen";

async function setup() {
  const backend = new MemoryBackend(demoShow());
  backend.cameraMapSamples = true;
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<CameraMapScreen />);
  return { backend, user };
}

describe("camera mapping screen", () => {
  it("flashes the code on the chosen target from Start", async () => {
    const { backend, user } = await setup();
    const arch = backend.show.props[0];
    await user.selectOptions(screen.getByRole("combobox", { name: "Target" }), `prop:${arch.id}`);
    await screen.findByText(/50 pixels on Garage Arch/);
    await user.click(screen.getByRole("button", { name: /Start flashing/ }));
    expect(backend.output.pattern?.kind).toBe("cameraMap");
    expect(backend.lastTarget).toEqual({ type: "prop", id: arch.id });
    await user.selectOptions(screen.getByRole("combobox", { name: "Code" }), "two");
    await user.click(screen.getByRole("button", { name: /Start (flashing|again)/ }));
    expect(backend.output.pattern?.kind).toBe("cameraMapBinary");
  });

  it("reads a capture, says what's worth checking, and places the props as one undo step", async () => {
    const { backend, user } = await setup();
    await screen.findByText(/pixels on 4 props/);
    await user.click(screen.getByRole("button", { name: /Use a sample video/ }));
    await screen.findByText(/4 · Check and place/, undefined, { timeout: 20_000 });
    const checking = await screen.findByRole("region", { name: "Worth checking" });
    expect(within(checking).getByText(/Garage Arch: pixel 11 never lit up/)).toBeInTheDocument();
    expect(within(checking).getByText(/Garage Arch: pixels 1–3 seen twice/)).toBeInTheDocument();
    // The sample capture swaps the second prop's red and green.
    expect(within(checking).getByText(/Mega Tree: colours came out wrong.*looks like GRB, not RGB/)).toBeInTheDocument();

    const before = backend.undoStack.length;
    await user.click(screen.getByRole("button", { name: /Place 4 props/ }));
    await waitFor(() => expect(backend.undoStack.length).toBe(before + 1));
    // The arch matches its shape, so it keeps it; the star doesn't, so it gets the measured one.
    expect(backend.show.props.find((p) => p.name === "Garage Arch")?.shape.source).toBe("generator");
    expect(backend.show.props.find((p) => p.name === "Porch Star")?.shape).toMatchObject({ source: "measured", provenance: "cameraMap" });

    await user.click(within(checking).getByRole("button", { name: "Use GRB" }));
    await waitFor(() => expect(backend.show.props.find((p) => p.name === "Mega Tree")?.colorOrder).toBe("GRB"));
  }, 30_000);

  it("keeps up to three anchors, the newest replacing the oldest", () => {
    expect(toggleAnchor([], 4)).toEqual([4]);
    expect(toggleAnchor([4, 7], 4)).toEqual([7]);
    expect(toggleAnchor([1, 2, 3], 9)).toEqual([2, 3, 9]);
  });
});
