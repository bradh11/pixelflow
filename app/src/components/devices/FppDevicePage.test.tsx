import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoDevices, demoFppFileDetails, demoFppFiles, demoFppSchedules, demoPlayers } from "../../api/demo";
import { MemoryBackend, emptyShow } from "../../api/memory";
import { useApp } from "../../state/store";
import { STATUS_POLL_MS } from "./useFppStatus";

const FPP = "192.0.2.10";
const FALCON = "192.0.2.20";

/** A show with one sequence, and the demo network: an FPP (playing, sending DDP to a Falcon it
 * can't reach) with its files and schedule. */
function stocked(setup?: (backend: MemoryBackend) => void) {
  const show = emptyShow("Home");
  show.sequences = [{ id: "seq-1", name: "Wizards", path: "/Shows/Wizards.fseq", audio: "/Shows/Wizards.mp3", offsetMs: 0 }];
  const backend = new MemoryBackend(show);
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  backend.fppFiles = demoFppFiles();
  backend.fppFileDetails = demoFppFileDetails();
  backend.fppSchedules = demoFppSchedules();
  setup?.(backend);
  return backend;
}

/** Opens the FPP's page from the Devices screen, as a user does. */
async function openPage(setup?: (backend: MemoryBackend) => void) {
  const backend = stocked(setup);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<App />);
  act(() => useApp.getState().setScreen("devices"));
  await user.click(screen.getByRole("button", { name: "Scan network" }));
  await user.click(await screen.findByRole("button", { name: "Open FPP" }));
  await screen.findByRole("heading", { name: "FPP" });
  return { backend, user };
}

const region = (name: string) => screen.getByRole("region", { name });
const pageHeader = () => screen.getByRole("heading", { name: "FPP" }).closest("header")!;

/** The FPP was told to start or stop something. */
const writes = (backend: MemoryBackend) => backend.calls.filter((c) => /^fpp(Start|Stop|Send):/.test(c));

