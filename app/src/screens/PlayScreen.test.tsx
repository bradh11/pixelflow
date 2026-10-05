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

  it("opens a sequence and controls playback", async () => {
    const { user, backend } = await openPlay(true);
    await user.click(screen.getByRole("button", { name: /open sequence/i }));
    expect(backend.calls).toContain("startPlayback:/Shows/Christmas Medley 2017.fseq");
    const transport = await screen.findByRole("region", { name: "Transport" });
    expect(within(transport).getByText("Christmas Medley 2017.fseq")).toBeInTheDocument();
    expect(within(transport).getByText(/\/ 1:00$/)).toBeInTheDocument();
    expect(within(transport).getByRole("slider", { name: "Position" })).toBeInTheDocument();

    expect(screen.getByLabelText(/^Falcon: 2,049 pixels as received/)).toBeInTheDocument();

    await user.click(within(transport).getByRole("button", { name: "Pause" }));
    expect(await within(transport).findByRole("button", { name: "Play" })).toBeInTheDocument();
    await user.click(within(transport).getByRole("button", { name: "Stop" }));
    expect(backend.calls).toContain("stopPlayback");
    expect(await screen.findByRole("button", { name: /open sequence/i })).toBeInTheDocument();
  });

  it("warns when an FPP is playing and can stop it", async () => {
    const { user, backend } = await openPlay(true);
    await useApp.getState().scan();
    const warning = await screen.findByRole("alert", { name: "FPP is playing" });
    expect(within(warning).getByText(/FPP is playing Christmas Medley 2017.fseq/)).toBeInTheDocument();
    await user.click(within(warning).getByRole("button", { name: "Stop FPP" }));
    expect(backend.calls).toContain("fppStop:192.0.2.10:now");
  });

  it("moves the slider a second at a time by keyboard and seeks when you leave it", async () => {
    const { user, backend } = await openPlay(true);
    await user.click(screen.getByRole("button", { name: /open sequence/i }));
    const slider = await screen.findByRole("slider", { name: "Position" });
    slider.focus();
    await user.keyboard("{ArrowRight}{ArrowRight}");
    expect(slider).toHaveAttribute("aria-valuetext", "0:02");
    await user.tab();
    const position = (await backend.playbackStatus())!.positionMs;
    expect(position).toBeGreaterThanOrEqual(2000);
    expect(position).toBeLessThan(3000);
  });

  it("says why playback stopped when an edit to the show ended it", async () => {
    const { user, backend } = await openPlay(true);
    await user.click(screen.getByRole("button", { name: /open sequence/i }));
    await screen.findByRole("region", { name: "Transport" });
    const id = useApp.getState().snapshot!.show.controllers[0].id;
    await backend.applyEdits([{ type: "removeController", id }]);
    expect(await screen.findByText("Playback stopped because no controller has sequence channels anymore.")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Transport" })).not.toBeInTheDocument();
  });

  it("disables Stop FPP while stopping and checks again right away", async () => {
    const { user, backend } = await openPlay(true);
    await useApp.getState().scan();
    const warning = await screen.findByRole("alert", { name: "FPP is playing" });
    await user.click(within(warning).getByRole("button", { name: "Stop FPP" }));
    expect(backend.calls).toContain("fppStop:192.0.2.10:now");
    await waitFor(() => expect(screen.queryByRole("alert", { name: "FPP is playing" })).not.toBeInTheDocument());
  });
});
