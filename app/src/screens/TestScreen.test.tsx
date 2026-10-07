import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { currentSetupKey, useSetup } from "../state/setup";
import { useApp } from "../state/store";
import { TestScreen } from "./TestScreen";

async function setup(show = demoShow(), ready?: (backend: MemoryBackend) => void) {
  const backend = new MemoryBackend(show);
  ready?.(backend);
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

  it("says which controllers the target sends to, and whether each answers", async () => {
    const { backend, user } = await setup(demoShow(), (b) => b.answering.add("192.168.1.50"));
    const list = screen.getByRole("region", { name: "Sends to" });
    // The whole show: only the FPP has props wired to it.
    const fpp = await within(list).findByText("Answering");
    expect(fpp.closest("li")).toHaveTextContent(/Main FPP192\.168\.1\.50sACN · universes 1–\d+ · ch 1–/);
    expect(within(list).queryByText("Porch WLED")).not.toBeInTheDocument();

    // Check again looks again (and says so meanwhile).
    const before = backend.calls.filter((c) => c === "checkControllers").length;
    backend.answering.clear();
    await user.click(screen.getByRole("button", { name: /Check again/ }));
    expect(await within(list).findByText(/^Not answering — check it's powered on and on the same network as this computer$/)).toBeInTheDocument();
    expect(backend.calls.filter((c) => c === "checkControllers").length).toBe(before + 1);

    // A single prop: only its controller, looked at again.
    await user.selectOptions(screen.getByRole("combobox", { name: "Target" }), "Prop: Mega Tree");
    await waitFor(() => expect(backend.calls.filter((c) => c === "checkControllers").length).toBe(before + 2));
    expect(within(list).getAllByRole("listitem")).toHaveLength(1);
  });

  it("with nothing wired on the target, says so, points to Wiring, and keeps Start off", async () => {
    const { user } = await setup();
    await user.selectOptions(screen.getByRole("combobox", { name: "Target" }), "Prop: Porch Star");
    expect(screen.getByText(/Nothing on this target is wired to a controller yet/)).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Sends to" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Start/ })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Go to Wiring" }));
    expect(useApp.getState().screen).toBe("wiring");
  });

  it("while running, says what is sent where", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("button", { name: /^Start/ }));
    expect(await screen.findByText("Sending Chase, white → Main FPP (192.168.1.50)")).toBeInTheDocument();
  });

  it("notes when the show's controllers aren't on this computer's network", async () => {
    await setup();
    // The demo show: always.
    expect(screen.getByText("This show's controllers aren't on your network. Add your own on the Devices screen.")).toBeInTheDocument();
  });

  it("notes it for any show whose controllers are all off this computer's networks, and not otherwise", async () => {
    const own = () => {
      const show = demoShow();
      show.controllers = show.controllers.map((c, i) => ({ ...c, name: `Mine ${i}`, address: `10.28.128.${175 + i}` }));
      return show;
    };
    const note = "This show's controllers aren't on your network. Add your own on the Devices screen.";
    await setup(own(), (b) => (b.localPrefixes = ["10.28.128."]));
    await screen.findByText(/^Not answering/);
    expect(screen.queryByText(note)).not.toBeInTheDocument();
    cleanup();
    await setup(own(), (b) => (b.localPrefixes = ["192.168.7."]));
    expect(await screen.findByText(note)).toBeInTheDocument();
  });
});
