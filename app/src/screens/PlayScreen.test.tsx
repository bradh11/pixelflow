import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
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

/** Adds sequences by name, through the Add button. */
async function addSequences(user: ReturnType<typeof userEvent.setup>, backend: MemoryBackend, names: string[]) {
  for (const name of names) {
    backend.nextSequencePath = `/Shows/${name}.fseq`;
    await user.click(screen.getByRole("button", { name: "Add sequence" }));
  }
  await screen.findByRole("region", { name: "Transport" });
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
    await user.click(within(alignment).getByRole("button", { name: "+50 ms: lights earlier" }));
    await user.click(within(alignment).getByRole("button", { name: "−10 ms: lights later" }));
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
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(await screen.findByRole("button", { name: "Medley" }));
    await user.click(screen.getByRole("checkbox", { name: "Play all and repeat" }));
    await user.click(within(screen.getByRole("region", { name: "Transport" })).getByRole("button", { name: "Play" }));
    await waitFor(() => expect(backend.calls).toContain("playSequence:Wizards@0"), { timeout: 2000 });
  });

  it("goes from the last sequence back to the first, leaving the selection alone", async () => {
    const { user, backend } = await openPlay(true);
    backend.sequenceDurationMs = 30;
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(screen.getByRole("button", { name: "Wizards" }));
    await user.click(screen.getByRole("checkbox", { name: "Play all and repeat" }));
    await user.click(within(screen.getByRole("region", { name: "Transport" })).getByRole("button", { name: "Play" }));
    await waitFor(() => expect(backend.calls).toContain("playSequence:Medley@0"), { timeout: 2000 });
    const transport = screen.getByRole("region", { name: "Transport" });
    expect(within(transport).getByRole("heading", { name: "Wizards" })).toBeInTheDocument();
    expect(await screen.findByRole("region", { name: "Now playing" })).toHaveTextContent(/Medley/);
  });

  it("doesn't jump to the next sequence when play all is turned on after one finished", async () => {
    const { user, backend } = await openPlay(true);
    backend.sequenceDurationMs = 30;
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(screen.getByRole("button", { name: "Medley" }));
    const transport = screen.getByRole("region", { name: "Transport" });
    await user.click(within(transport).getByRole("button", { name: "Play" }));
    expect(await within(transport).findByText("Finished. Press play to start again.", {}, { timeout: 2000 })).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "Play all and repeat" }));
    await new Promise((r) => setTimeout(r, 400));
    expect(backend.calls).not.toContain("playSequence:Wizards@0");
  });

  it("moves the slider a second at a time by keyboard and seeks when you leave it", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const transport = screen.getByRole("region", { name: "Transport" });
    await user.click(within(transport).getByRole("button", { name: "Play" }));
    await within(transport).findByRole("button", { name: "Pause" });
    const slider = within(transport).getByRole("slider", { name: "Position" });
    slider.focus();
    await user.keyboard("{ArrowRight}{ArrowRight}");
    expect(slider).toHaveAttribute("aria-valuetext", "0:02");
    await user.tab();
    const position = (await backend.playbackStatus())!.positionMs;
    expect(position).toBeGreaterThanOrEqual(2000);
    expect(position).toBeLessThan(3000);
  });

  it("starts playing from where the slider is moved to when stopped", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const slider = within(screen.getByRole("region", { name: "Transport" })).getByRole("slider", { name: "Position" });
    slider.focus();
    await user.keyboard("{PageUp}");
    await user.tab();
    expect(backend.calls).toContain("playSequence:Medley@10000");
  });

  it("jumps to where the waveform is clicked", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const transport = screen.getByRole("region", { name: "Transport" });
    const waveform = await within(transport).findByRole("img", { name: /music waveform/i });
    vi.spyOn(waveform, "getBoundingClientRect").mockReturnValue({ left: 0, width: 200, top: 0, height: 64, right: 200, bottom: 64, x: 0, y: 0, toJSON: () => ({}) });
    fireEvent.click(waveform, { clientX: 50 });
    // A quarter of the way along a one-minute song.
    expect(backend.calls).toContain("playSequence:Medley@15000");
  });

  it("says when the music is still being read", async () => {
    const { user, backend } = await openPlay(true);
    backend.audioWaveform = () => new Promise(() => {});
    await addSequences(user, backend, ["Medley"]);
    expect(await screen.findByText("Reading the music…")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Loading the music waveform" })).toBeInTheDocument();
  });

  it("sets the volume before playing and keeps it", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const transport = screen.getByRole("region", { name: "Transport" });
    fireEvent.change(within(transport).getByRole("slider", { name: "Volume" }), { target: { value: "40" } });
    await user.click(within(transport).getByRole("button", { name: "Play" }));
    await within(transport).findByRole("button", { name: "Pause" });
    expect((await backend.playbackStatus())!.volume).toBe(0.4);
    expect(within(transport).getByRole("slider", { name: "Volume" })).toHaveValue("40");
  });

  it("adds quick alignment clicks together", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const alignment = screen.getByRole("group", { name: "Music alignment" });
    const earlier = within(alignment).getByRole("button", { name: "+50 ms: lights earlier" });
    fireEvent.click(earlier);
    fireEvent.click(earlier);
    expect(await within(alignment).findByText("Lights are 100 ms ahead of the music")).toBeInTheDocument();
  });

  it("keeps showing the playing sequence when another one is selected", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(screen.getByRole("button", { name: "Medley" }));
    await user.click(within(screen.getByRole("region", { name: "Transport" })).getByRole("button", { name: "Play" }));
    await user.click(screen.getByRole("button", { name: "Wizards" }));
    const playing = await screen.findByRole("region", { name: "Now playing" });
    expect(playing).toHaveTextContent(/Now playing: Medley/);
    await user.click(within(playing).getByRole("button", { name: "Stop" }));
    expect(backend.calls).toContain("stopPlayback");
    await waitFor(() => expect(screen.queryByRole("region", { name: "Now playing" })).not.toBeInTheDocument());
  });

  it("keeps reordering reachable, with focus off buttons that stop working", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley", "Wizards", "Medley"]);
    const list = screen.getByRole("complementary", { name: "Sequences" });
    expect(within(list).getByRole("button", { name: "Medley (2)" })).toBeInTheDocument();
    expect(within(list).getByRole("button", { name: "Medley (2)" })).toHaveAttribute("aria-current", "true");
    expect(within(list).getByRole("button", { name: "Medley" })).not.toHaveAttribute("aria-current");
    await user.click(within(list).getByRole("button", { name: "Move Wizards up" }));
    const names = within(list)
      .getAllByRole("listitem")
      .map((li) => li.querySelector("button")!.textContent);
    expect(names).toEqual(["Wizards", "Medley", "Medley (2)"]);
    await waitFor(() => expect(within(list).getByRole("button", { name: "Move Wizards down" })).toHaveFocus());
  });

  it("says why playback stopped when an edit to the show ended it", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const transport = screen.getByRole("region", { name: "Transport" });
    await user.click(within(transport).getByRole("button", { name: "Play" }));
    await within(transport).findByRole("button", { name: "Pause" });
    const id = useApp.getState().snapshot!.show.controllers[0].id;
    await backend.applyEdits([{ type: "removeController", id }]);
    expect(await screen.findByText("Playback stopped because no controller has sequence channels anymore.")).toBeInTheDocument();
    expect(within(transport).getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(within(transport).getByRole("button", { name: "Stop" })).toBeDisabled();
  });

  it("disables Stop FPP while stopping and checks again right away", async () => {
    const { user, backend } = await openPlay(true);
    await useApp.getState().scan();
    const warning = await screen.findByRole("alert", { name: "FPP is playing" });
    await user.click(within(warning).getByRole("button", { name: "Stop FPP" }));
    expect(backend.calls).toContain("fppStop:192.0.2.10:now");
    await waitFor(() => expect(screen.queryByRole("alert", { name: "FPP is playing" })).not.toBeInTheDocument());
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
