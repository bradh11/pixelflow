import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { demoDevices, demoPlayers } from "../api/demo";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { SendSource } from "../api/types";
import { newController } from "../lib/shows";
import { useApp } from "../state/store";
import { SendToFppDialog, fppChoices } from "./SendToFppDialog";

const FPP = "192.0.2.10";
const SOURCE: SendSource = { kind: "openSequence", name: "Jingle Bells" };

async function open(setup?: (backend: MemoryBackend) => void, props: Partial<Parameters<typeof SendToFppDialog>[0]> = {}) {
  const show = emptyShow("Home");
  show.controllers = [{ ...newController("Main FPP", FPP, "ddp", 0), adapter: "fpp" }];
  const backend = new MemoryBackend(show);
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  backend.fppFiles[FPP] = { media: [], playlists: { "Christmas Show": [] }, freeBytes: 5 * 1024 ** 3 };
  setup?.(backend);
  await useApp.getState().connect(backend);
  const onClose = vi.fn();
  const onSent = vi.fn();
  const user = userEvent.setup();
  render(<SendToFppDialog source={SOURCE} title="Jingle Bells" music="/music/Jingle Bells.mp3" onClose={onClose} onSent={onSent} {...props} />);
  const dialog = screen.getByRole("dialog", { name: "Send to FPP" });
  await within(dialog).findByText(/5\.0 GB free/);
  return { backend, user, dialog, onClose, onSent };
}

describe("Send to FPP", () => {
  it("lists the show's FPPs and the ones found, each once", () => {
    const show = emptyShow("Home");
    show.controllers = [
      { ...newController("Main FPP", FPP, "ddp", 0), adapter: "fpp" },
      { ...newController("Falcon", "192.0.2.20", "ddp", 0), adapter: "falcon" },
    ];
    const devices = demoDevices().details.map((d) => ({ ...d.device, responding: true, lastSeen: 0 }));
    expect(fppChoices(show.controllers, devices)).toEqual([{ address: FPP, name: "Main FPP" }]);
    expect(fppChoices(undefined, [{ ...devices[0], address: "192.0.2.99", name: "Garage" }])).toEqual([{ address: "192.0.2.99", name: "Garage" }]);
  });

  it("checks the FPP without changing anything, then sends only on Send", async () => {
    const { backend, user, dialog, onSent } = await open();
    expect(within(dialog).getByRole("combobox", { name: "FPP" })).toHaveValue(FPP);
    expect(within(dialog).getByText("Jingle Bells.mp3")).toBeInTheDocument();
    expect(backend.calls.filter((c) => c.startsWith("fppSend") || c.startsWith("fppStart"))).toEqual([]);

    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    expect(await within(dialog).findByText(/Jingle Bells.fseq and Jingle Bells.mp3 are on Main FPP/)).toBeInTheDocument();
    expect(backend.calls).toContain(`fppSend:${FPP}:Jingle Bells.fseq:none`);
    expect(onSent).toHaveBeenCalledWith(FPP, expect.objectContaining({ sequenceName: "Jingle Bells.fseq", playName: "Jingle Bells.fseq" }));
    // Sending never plays anything: that's its own click.
    expect(backend.calls.some((c) => c.startsWith("fppStart"))).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Play it now on the FPP" }));
    expect(await within(dialog).findByText("Playing on Main FPP.")).toBeInTheDocument();
    expect(backend.calls).toContain(`fppStart:${FPP}:Jingle Bells.fseq`);
  });

  it("asks what to do about a name that's taken, and keeps both when asked", async () => {
    const { backend, user, dialog } = await open((b) => {
      b.fppPlayers[FPP].sequences.push({ name: "Jingle Bells", frames: 1, stepMs: 50, channels: 3 });
    });
    const clash = within(dialog).getByRole("group", { name: /already has a sequence called Jingle Bells.fseq/ });
    expect(within(clash).getByRole("radio", { name: "Replace it" })).toBeChecked();
    await user.click(within(clash).getByRole("radio", { name: /Keep both: send this one as Jingle Bells \(2\)\.fseq/ }));
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" });
    expect(backend.calls).toContain(`fppSend:${FPP}:Jingle Bells (2).fseq:none`);
  });

  it("puts it on a new playlist and plays that playlist", async () => {
    const { backend, user, dialog } = await open();
    await user.click(within(dialog).getByRole("radio", { name: /A new playlist called/ }));
    expect(within(dialog).getByRole("textbox", { name: "New playlist name" })).toHaveValue("Jingle Bells");
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    expect(await within(dialog).findByText(/on the playlist “Jingle Bells”/)).toBeInTheDocument();
    expect(backend.fppFiles[FPP].playlists["Jingle Bells"]).toEqual(["Jingle Bells.fseq"]);
    await user.click(within(dialog).getByRole("button", { name: "Play it now on the FPP" }));
    await waitFor(() => expect(backend.calls).toContain(`fppStart:${FPP}:Jingle Bells`));
  });

  it("adds it to an existing playlist", async () => {
    const { backend, user, dialog } = await open();
    await user.click(within(dialog).getByRole("radio", { name: /Add it to/ }));
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByText(/on the playlist “Christmas Show”/);
    expect(backend.fppFiles[FPP].playlists["Christmas Show"]).toEqual(["Jingle Bells.fseq"]);
  });

  it("shows progress, and Cancel stops the send", async () => {
    const { backend, user, dialog } = await open((b) => {
      b.fppSendStepMs = 30;
    });
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    expect(await within(dialog).findByRole("progressbar", { name: "Sending to the FPP" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(await within(dialog).findByText(/Sending was stopped/)).toBeInTheDocument();
    expect(backend.calls).toContain("cancelFppSend");
    expect(backend.fppPlayers[FPP].sequences.map((s) => s.name)).not.toContain("Jingle Bells");
  });

  it("says plainly when sending fails, and offers to try again", async () => {
    const { user, dialog } = await open((b) => {
      b.fppSendError = "The FPP's storage is full, so Jingle Bells.fseq couldn't be stored.";
    });
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("storage is full");
    await user.click(within(dialog).getByRole("button", { name: /Try again/ }));
    expect(await within(dialog).findByRole("button", { name: "Play it now on the FPP" })).toBeInTheDocument();
  });

  it("can send without music, and can't send music FPP can't play", async () => {
    const { backend, user, dialog } = await open();
    await user.click(within(dialog).getByRole("button", { name: "No music" }));
    expect(await within(dialog).findByText("No music (lights only)")).toBeInTheDocument();
    backend.nextAudioPath = "/music/notes.txt";
    await user.click(within(dialog).getByRole("button", { name: "Choose music…" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("can't play notes.txt");
    expect(within(dialog).getByRole("button", { name: /^Send$/ })).toBeDisabled();
  });

  it("says when the FPP can't be reached", async () => {
    const show = emptyShow("Home");
    const backend = new MemoryBackend(show);
    await useApp.getState().connect(backend);
    const user = userEvent.setup();
    render(<SendToFppDialog source={SOURCE} title="Jingle Bells" music={null} onClose={() => {}} />);
    const dialog = screen.getByRole("dialog", { name: "Send to FPP" });
    expect(within(dialog).getByText(/Type its address/)).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox", { name: "FPP address" }), "192.0.2.99");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Could not reach 192.0.2.99");
  });

  it("closes with Escape", async () => {
    const { user, onClose } = await open();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });
});
