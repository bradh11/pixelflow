import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { demoDevices, demoPlayers } from "../api/demo";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { FppSendRequest, SendSource } from "../api/types";
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
  // Idle, so Play it now doesn't have to ask.
  backend.fppPlayers[FPP].status = { ...backend.fppPlayers[FPP].status, state: "idle", playlist: null, sequence: null, nextPlaylist: null, nextStart: null };
  backend.fppFiles[FPP] = { media: [], playlists: { "Christmas Show": [] }, freeBytes: 5 * 1024 ** 3 };
  setup?.(backend);
  await useApp.getState().connect(backend);
  const onClose = vi.fn();
  const onSent = vi.fn();
  const user = userEvent.setup();
  const { unmount } = render(<SendToFppDialog source={SOURCE} title="Jingle Bells" music="/music/Jingle Bells.mp3" onClose={onClose} onSent={onSent} {...props} />);
  const dialog = screen.getByRole("dialog", { name: "Send to FPP" });
  await within(dialog).findByText(/free\.|free space/);
  return { backend, user, dialog, onClose, onSent, unmount };
}

/** The request the last send made. */
function lastRequest(spy: { mock: { calls: unknown[][] } }): FppSendRequest {
  return spy.mock.calls.at(-1)![1] as FppSendRequest;
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

    const send = vi.spyOn(backend, "fppSend");
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    expect(await within(dialog).findByText(/Jingle Bells.fseq and Jingle Bells.mp3 are on Main FPP/)).toBeInTheDocument();
    expect(lastRequest(send)).toMatchObject({ replaceSequence: false, replaceMusic: false, playlist: { kind: "none" } });
    expect(onSent).toHaveBeenCalledWith(FPP, expect.objectContaining({ sequenceName: "Jingle Bells.fseq", playName: "Jingle Bells.fseq" }));
    // Sending never plays anything: that's its own click.
    expect(backend.calls.some((c) => c.startsWith("fppStart"))).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Play it now on the FPP" }));
    expect(await within(dialog).findByText("Playing on Main FPP.")).toBeInTheDocument();
    expect(backend.calls).toContain(`fppStart:${FPP}:Jingle Bells.fseq`);
  });

  it("chooses nothing for a name that's taken: Send waits until the user does", async () => {
    const { backend, user, dialog } = await open((b) => {
      b.fppPlayers[FPP].sequences.push({ name: "Jingle Bells", frames: 1, stepMs: 50, channels: 3 });
      b.fppFiles[FPP].media.push("Jingle Bells.mp3");
    });
    const sequence = within(dialog).getByRole("radiogroup", { name: /already has a sequence called Jingle Bells.fseq/ });
    const music = within(dialog).getByRole("radiogroup", { name: /already has music called Jingle Bells.mp3/ });
    for (const radio of [...within(sequence).getAllByRole("radio"), ...within(music).getAllByRole("radio")]) expect(radio).not.toBeChecked();
    const sendButton = within(dialog).getByRole("button", { name: /^Send$/ });
    expect(sendButton).toBeDisabled();
    expect(within(music).getByText(/Anything on the FPP that uses Jingle Bells.mp3 will play this file instead/)).toBeInTheDocument();

    const send = vi.spyOn(backend, "fppSend");
    await user.click(within(sequence).getByRole("radio", { name: /Keep both: send this one as Jingle Bells \(2\)\.fseq/ }));
    expect(sendButton).toBeDisabled();
    await user.click(within(music).getByRole("radio", { name: "Replace it" }));
    expect(sendButton).toBeEnabled();
    await user.click(sendButton);
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" });
    expect(lastRequest(send)).toMatchObject({
      sequenceName: "Jingle Bells (2).fseq",
      replaceSequence: false,
      musicName: "Jingle Bells.mp3",
      replaceMusic: true,
      uploadMusic: true,
    });
  });

  it("replaces or reuses the FPP's file by its own spelling", async () => {
    const { backend, user, dialog } = await open((b) => {
      b.fppFiles[FPP].media.push("jingle bells.mp3");
    });
    const music = within(dialog).getByRole("radiogroup", { name: /already has music called jingle bells.mp3/ });
    expect(within(music).getByText(/different capitals/)).toBeInTheDocument();
    const send = vi.spyOn(backend, "fppSend");
    await user.click(within(music).getByRole("radio", { name: "Use the one already on the FPP" }));
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" });
    expect(lastRequest(send)).toMatchObject({ musicName: "jingle bells.mp3", uploadMusic: false });
  });

  it("doesn't pick an FPP playlist for the user", async () => {
    const { backend, user, dialog } = await open();
    expect(within(dialog).getByRole("radio", { name: "Don't add it to a playlist" })).toBeChecked();
    const pick = within(dialog).getByRole("combobox", { name: "Playlist" });
    expect(pick).toHaveValue("");
    await user.click(within(dialog).getByRole("radio", { name: /Add it to/ }));
    expect(within(dialog).getByRole("button", { name: /^Send$/ })).toBeDisabled();
    await user.selectOptions(pick, "Christmas Show");
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByText(/on the playlist “Christmas Show”/);
    expect(backend.fppFiles[FPP].playlists["Christmas Show"]).toEqual(["Jingle Bells.fseq"]);
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

  it("won't make a new playlist with a name the FPP already uses", async () => {
    const { user, dialog } = await open();
    await user.click(within(dialog).getByRole("radio", { name: /A new playlist called/ }));
    const name = within(dialog).getByRole("textbox", { name: "New playlist name" });
    await user.clear(name);
    await user.type(name, "christmas show");
    expect(within(dialog).getByText(/already has a playlist called Christmas Show/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /^Send$/ })).toBeDisabled();
  });

  it("shows progress; Cancel stops the send until the files start moving into place", async () => {
    const { backend, user, dialog } = await open((b) => {
      b.fppSendStepMs = 30;
    });
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    expect(await within(dialog).findByRole("progressbar", { name: "Sending to the FPP" })).toBeInTheDocument();
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    await waitFor(() => expect(cancel).toHaveFocus());
    await user.click(cancel);
    expect(await within(dialog).findByText(/Nothing on the FPP was changed/)).toBeInTheDocument();
    expect(backend.calls).toContain("cancelFppSend");
    expect(backend.fppPlayers[FPP].sequences.map((s) => s.name)).not.toContain("Jingle Bells");
  });

  it("can't be cancelled once files are moving into place", async () => {
    const { user, dialog } = await open((b) => {
      b.fppSendStepMs = 40;
    });
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    expect(await within(dialog).findByText("Finishing on the FPP…", {}, { timeout: 3000 })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" }, { timeout: 3000 });
  });

  it("Escape while sending cancels it", async () => {
    const { backend, user, dialog, onClose } = await open((b) => {
      b.fppSendStepMs = 30;
    });
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("progressbar", { name: "Sending to the FPP" });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(backend.calls).toContain("cancelFppSend"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("says plainly when sending fails, with FPP's own words under Details", async () => {
    const { user, dialog } = await open((b) => {
      b.fppSendError =
        "The FPP's storage is full, so Jingle Bells.fseq couldn't be stored. Nothing on the FPP was replaced.\n\nFPP said: Only 0 of 65536 bytes written, possibly out of free disk space";
    });
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    const alert = await within(dialog).findByRole("alert");
    expect(alert).toHaveTextContent("The FPP's storage is full");
    const details = within(alert).getByText("Details");
    expect(details.closest("details")).not.toHaveAttribute("open");
    expect(within(alert).getByText(/possibly out of free disk space/)).toBeInTheDocument();
    // The FPP was read again, ready to try again.
    await user.click(within(dialog).getByRole("button", { name: /Try again/ }));
    expect(await within(dialog).findByRole("button", { name: "Play it now on the FPP" })).toBeInTheDocument();
  });

  it("asks before Play it now stops what the FPP is playing", async () => {
    const { backend, user, dialog } = await open();
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" });
    backend.fppPlayers[FPP].status = { ...backend.fppPlayers[FPP].status, state: "playing", sequence: "Wizards.fseq", playlist: "Christmas Show" };
    await user.click(within(dialog).getByRole("button", { name: "Play it now on the FPP" }));
    expect(await within(dialog).findByText("Stop Christmas Show and play this now?")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.startsWith("fppStart"))).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Stop it and play this" }));
    await waitFor(() => expect(backend.calls).toContain(`fppStart:${FPP}:Jingle Bells.fseq`));
  });

  it("asks before playing when the FPP has a show scheduled", async () => {
    const { backend, user, dialog } = await open();
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" });
    backend.fppPlayers[FPP].status = { ...backend.fppPlayers[FPP].status, nextPlaylist: "Christmas Show", nextStart: "Mon Oct 5 @ 06:48 PM" };
    await user.click(within(dialog).getByRole("button", { name: "Play it now on the FPP" }));
    expect(await within(dialog).findByText(/Christmas Show is scheduled \(Mon Oct 5 @ 06:48 PM\)\. Play this now anyway\?/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Not now" }));
    expect(backend.calls.some((c) => c.startsWith("fppStart"))).toBe(false);
  });

  it("warns about a channel layout that doesn't match, and still sends", async () => {
    const { user, dialog } = await open((b) => {
      b.fppFiles[FPP].layoutWarnings = ["This sequence has 5,000 channels but the FPP sends 6,147. Lights past channel 5,000 will stay dark."];
    });
    expect(within(dialog).getByText(/Lights past channel 5,000 will stay dark/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("button", { name: "Play it now on the FPP" });
  });

  it("says when the FPP's free space isn't known", async () => {
    const { dialog } = await open((b) => {
      b.fppFiles[FPP].freeBytes = null;
    });
    expect(within(dialog).getByText("Couldn't read the FPP's free space.")).toBeInTheDocument();
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

  it("checks a typed address only when asked, not while typing", async () => {
    const show = emptyShow("Home");
    const backend = new MemoryBackend(show);
    await useApp.getState().connect(backend);
    const plan = vi.spyOn(backend, "fppSendPlan");
    const user = userEvent.setup();
    render(<SendToFppDialog source={SOURCE} title="Jingle Bells" music={null} onClose={() => {}} />);
    const dialog = screen.getByRole("dialog", { name: "Send to FPP" });
    expect(within(dialog).getByText(/Type its address/)).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox", { name: "FPP address" }), "192.0.2.99");
    await new Promise((r) => setTimeout(r, 600));
    expect(plan).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Could not reach 192.0.2.99");
    expect(plan).toHaveBeenCalledTimes(1);
  });

  it("closes with Escape", async () => {
    const { user, onClose } = await open();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("cancels a send in progress if it goes away", async () => {
    const { backend, user, dialog, unmount } = await open((b) => {
      b.fppSendStepMs = 30;
    });
    await user.click(within(dialog).getByRole("button", { name: /^Send$/ }));
    await within(dialog).findByRole("progressbar", { name: "Sending to the FPP" });
    unmount();
    await waitFor(() => expect(backend.calls).toContain("cancelFppSend"));
  });
});
