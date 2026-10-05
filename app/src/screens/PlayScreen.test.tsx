import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoDevices, demoPlayers } from "../api/demo";
import { MemoryBackend, emptyShow } from "../api/memory";
import { newController } from "../lib/shows";
import { useApp } from "../state/store";

/** Opens the Play screen; `known` gives the show a controller that knows its sequence channels. */
async function openPlay(known: boolean) {
  const show = emptyShow("Home");
  if (known) show.controllers = [{ ...newController("Falcon", "192.0.2.20", "ddp", 0), sequenceChannels: { start: 1, count: 6147 } }];
  const backend = new MemoryBackend(show);
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  backend.nextSequencePath = "/Shows/Christmas Medley 2017.fseq";
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<App />);
  await user.click(screen.getByRole("button", { name: "Play" }));
  return { backend, user };
}

describe("play", () => {
  it("explains what's needed before a sequence can play", async () => {
    const { user } = await openPlay(false);
    expect(screen.getByRole("heading", { name: "Play" })).toBeInTheDocument();
    expect(screen.getByText(/doesn't know which channels go to which controller yet/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Go to Devices" }));
    expect(screen.getByRole("heading", { name: "Devices" })).toBeInTheDocument();
  });

  it("adds a sequence with its music, plays it, and lines the lights up with the music", async () => {
    const { user, backend } = await openPlay(true);
    const list = screen.getByRole("complementary", { name: "Sequences" });
    await user.click(within(list).getByRole("button", { name: "Add sequence" }));
    expect(backend.calls).toContain("addSequence:/Shows/Christmas Medley 2017.fseq");
    expect(await within(list).findByText("Christmas Medley 2017")).toBeInTheDocument();
    const transport = screen.getByRole("region", { name: "Transport" });
    expect(within(transport).getByText("Christmas Medley 2017.mp3")).toBeInTheDocument();
    expect(within(transport).getByRole("img", { name: /music waveform/i })).toBeInTheDocument();

    await user.click(within(transport).getByRole("button", { name: "Play" }));
    expect(backend.calls).toContain("playSequence:Christmas Medley 2017@0");
    expect(await within(transport).findByRole("button", { name: "Pause" })).toBeInTheDocument();
    expect(within(transport).getByText(/\/ 1:00$/)).toBeInTheDocument();

    const alignment = within(transport).getByRole("group", { name: "Music alignment" });
    expect(within(alignment).getByText("Lights are in sync with the music")).toBeInTheDocument();
    await user.click(within(alignment).getByRole("button", { name: "Lights earlier by 50 ms" }));
    await user.click(within(alignment).getByRole("button", { name: "Lights later by 10 ms" }));
    expect(within(alignment).getByText("Lights are 40 ms ahead of the music")).toBeInTheDocument();
    await useApp.getState().undo();
    expect(await within(alignment).findByText("Lights are 50 ms ahead of the music")).toBeInTheDocument();

    await user.click(within(transport).getByRole("button", { name: "Pause" }));
    expect(await within(transport).findByRole("button", { name: "Play" })).toBeInTheDocument();
    await user.click(within(transport).getByRole("button", { name: "Stop" }));
    expect(backend.calls).toContain("stopPlayback");
  });

  it("plays every sequence in order when asked", async () => {
    const { user, backend } = await openPlay(true);
    backend.sequenceDurationMs = 30;
    for (const name of ["Medley", "Wizards"]) {
      backend.nextSequencePath = `/Shows/${name}.fseq`;
      await user.click(screen.getByRole("button", { name: "Add sequence" }));
    }
    await user.click(await screen.findByRole("button", { name: "Medley" }));
    await user.click(screen.getByRole("checkbox", { name: "Play all in order" }));
    await user.click(within(screen.getByRole("region", { name: "Transport" })).getByRole("button", { name: "Play" }));
    await waitFor(() => expect(backend.calls).toContain("playSequence:Wizards@0"), { timeout: 2000 });
  });

  it("warns when an FPP is playing and can stop it", async () => {
    const { user, backend } = await openPlay(true);
    await useApp.getState().scan();
    const warning = await screen.findByRole("alert", { name: "FPP is playing" });
    expect(within(warning).getByText(/FPP is playing Christmas Medley 2017.fseq/)).toBeInTheDocument();
    await user.click(within(warning).getByRole("button", { name: "Stop FPP" }));
    expect(backend.calls).toContain("fppStop:192.0.2.10:now");
  });
});