describe("an FPP's page", () => {
  afterEach(() => {
    vi.useRealTimers();
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  });

  it("heads the page with its name, model, version, address, and state, and opens FPP's web page", async () => {
    const { backend, user } = await openPage();
    const header = pageHeader();
    expect(header).toHaveTextContent("Pi 3 Model B+ · FPP 9.3 · 192.0.2.10");
    expect(await within(header).findByRole("status")).toHaveTextContent("Playing");
    const link = within(header).getByRole("link", { name: "Open FPP's web page" });
    expect(link).toHaveAttribute("href", "http://192.0.2.10/");
    await user.click(link);
    expect(backend.calls).toContain(`openDevicePage:${FPP}`);
    expect(writes(backend)).toEqual([]);
  });

  it("shows what's playing, how far along, and what's next, and stops only when clicked", async () => {
    const { backend, user } = await openPage();
    const playing = region("Now playing");
    expect(await within(playing).findByText("Christmas Medley 2017.fseq", { selector: "p" })).toBeInTheDocument();
    const bar = within(playing).getByRole("progressbar", { name: "How far along" });
    expect(bar).toHaveAttribute("aria-valuenow", "109");
    expect(bar).toHaveAttribute("aria-valuemax", "565");
    expect(within(playing).getByText("1:49")).toBeInTheDocument();
    expect(within(playing).getByText("7:36 left")).toBeInTheDocument();
    expect(within(playing).getByText(/^Next:/).parentElement).toHaveTextContent("Next: Christmas Medley 2017.fseq · Mon Oct 5 @ 06:48 PM - (Everyday)");
    expect(writes(backend)).toEqual([]);

    await user.click(within(playing).getByRole("button", { name: /Stop after this/ }));
    expect(backend.calls).toContain(`fppStop:${FPP}:gracefully`);
    expect(await within(playing).findByText("Stopping after this sequence")).toBeInTheDocument();
    await user.click(within(playing).getByRole("button", { name: "Stop now" }));
    expect(backend.calls).toContain(`fppStop:${FPP}:now`);
    expect(await within(playing).findByText("Nothing playing.")).toBeInTheDocument();
    expect(within(playing).queryByRole("button", { name: "Stop now" })).not.toBeInTheDocument();
  });

  it("says in plain words that it can't reach the Falcon it sends to, and what to do", async () => {
    await openPage();
    const health = region("Health");
    expect(await within(health).findByText(`Can't reach the Falcon at ${FALCON} that this FPP sends to.`)).toBeInTheDocument();
    expect(within(health).getByText("Check it's powered on and plugged into the network.")).toBeInTheDocument();
    expect(within(health).queryByText(/Cannot Ping/)).not.toBeInTheDocument();
    const target = within(await within(health).findByRole("list", { name: "Output targets" })).getByRole("listitem");
    expect(target).toHaveTextContent("Falcon_F16V5_B9F5");
    expect(await within(target).findByText("Not answering")).toBeInTheDocument();
  });

  it("marks a target that answers, and says all's well when nothing is reported", async () => {
    await openPage((b) => {
      b.answering.add(FALCON);
      b.fppPlayers[FPP].status.warnings = [];
    });
    const health = region("Health");
    expect(await within(health).findByText("Answering")).toBeInTheDocument();
    expect(await within(health).findByText("No problems reported.")).toBeInTheDocument();
  });

  it("says when the FPP itself isn't answering", async () => {
    const { backend } = await openPage();
    delete backend.fppPlayers[FPP];
    await act(() => new Promise((r) => setTimeout(r, STATUS_POLL_MS + 200)));
    expect(within(region("Health")).getByText(`This FPP at ${FPP} isn't answering.`)).toBeInTheDocument();
    expect(within(pageHeader()).getByRole("status")).toHaveTextContent("Not answering");
    expect(within(region("Now playing")).queryByRole("button", { name: "Stop now" })).not.toBeInTheDocument();
  });

  it("lists its sequences, music, and playlists with length, size, and date", async () => {
    const { user } = await openPage();
    const library = region("On this FPP");
    const row = await within(library).findByRole("row", { name: /Christmas Medley 2017/ });
    expect(row).toHaveTextContent("9:27");
    expect(row).toHaveTextContent("66.5 MB");
    expect(row).toHaveTextContent("Nov 28, 2025");

    await user.click(within(library).getByRole("tab", { name: /Music/ }));
    const song = await within(library).findByRole("row", { name: /Christmas Medley 2017\.mp3/ });
    expect(song).toHaveTextContent("9:27");
    expect(song).toHaveTextContent("8.5 MB");
    expect(within(song).queryByRole("button", { name: /Play/ })).not.toBeInTheDocument();

    await user.click(within(library).getByRole("tab", { name: /Playlists/ }));
    const playlist = await within(library).findByRole("row", { name: /Christmas Show/ });
    expect(playlist).toHaveTextContent("1 item");
    expect(playlist).toHaveTextContent("9:27");
    expect(within(library).queryByRole("button", { name: /Delete/ })).not.toBeInTheDocument();
  });

  it("asks before Play stops a running show, and plays straight away when idle", async () => {
    const { backend, user } = await openPage();
    const library = region("On this FPP");
    await user.click(await within(library).findByRole("button", { name: "Play Christmas Medley 2017" }));
    const ask = await screen.findByRole("alertdialog", { name: "Stop the running show?" });
    expect(ask).toHaveTextContent("FPP is playing Christmas Medley 2017.fseq. Stop it and play Christmas Medley 2017 now?");
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(writes(backend)).toEqual([]);

    await user.click(within(library).getByRole("button", { name: "Play Christmas Medley 2017" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Stop and play" }));
    expect(writes(backend)).toEqual([`fppStart:${FPP}:Christmas Medley 2017.fseq`]);

    backend.fppPlayers[FPP].status.state = "idle";
    await user.click(within(library).getByRole("tab", { name: /Playlists/ }));
    await user.click(await within(library).findByRole("button", { name: "Play Christmas Show" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(writes(backend)).toContain(`fppStart:${FPP}:Christmas Show`);
  });

  it("sends one of the show's sequences with the Send to FPP dialog, then lists it", async () => {
    const { backend, user } = await openPage();
    const library = region("On this FPP");
    await within(library).findByRole("row", { name: /Christmas Medley 2017/ });
    await user.click(within(library).getByRole("button", { name: /^Send a sequence/ }));
    await user.click(screen.getByRole("menuitem", { name: "Wizards" }));
    const dialog = await screen.findByRole("dialog", { name: "Send to FPP" });
    expect(dialog).toHaveTextContent("Puts Wizards and its music on your FPP");
    expect(writes(backend)).toEqual([]);
    await within(dialog).findByText(/free\./);
    await user.click(within(dialog).getByRole("button", { name: /^Send/ }));
    expect(await within(dialog).findByText(/Wizards.fseq and Wizards.mp3 are on FPP/)).toBeInTheDocument();
    expect(backend.calls.some((c) => c.startsWith(`fppSend:${FPP}:Wizards.fseq`))).toBe(true);
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(await within(library).findByRole("row", { name: /Wizards/ })).toBeInTheDocument();
  });

  it("shows where it sends, and sets up the show from it as one undoable edit after showing what it adds", async () => {
    const { backend, user } = await openPage();
    const outputs = region("Outputs → your show");
    expect(await within(outputs).findByText("Falcon_F16V5_B9F5")).toBeInTheDocument();
    expect(within(outputs).getByText(/6,147 channels/)).toHaveTextContent("192.0.2.20 · 6,147 channels, 1–6,147");
    expect(within(outputs).getByText("This FPP has no light outputs of its own: it passes its sequence on to the controllers it sends to.")).toHaveClass("text-neutral-500");

    await user.click(within(outputs).getByRole("button", { name: "Set up my show from this FPP" }));
    const preview = await within(outputs).findByRole("group", { name: "What will be added" });
    expect(preview).toHaveTextContent("Falcon_F16V5_B9F5 · DDP to 192.0.2.20 · channels 1–6,147");
    expect(useApp.getState().snapshot!.show.controllers).toEqual([]);

    const edits = backend.calls.filter((c) => c === "applyEdits").length;
    await user.click(within(preview).getByRole("button", { name: "Add 1 controller" }));
    expect(await within(outputs).findByText("In your show")).toBeInTheDocument();
    expect(backend.calls.filter((c) => c === "applyEdits").length).toBe(edits + 1);
    expect(backend.calls).toContain(`fppSetUpShow:${FPP}:${FALCON}`);
    const controllers = useApp.getState().snapshot!.show.controllers;
    expect(controllers.map((c) => [c.name, c.address, c.protocol.type, c.sequenceChannels])).toEqual([
      ["Falcon_F16V5_B9F5", FALCON, "ddp", { start: 1, count: 6147 }],
    ]);
    expect(writes(backend)).toEqual([]);

    await act(() => useApp.getState().undo());
    expect(useApp.getState().snapshot!.show.controllers).toEqual([]);
    expect(within(outputs).getByRole("button", { name: "Set up my show from this FPP" })).toBeInTheDocument();
  });

  it("leaves out targets PixelFlow can't send to, and adds nothing if the FPP changed since the preview", async () => {
    const { backend, user } = await openPage((b) => {
      b.deviceNetwork.details[0].config.destinations.push({ ...b.deviceNetwork.details[0].config.destinations[0], address: "192.0.2.30", description: "Old Pixie", protocol: "Art-Net" });
    });
    const outputs = region("Outputs → your show");
    expect(await within(outputs).findByText("Not supported yet")).toBeInTheDocument();
    await user.click(within(outputs).getByRole("button", { name: "Set up my show from this FPP" }));
    const preview = await within(outputs).findByRole("group", { name: "What will be added" });
    expect(preview).toHaveTextContent("Left out: Old Pixie — PixelFlow can't send Art-Net yet.");

    backend.deviceNetwork.details[0].config.destinations[1].protocol = "DDP";
    await user.click(within(preview).getByRole("button", { name: "Add 1 controller" }));
    expect(await screen.findByText(/changed since you looked/)).toBeInTheDocument();
    expect(useApp.getState().snapshot!.show.controllers).toEqual([]);
  });

  it("lists the schedule, read only", async () => {
    await openPage();
    const schedule = region("Schedule");
    const entries = within(await within(schedule).findByRole("list", { name: "Schedule entries" })).getAllByRole("listitem");
    expect(entries.map((e) => e.textContent)).toEqual([
      "Christmas Medley 2017.fseqSequenceEvery day · Sunset + 15 min – 10:00 PM · Nov 27, 2026 – Jan 6, 2027 · Repeats, Stops gracefully",
      "Christmas ShowFri, Sat · 5:30 PM – 11:00 PM · Nov 27, 2026 – Jan 6, 2027 · Repeats, Stops gracefully",
      "HalloweenOffEvery day · 5:30 PM – 10:00 PM · Oct 1, 2026 – Oct 31, 2026 · Stops gracefully",
    ]);
    expect(within(schedule).queryAllByRole("button")).toEqual([]);
  });

  it("refresh reads everything again", async () => {
    const { backend, user } = await openPage();
    await within(region("Schedule")).findByRole("list", { name: "Schedule entries" });
    backend.fppSchedules[FPP] = [];
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await within(region("Schedule")).findByText("Nothing is scheduled on this FPP.")).toBeInTheDocument();
  });
});
