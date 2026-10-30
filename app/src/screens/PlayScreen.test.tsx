import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { appWindow } from "../api/appWindow";
import { demoDevices, demoPlayers, demoShow } from "../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend, emptyShow } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import { newController } from "../lib/shows";
import { usePreviewWindow } from "../state/previewWindow";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { useTheatre } from "../state/theatre";

/** Opens the Play screen; `known` gives the show a controller that knows its sequence channels. */
async function openPlay(known: boolean) {
  const show = emptyShow("Home");
  if (known) show.controllers = [{ ...newController("Falcon", "192.0.2.20", "ddp", 0), sequenceChannels: { start: 1, count: 6147 } }];
  const backend = new MemoryBackend(show);
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  backend.nextSequencePath = "/Shows/Chimes Medley 2020.fseq";
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<App />);
  await user.click(screen.getByRole("button", { name: "Play" }));
  return { backend, user };
}

/** The Play screen with the demo sequence open in the editor (and `rendered` sequences on the playlist). */
async function openPlayWithSequence(rendered: string[] = []) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  backend.path = "/Shows/Demo House/Demo House.pixelflow.json";
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  for (const name of rendered) await backend.addSequence(`/Shows/${name}.fseq`);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "play" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  const name = useSequencer.getState().doc!.name;
  return { backend, seq, user, name };
}

const transport = () => screen.getByRole("region", { name: "Transport" });
const drawer = () => screen.getByRole("complementary", { name: "Playlist" });
const rendered = () => within(drawer()).getByRole("list", { name: "Rendered sequences" });
const music = () => within(drawer()).getByRole("region", { name: "Music" });

/** Adds rendered sequences by name: the first from the empty screen, the rest from the playlist. */
async function addSequences(user: ReturnType<typeof userEvent.setup>, backend: MemoryBackend, names: string[]) {
  for (const name of names) {
    backend.nextSequencePath = `/Shows/${name}.fseq`;
    await user.click(screen.getByRole("button", { name: /^Add (a rendered )?sequence/ }));
  }
  await screen.findByRole("region", { name: "Transport" });
}

