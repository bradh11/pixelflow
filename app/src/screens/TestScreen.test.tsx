import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { currentSetupKey, useSetup } from "../state/setup";
import { useApp } from "../state/store";
import { TestScreen } from "./TestScreen";

async function setup() {
  const backend = new MemoryBackend(demoShow());
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<TestScreen />);
  return { backend, user };
}

describe("test screen", () => {
  it("Stop right after a change keeps the lights off (the pending change is dropped)", async () => {
    const { backend, user } = await setup();
    await user.click(screen.getByRole("button", { name: /^Start/ }));
    await screen.findByText(/Sending/);
    await user.click(screen.getByRole("button", { name: "Red" }));
    // Well within the 150 ms the change waits before reaching the lights.
    await user.click(screen.getByRole("button", { name: /^Stop/ }));
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(backend.output.running).toBe(false);
    expect(backend.calls.filter((c) => c === "startOutput")).toHaveLength(1);
    await waitFor(() => expect(screen.getByText("Output stopped")).toBeInTheDocument());
  });

  it("a change while running reaches the lights", async () => {
    const { backend, user } = await setup();
    await user.click(screen.getByRole("button", { name: /^Start/ }));
    await screen.findByText(/Sending/);
    await user.click(screen.getByRole("button", { name: "Blue" }));
    await waitFor(() => expect(backend.output.pattern?.color).toBe("0000ff"));
  });

  it("remembers that the show has been tested, for the setup checklist", async () => {
    const { user } = await setup();
    const key = currentSetupKey();
    expect(useSetup.getState().tested).not.toContain(key);
    await user.click(screen.getByRole("button", { name: /^Start/ }));
    await screen.findByText(/Sending/);
    expect(useSetup.getState().tested).toContain(key);
  });

  it("with no controllers, buttons go to find them or wire them", async () => {
    await useApp.getState().connect(new MemoryBackend({ ...demoShow(), controllers: [] }));
    const user = userEvent.setup();
    render(<TestScreen />);
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    expect(useApp.getState().screen).toBe("devices");
    await user.click(screen.getByRole("button", { name: "Go to Wiring" }));
    expect(useApp.getState().screen).toBe("wiring");
  });
});