afterEach(async () => {
  await act(async () => {
    if (useSequencer.getState().status) await useSequencer.getState().stop();
  });
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("play, with nothing to play yet", () => {
  it("says what the screen is for and what's needed before a rendered sequence can play", async () => {
    const { user } = await openPlay(false);
    expect(screen.getByRole("heading", { name: "Play" })).toBeInTheDocument();
    const empty = screen.getByRole("region", { name: "Nothing to play yet" });
    expect(empty).toHaveTextContent("Play shows your lights filling the window while a sequence plays.");
    // No transport and no playlist until there's something to play.
    expect(screen.queryByRole("region", { name: "Transport" })).not.toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Playlist" })).not.toBeInTheDocument();
    expect(within(empty).getByText(/doesn't know which channels go to which controller yet/)).toBeInTheDocument();
    await user.click(within(empty).getByRole("button", { name: "Go to Controllers" }));
    expect(screen.getByRole("heading", { name: "Controllers" })).toBeInTheDocument();
  });

  it("goes to make a sequence", async () => {
    const { user } = await openPlay(true);
    const empty = screen.getByRole("region", { name: "Nothing to play yet" });
    expect(within(empty).queryByText(/doesn't know which channels/)).not.toBeInTheDocument();
    await user.click(within(empty).getByRole("button", { name: "Make a sequence" }));
    expect(useApp.getState().screen).toBe("sequence");
  });

  it("adds a rendered sequence, which then shows ready to play with the playlist open", async () => {
    const { user, backend } = await openPlay(true);
    await user.click(screen.getByRole("button", { name: "Add a rendered sequence (.fseq)" }));
    expect(backend.calls).toContain("addSequence:/Shows/Chimes Medley 2020.fseq");
    expect(await within(transport()).findByRole("heading", { name: "Chimes Medley 2020" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Nothing to play yet" })).not.toBeInTheDocument();
    expect(within(rendered()).getByRole("button", { name: "Chimes Medley 2020" })).toHaveAttribute("aria-current", "true");
    // Added, not started.
    expect(backend.calls.some((c) => c.startsWith("playSequence"))).toBe(false);
  });
});

describe("play, with a sequence open in the editor", () => {
  it("shows it ready to play, with the Sequence screen's own playback", async () => {
    const { user, seq, name } = await openPlayWithSequence();
    expect(screen.queryByRole("region", { name: "Nothing to play yet" })).not.toBeInTheDocument();
    expect(within(transport()).getByRole("heading", { name })).toBeInTheDocument();
    expect(await screen.findByRole("img", { name: "Preview of the show at the playhead" })).toBeInTheDocument();
    // The playlist is put away while a sequence is open: it's what plays.
    expect(screen.queryByRole("complementary", { name: "Playlist" })).not.toBeInTheDocument();
    expect(within(transport()).getByText(/^0:00 \/ 1:00$/)).toBeInTheDocument();

    await user.click(within(transport()).getByRole("button", { name: "Play" }));
    expect(seq.calls).toContain("playSequenceDoc@0");
    expect(await within(transport()).findByRole("button", { name: "Pause" })).toBeInTheDocument();
    expect(useSequencer.getState().status?.state).toBe("playing");

    await user.click(within(transport()).getByRole("button", { name: "Pause" }));
    expect(await within(transport()).findByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(useSequencer.getState().status?.state).toBe("paused");

    // Stop lets go of the player where it is; pressed again, it goes back to the start.
    await act(() => useSequencer.getState().seek(20_000));
    await user.click(within(transport()).getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(useSequencer.getState().status).toBeNull());
    expect(useSequencer.getState().playheadMs).toBe(20_000);
    await user.click(within(transport()).getByRole("button", { name: "Back to the start" }));
    expect(useSequencer.getState().playheadMs).toBe(0);
    expect(within(transport()).getByRole("button", { name: "Stop" })).toBeDisabled();
  });

  it("seeks, loops, and shows it on the lights like the Sequence screen does", async () => {
    const { user, seq } = await openPlayWithSequence();
    const slider = within(transport()).getByRole("slider", { name: "Position" });
    slider.focus();
    await user.keyboard("{ArrowRight}{ArrowRight}{PageUp}");
    expect(useSequencer.getState().playheadMs).toBe(12_000);
    expect(slider).toHaveAttribute("aria-valuetext", "0:12");
    await user.click(within(transport()).getByRole("button", { name: "Restart" }));
    expect(useSequencer.getState().playheadMs).toBe(0);

    const loop = within(transport()).getByRole("button", { name: "Loop playback" });
    expect(loop).toHaveAttribute("aria-pressed", "false");
    await user.click(loop);
    expect(useSequencer.getState().looping).toBe(true);
    expect(seq.calls).toContain("setSequenceDocLoop:true");
    act(() => useSequencer.getState().setLooping(false));

    const lights = within(transport()).getByRole("button", { name: "Show on my lights" });
    expect(lights).toHaveAttribute("aria-pressed", "false");
    await user.click(lights);
    expect(seq.calls).toContain("setSequenceDocOutput:true");
    expect(within(transport()).getByRole("button", { name: "Show on my lights" })).toHaveAttribute("aria-pressed", "true");
  });

  it("stays in step with the Sequence screen: one playback, whichever screen is in front", async () => {
    const { user } = await openPlayWithSequence();
    await user.click(within(transport()).getByRole("button", { name: "Play" }));
    await within(transport()).findByRole("button", { name: "Pause" });
    await user.click(screen.getByRole("button", { name: "Sequence" }));
    const toolbar = screen.getByRole("toolbar", { name: "Sequence" });
    expect(within(toolbar).getByRole("button", { name: "Pause" })).toBeInTheDocument();
    await user.click(within(toolbar).getByRole("button", { name: "Pause" }));
    const at = useSequencer.getState().playheadMs;
    await user.click(within(screen.getByRole("navigation", { name: "Screens" })).getByRole("button", { name: "Play" }));
    expect(within(transport()).getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(within(transport()).getByRole("slider", { name: "Position" })).toHaveValue(String(at));
  });

  it("plays and pauses with Space, loops with L, and jumps with Home and End", async () => {
    const { user, seq } = await openPlayWithSequence();
    await user.keyboard(" ");
    expect(seq.calls).toContain("playSequenceDoc@0");
    await within(transport()).findByRole("button", { name: "Pause" });
    await user.keyboard(" ");
    await within(transport()).findByRole("button", { name: "Play" });
    await user.keyboard("l");
    expect(useSequencer.getState().looping).toBe(true);
    await user.keyboard("l");
    expect(useSequencer.getState().looping).toBe(false);
    await user.keyboard("{End}");
    expect(useSequencer.getState().playheadMs).toBe(60_000);
    await user.keyboard("{Home}");
    expect(useSequencer.getState().playheadMs).toBe(0);
  });

  it("lists what there is to play in the playlist drawer, and remembers the drawer", async () => {
    const { user, name } = await openPlayWithSequence(["Medley", "Wizards"]);
    await user.click(within(transport()).getByRole("button", { name: "Playlist" }));
    const open = within(drawer()).getByRole("group", { name: "Open in the editor" });
    expect(within(open).getByRole("button", { name })).toHaveAttribute("aria-current", "true");
    expect(
      within(rendered())
        .getAllByRole("listitem")
        .map((li) => li.querySelector("button")!.textContent),
    ).toEqual(["Medley", "Wizards"]);
    expect(within(drawer()).getByRole("checkbox", { name: "Play all and repeat" })).toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.playDrawer")).toBe("open");
    await user.click(within(drawer()).getByRole("button", { name: "Close the playlist" }));
    expect(screen.queryByRole("complementary", { name: "Playlist" })).not.toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.playDrawer")).toBe("closed");
  });

  it("plays a rendered sequence when it's picked, and the open one when that is", async () => {
    const { user, backend, seq, name } = await openPlayWithSequence(["Medley", "Wizards"]);
    await user.click(within(transport()).getByRole("button", { name: "Playlist" }));
    await user.click(within(rendered()).getByRole("button", { name: "Wizards" }));
    expect(backend.calls).toContain("playSequence:Wizards@0");
    expect(await within(transport()).findByRole("heading", { name: "Wizards" })).toBeInTheDocument();
    expect(within(transport()).getByRole("button", { name: "Pause" })).toBeInTheDocument();
    expect(within(rendered()).getByRole("button", { name: "Wizards" })).toHaveAttribute("aria-current", "true");
    // The lights live, not the open sequence at its playhead.
    expect(screen.getByRole("img", { name: "Preview of the lights" })).toBeInTheDocument();
    // Looping and "Show on my lights" are the open sequence's: a rendered one always goes to the lights.
    expect(within(transport()).queryByRole("button", { name: "Loop playback" })).not.toBeInTheDocument();
    expect(within(transport()).queryByRole("button", { name: "Show on my lights" })).not.toBeInTheDocument();
    // Its music and the lights' timing against it are in the drawer.
    expect(within(music()).getByText("Wizards.mp3")).toBeInTheDocument();
    expect(within(music()).getByRole("group", { name: "Light timing" })).toBeInTheDocument();

    await user.click(within(drawer()).getByRole("button", { name }));
    await waitFor(() => expect(seq.calls).toContain("playSequenceDoc@0"));
    expect(await within(transport()).findByRole("heading", { name })).toBeInTheDocument();
    expect(await within(transport()).findByRole("button", { name: "Pause" })).toBeInTheDocument();
    expect((await backend.playbackStatus())?.authored).toBe(true);
  });

  it("opens a sequence saved with this show from the playlist, asking about unsaved changes first", async () => {
    const { user, backend, seq } = await openPlayWithSequence();
    const other = "/Shows/Demo House/Carol of the Bells.pfseq.json";
    seq.files.set(other, { ...demoSequence(backend.show, 30_000), name: "Carol of the Bells" });
    act(() => useSequencer.setState({ recent: [{ path: other, show: useApp.getState().snapshot!.path }, ...useSequencer.getState().recent], dirty: true }));
    await user.click(within(transport()).getByRole("button", { name: "Playlist" }));
    const saved = within(drawer()).getByRole("group", { name: "Saved with this show" });
    await user.click(within(saved).getByRole("button", { name: "Carol of the Bells" }));
    // The open sequence has unsaved changes: nothing is replaced until that's settled.
    const question = screen.getByRole("dialog", { name: "Unsaved changes" });
    expect(useSequencer.getState().doc?.name).not.toBe("Carol of the Bells");
    await user.click(within(question).getByRole("button", { name: "Don't save" }));
    expect(await within(transport()).findByRole("heading", { name: "Carol of the Bells" })).toBeInTheDocument();
    expect(useSequencer.getState().path).toBe(other);
    await waitFor(() => expect(seq.calls).toContain("playSequenceDoc@0"));
  });
});

describe("play, full screen", () => {
  it("puts the app's chrome away so only the lights and the transport are left, and Escape brings it back", async () => {
    const fill = vi.spyOn(appWindow, "setFullscreen").mockResolvedValue();
    const { user } = await openPlayWithSequence();
    expect(screen.getByRole("navigation", { name: "Screens" })).toBeInTheDocument();
    await user.click(within(transport()).getByRole("button", { name: "Full screen" }));
    expect(fill).toHaveBeenLastCalledWith(true);
    expect(screen.queryByRole("navigation", { name: "Screens" })).not.toBeInTheDocument();
    expect(screen.queryByRole("banner")).not.toBeInTheDocument();
    expect(screen.queryByRole("contentinfo")).not.toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Preview of the show at the playhead" })).toBeInTheDocument();
    // The transport is still there, without what needs the rest of the app.
    expect(within(transport()).getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(within(transport()).queryByRole("button", { name: "Playlist" })).not.toBeInTheDocument();
    // The button that was clicked let go of the keys: Space plays.
    await user.keyboard(" ");
    await within(transport()).findByRole("button", { name: "Pause" });
    expect(useTheatre.getState().on).toBe(true);

    await user.keyboard("{Escape}");
    expect(fill).toHaveBeenLastCalledWith(false);
    expect(screen.getByRole("navigation", { name: "Screens" })).toBeInTheDocument();
    expect(screen.getByRole("banner")).toBeInTheDocument();
    // Still playing: leaving full screen doesn't touch playback.
    expect(within(transport()).getByRole("button", { name: "Pause" })).toBeInTheDocument();
  });

  it("goes in and out with F, and ends when the screen is left", async () => {
    vi.spyOn(appWindow, "setFullscreen").mockResolvedValue();
    const { user } = await openPlayWithSequence();
    await user.keyboard("f");
    expect(useTheatre.getState().on).toBe(true);
    expect(within(transport()).getByRole("button", { name: "Leave full screen" })).toHaveAttribute("aria-pressed", "true");
    await user.keyboard("f");
    expect(useTheatre.getState().on).toBe(false);
    await user.keyboard("f");
    act(() => useApp.getState().setScreen("layout"));
    expect(useTheatre.getState().on).toBe(false);
    expect(screen.getByRole("navigation", { name: "Screens" })).toBeInTheDocument();
  });

  it("ends when the window stops filling its display some other way", async () => {
    vi.spyOn(appWindow, "setFullscreen").mockResolvedValue();
    let changed: (on: boolean) => void = () => {};
    vi.spyOn(appWindow, "onFullscreenChange").mockImplementation((handler) => {
      changed = handler;
      return () => {};
    });
    const { user } = await openPlayWithSequence();
    await user.click(within(transport()).getByRole("button", { name: "Full screen" }));
    // Not filling it yet (the system is still getting there) changes nothing.
    act(() => changed(false));
    expect(useTheatre.getState().on).toBe(true);
    act(() => changed(true));
    act(() => changed(false));
    expect(useTheatre.getState().on).toBe(false);
    expect(screen.getByRole("navigation", { name: "Screens" })).toBeInTheDocument();
  });

  it("hides the transport while a sequence plays, until the pointer moves", async () => {
    vi.spyOn(appWindow, "setFullscreen").mockResolvedValue();
    await openPlayWithSequence();
    const chrome = () => transport().parentElement!;
    fireEvent.click(within(transport()).getByRole("button", { name: "Play" }));
    await within(transport()).findByRole("button", { name: "Pause" });
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    act(() => useTheatre.getState().enter());
    expect(chrome()).not.toHaveAttribute("data-hidden");
    act(() => void vi.advanceTimersByTime(3000));
    expect(chrome()).toHaveAttribute("data-hidden");
    fireEvent.pointerMove(window);
    expect(chrome()).not.toHaveAttribute("data-hidden");
  });
});

describe("play, with the preview in its own window", () => {
  it("leaves a note where the preview was, with the way back", async () => {
    const { user, backend } = await openPlayWithSequence();
    await user.click(within(transport()).getByRole("button", { name: "Pop out the preview" }));
    expect(backend.calls).toContain("openPreviewWindow");
    const note = await screen.findByRole("region", { name: "Preview" });
    expect(note).toHaveTextContent("Preview is in its own window");
    expect(screen.queryByRole("img", { name: "Preview of the show at the playhead" })).not.toBeInTheDocument();
    // The transport stays; full screen has nothing to fill the window with.
    expect(within(transport()).getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(within(transport()).queryByRole("button", { name: "Full screen" })).not.toBeInTheDocument();
    // What the window shows is said in the main window's note.
    await waitFor(async () => expect((await backend.previewLink("preview")).theirs).toMatchObject({ source: "sequence", transport: true }));

    await user.click(within(note).getByRole("button", { name: "Bring the preview back" }));
    expect(backend.calls).toContain("closePreviewWindow");
    expect(await screen.findByRole("img", { name: "Preview of the show at the playhead" })).toBeInTheDocument();
    expect(usePreviewWindow.getState().poppedOut).toBe(false);
  });
});

describe("play, rendered sequences", () => {
  it("adds a sequence with its music, plays it, and lines the lights up with the music", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Chimes Medley 2020"]);
    expect(await within(rendered()).findByText("Chimes Medley 2020")).toBeInTheDocument();
    expect(within(music()).getByText("Chimes Medley 2020.mp3")).toBeInTheDocument();
    expect(within(music()).getByRole("img", { name: /music waveform/i })).toBeInTheDocument();

    await user.click(within(transport()).getByRole("button", { name: "Play" }));
    expect(backend.calls).toContain("playSequence:Chimes Medley 2020@0");
    expect(await within(transport()).findByRole("button", { name: "Pause" })).toBeInTheDocument();
    expect(within(transport()).getByText(/\/ 1:00$/)).toBeInTheDocument();

    const alignment = within(music()).getByRole("group", { name: "Light timing" });
    expect(within(alignment).getByText("Move the lights earlier or later against the music.")).toBeInTheDocument();
    expect(within(alignment).getByText("Light timing").getAttribute("title")).toMatch(/isn't saved into exported files or Send to FPP.*separate from Preview sync/);
    expect(within(alignment).getByText("Lights are in sync with the music")).toBeInTheDocument();
    await user.click(within(alignment).getByRole("button", { name: "+50 ms: lights earlier" }));
    await user.click(within(alignment).getByRole("button", { name: "−10 ms: lights later" }));
    expect(within(alignment).getByText("Lights are 40 ms ahead of the music")).toBeInTheDocument();
    await useApp.getState().undo();
    expect(await within(alignment).findByText("Lights are 50 ms ahead of the music")).toBeInTheDocument();

    await user.click(within(transport()).getByRole("button", { name: "Pause" }));
    expect(await within(transport()).findByRole("button", { name: "Play" })).toBeInTheDocument();
    await user.click(within(transport()).getByRole("button", { name: "Stop" }));
    expect(backend.calls).toContain("stopPlayback");
  });

  it("plays every sequence in order when asked", async () => {
    const { user, backend } = await openPlay(true);
    backend.sequenceDurationMs = 30;
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(within(drawer()).getByRole("checkbox", { name: "Play all and repeat" }));
    await user.click(within(rendered()).getByRole("button", { name: "Medley" }));
    expect(backend.calls).toContain("playSequence:Medley@0");
    await waitFor(() => expect(backend.calls).toContain("playSequence:Wizards@0"), { timeout: 2000 });
  });

  it("goes from the last sequence back to the first: the transport follows what plays, and what's picked stays", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(within(drawer()).getByRole("checkbox", { name: "Play all and repeat" }));
    backend.sequenceDurationMs = 30;
    await user.click(within(rendered()).getByRole("button", { name: "Wizards" }));
    await waitFor(() => expect(backend.calls).toContain("playSequence:Medley@0"), { timeout: 2000 });
    backend.sequenceDurationMs = 60_000;
    // Medley is on the transport now, to pause or stop, while Wizards stays picked (its music shows).
    expect(await within(transport()).findByRole("heading", { name: "Medley" }, { timeout: 2000 })).toBeInTheDocument();
    expect(within(rendered()).getByRole("button", { name: "Wizards" })).toHaveAttribute("aria-current", "true");
    expect(within(music()).getByText("Wizards.mp3")).toBeInTheDocument();
    await user.click(within(drawer()).getByRole("checkbox", { name: "Play all and repeat" }));
    await user.click(await within(transport()).findByRole("button", { name: "Stop" }));
    expect(backend.calls).toContain("stopPlayback");
    // Nothing plays: the transport is back on what's picked.
    expect(await within(transport()).findByRole("heading", { name: "Wizards" })).toBeInTheDocument();
  });

  it("doesn't jump to the next sequence when play all is turned on after one finished", async () => {
    const { user, backend } = await openPlay(true);
    backend.sequenceDurationMs = 30;
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(within(rendered()).getByRole("button", { name: "Medley" }));
    expect(await screen.findByText("Finished. Press play to start again.", {}, { timeout: 2000 })).toBeInTheDocument();
    await user.click(within(drawer()).getByRole("checkbox", { name: "Play all and repeat" }));
    await new Promise((r) => setTimeout(r, 400));
    expect(backend.calls).not.toContain("playSequence:Wizards@0");
  });

  it("moves the slider a second at a time by keyboard and seeks when you leave it", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    await user.click(within(transport()).getByRole("button", { name: "Play" }));
    await within(transport()).findByRole("button", { name: "Pause" });
    const slider = within(transport()).getByRole("slider", { name: "Position" });
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
    const slider = within(transport()).getByRole("slider", { name: "Position" });
    await waitFor(() => expect(slider).toBeEnabled());
    slider.focus();
    await user.keyboard("{PageUp}");
    await user.tab();
    expect(backend.calls).toContain("playSequence:Medley@10000");
  });

  it("jumps to where the waveform is clicked", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const waveform = await within(music()).findByRole("img", { name: /music waveform/i });
    vi.spyOn(waveform, "getBoundingClientRect").mockReturnValue({ left: 0, width: 200, top: 0, height: 64, right: 200, bottom: 64, x: 0, y: 0, toJSON: () => ({}) });
    fireEvent.click(waveform, { clientX: 50 });
    // A quarter of the way along a one-minute song.
    expect(backend.calls).toContain("playSequence:Medley@15000");
  });

  it("says when the music is still being read", async () => {
    const { user, backend } = await openPlay(true);
    backend.audioWaveform = () => new Promise(() => {});
    await addSequences(user, backend, ["Medley"]);
    expect(await within(music()).findByText("Reading the music…")).toBeInTheDocument();
    expect(within(music()).getByRole("img", { name: "Loading the music waveform" })).toBeInTheDocument();
  });

  it("sets the volume before playing and keeps it", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    fireEvent.change(within(music()).getByRole("slider", { name: "Volume" }), { target: { value: "40" } });
    await user.click(within(transport()).getByRole("button", { name: "Play" }));
    await within(transport()).findByRole("button", { name: "Pause" });
    expect((await backend.playbackStatus())!.volume).toBe(0.4);
    expect(within(music()).getByRole("slider", { name: "Volume" })).toHaveValue("40");
  });

  it("adds quick alignment clicks together", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const alignment = within(music()).getByRole("group", { name: "Light timing" });
    const earlier = within(alignment).getByRole("button", { name: "+50 ms: lights earlier" });
    fireEvent.click(earlier);
    fireEvent.click(earlier);
    expect(await within(alignment).findByText("Lights are 100 ms ahead of the music")).toBeInTheDocument();
  });

  it("switches to another sequence when it's picked while one plays", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley", "Wizards"]);
    await user.click(within(rendered()).getByRole("button", { name: "Medley" }));
    expect(await within(transport()).findByRole("heading", { name: "Medley" })).toBeInTheDocument();
    await user.click(within(rendered()).getByRole("button", { name: "Wizards" }));
    expect(backend.calls).toContain("playSequence:Wizards@0");
    expect(await within(transport()).findByRole("heading", { name: "Wizards" })).toBeInTheDocument();
    // Picked again while it plays, it carries on rather than starting over.
    const starts = backend.calls.filter((c) => c === "playSequence:Wizards@0").length;
    await user.click(within(rendered()).getByRole("button", { name: "Wizards" }));
    expect(backend.calls.filter((c) => c === "playSequence:Wizards@0")).toHaveLength(starts);
    await user.click(within(transport()).getByRole("button", { name: "Stop" }));
    expect(backend.calls).toContain("stopPlayback");
  });

  it("keeps reordering reachable, with focus off buttons that stop working", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley", "Wizards", "Medley"]);
    const list = rendered();
    expect(within(list).getByRole("button", { name: "Medley (2)" })).toBeInTheDocument();
    expect(within(list).getByRole("button", { name: "Medley (2)" })).toHaveAttribute("aria-current", "true");
    expect(within(list).getByRole("button", { name: "Medley" })).not.toHaveAttribute("aria-current");
    await user.click(within(list).getByRole("button", { name: "Move Wizards up" }));
    const names = within(rendered())
      .getAllByRole("listitem")
      .map((li) => li.querySelector("button")!.textContent);
    expect(names).toEqual(["Wizards", "Medley", "Medley (2)"]);
    await waitFor(() => expect(within(rendered()).getByRole("button", { name: "Move Wizards down" })).toHaveFocus());
  });

  it("says why playback stopped when an edit to the show ended it", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    await user.click(within(transport()).getByRole("button", { name: "Play" }));
    await within(transport()).findByRole("button", { name: "Pause" });
    const id = useApp.getState().snapshot!.show.controllers[0].id;
    await backend.applyEdits([{ type: "removeController", id }]);
    expect(await screen.findByText("Playback stopped because no controller has sequence channels anymore.")).toBeInTheDocument();
    expect(within(transport()).getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(within(transport()).getByRole("button", { name: "Stop" })).toBeDisabled();
  });

  it("says a rendered sequence can't reach the lights until the controllers know their channels", async () => {
    const { user, backend } = await openPlay(false);
    await addSequences(user, backend, ["Medley"]);
    expect(screen.getByText(/doesn't know which channels go to which controller yet/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Go to Controllers" }));
    expect(useApp.getState().screen).toBe("devices");
  });
});

describe("play, with an FPP", () => {
  it("disables Stop FPP while stopping and checks again right away", async () => {
    const { user, backend } = await openPlay(true);
    await useApp.getState().scan();
    const warning = await screen.findByRole("alert", { name: "FPP is playing" });
    await user.click(within(warning).getByRole("button", { name: "Stop FPP" }));
    expect(backend.calls).toContain("fppStop:192.0.2.10:now");
    await waitFor(() => expect(screen.queryByRole("alert", { name: "FPP is playing" })).not.toBeInTheDocument());
  });

  it("warns in the main view when an FPP is playing, and can stop it", async () => {
    const { user, backend } = await openPlayWithSequence();
    await act(() => useApp.getState().scan());
    const warning = await screen.findByRole("alert", { name: "FPP is playing" });
    expect(within(warning).getByText(/FPP is playing Chimes Medley 2020.fseq/)).toBeInTheDocument();
    // In the main view, with the playlist put away.
    expect(screen.queryByRole("complementary", { name: "Playlist" })).not.toBeInTheDocument();
    await user.click(within(warning).getByRole("button", { name: "Stop FPP" }));
    expect(backend.calls).toContain("fppStop:192.0.2.10:now");
  });

  it("shows which sequences are on the FPP and sends a missing one", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Chimes Medley 2020", "Wizards in Winter"]);
    // Until the FPP is known, there's nothing to say.
    expect(screen.queryByRole("region", { name: "On your FPP" })).not.toBeInTheDocument();
    await useApp.getState().scan();
    const panel = await within(drawer()).findByRole("region", { name: "On your FPP" });
    expect(await within(panel).findByText("1 of 2 sequences are on FPP.")).toBeInTheDocument();
    const medley = within(panel).getByRole("listitem", { name: "Chimes Medley 2020" });
    // Only the name is compared, and it says so.
    expect(medley).toHaveTextContent("Same name on the FPP");
    expect(within(medley).getByTitle(/doesn't check that it's the same version/)).toBeInTheDocument();
    expect(backend.calls.some((c) => c.startsWith("fppSend"))).toBe(false);

    await user.click(within(panel).getByRole("button", { name: "Send Wizards in Winter to FPP" }));
    const dialog = screen.getByRole("dialog", { name: "Send to FPP" });
    expect(within(dialog).getByRole("combobox", { name: "FPP" })).toHaveValue("192.0.2.10");
    await within(dialog).findByText(/free/);
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" });
    expect(backend.calls).toContain("fppSend:192.0.2.10:Wizards in Winter.fseq:none");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(await within(panel).findByText("All 2 sequences are on FPP.")).toBeInTheDocument();
  });

  it("counts a sequence as on the FPP only under its exact name", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Wizards in Winter"]);
    backend.fppPlayers["192.0.2.10"].sequences.push({ name: "wizards in winter", frames: 1, stepMs: 50, channels: 3 });
    await useApp.getState().scan();
    const panel = await screen.findByRole("region", { name: "On your FPP" });
    expect(await within(panel).findByText("0 of 1 sequences are on FPP.")).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Send Wizards in Winter to FPP" })).toBeInTheDocument();
  });

  it("says when the FPP can't be checked", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Wizards in Winter"]);
    delete backend.fppPlayers["192.0.2.10"];
    await useApp.getState().scan();
    const panel = await screen.findByRole("region", { name: "On your FPP" });
    expect(await within(panel).findByText(/Couldn't check FPP/)).toBeInTheDocument();
  });

  it("folds the drawer's sections away, and remembers which", async () => {
    const { user, backend } = await openPlay(true);
    await addSequences(user, backend, ["Medley"]);
    const heading = within(music()).getByRole("button", { name: "Music" });
    expect(heading).toHaveAttribute("aria-expanded", "true");
    await user.click(heading);
    expect(heading).toHaveAttribute("aria-expanded", "false");
    expect(within(music()).queryByRole("group", { name: "Light timing" })).not.toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("pixelflow.playSections")!)).toEqual({ music: true });
  });
});
