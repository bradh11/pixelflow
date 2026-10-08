import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../../App";
import { demoDevices, demoFppFileDetails, demoFppFiles, demoFppSchedules, demoFppSoftware, demoPlayers } from "../../api/demo";
import { MemoryBackend, emptyShow } from "../../api/memory";
import { useApp } from "../../state/store";

const FPP = "192.0.2.10";
const SHOW = "/Shows/Home/Home.pixelflow.json";
const NOT_EDITABLE = /plays as it is, but its effects can't be edited/;

/** The demo network's FPP, with its files; `setup` runs before the app connects. */
async function openPage(setup?: (backend: MemoryBackend) => void) {
  const backend = new MemoryBackend(emptyShow("Home"));
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  backend.fppFiles = demoFppFiles();
  backend.fppFileDetails = demoFppFileDetails();
  backend.fppSchedules = demoFppSchedules();
  backend.fppSoftwares = demoFppSoftware();
  setup?.(backend);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<App />);
  act(() => useApp.getState().setScreen("devices"));
  await user.click(screen.getByRole("button", { name: "Scan network" }));
  await user.click(await screen.findByRole("button", { name: "Open FPP" }));
  const library = screen.getByRole("region", { name: "On this FPP" });
  await within(library).findByRole("row", { name: /Christmas Medley 2017/ });
  return { backend, user, library };
}

/** Anything that changes the FPP. */
const writes = (backend: MemoryBackend) => backend.calls.filter((c) => /^fpp(Start|Stop|Send):/.test(c));

async function openDownload(user: ReturnType<typeof userEvent.setup>, library: HTMLElement) {
  await user.click(within(library).getByRole("button", { name: "Download Christmas Medley 2017" }));
  return screen.findByRole("dialog", { name: "Download from FPP" });
}

describe("downloading a sequence from an FPP", () => {
  it("saves it and its music in the show's folder, then adds it to Play with that music", async () => {
    const { backend, user, library } = await openPage((b) => (b.path = SHOW));
    // Only sequences can be downloaded.
    expect(within(library).getAllByRole("button", { name: /^Download/ })).toHaveLength(1);
    const dialog = await openDownload(user, library);
    const saving = await within(dialog).findByRole("list", { name: "What will be saved" });
    expect(saving).toHaveTextContent("Christmas Medley 2017.fseq · 66.5 MB");
    expect(saving).toHaveTextContent("to /Shows/Home/sequences");
    expect(saving).toHaveTextContent("Christmas Medley 2017.mp3 · 8.5 MB");
    expect(saving).toHaveTextContent("to /Shows/Home/music");
    expect(dialog).toHaveTextContent(NOT_EDITABLE);
    expect(within(dialog).getByRole("button", { name: "Import xLights sequence…" })).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(await within(dialog).findByText("Saved Christmas Medley 2017.fseq and Christmas Medley 2017.mp3.")).toBeInTheDocument();
    expect(backend.calls).toContain(`fppDownload:${FPP}:Christmas Medley 2017.fseq:Christmas Medley 2017.mp3`);
    expect(dialog).toHaveTextContent(NOT_EDITABLE);

    await user.click(within(dialog).getByRole("button", { name: "Add to Play" }));
    expect(await within(dialog).findByText(/It's on the Play screen now\./)).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Add to Play" })).not.toBeInTheDocument();
    expect(useApp.getState().snapshot!.show.sequences.map((s) => [s.name, s.path, s.audio])).toEqual([
      ["Christmas Medley 2017", "/Shows/Home/sequences/Christmas Medley 2017.fseq", "/Shows/Home/music/Christmas Medley 2017.mp3"],
    ]);
    expect(writes(backend)).toEqual([]);
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("asks where to save it while the show isn't saved, with the shell's folder dialog", async () => {
    const { backend, user, library } = await openPage();
    const dialog = await openDownload(user, library);
    expect(dialog).toHaveTextContent("Your show isn't saved yet, so choose a folder");
    // Cancelling the folder dialog leaves this one as it was.
    await user.click(within(dialog).getByRole("button", { name: "Choose folder…" }));
    expect(backend.calls).toContain("pickDownloadFolder");
    expect(within(dialog).getByRole("button", { name: "Choose folder…" })).toBeInTheDocument();

    backend.nextDownloadFolder = "/Users/me/FPP copies";
    await user.click(within(dialog).getByRole("button", { name: "Choose folder…" }));
    const saving = await within(dialog).findByRole("list", { name: "What will be saved" });
    expect(saving).toHaveTextContent("to /Users/me/FPP copies/sequences");
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(await within(dialog).findByText(/^Saved Christmas Medley 2017\.fseq/)).toBeInTheDocument();
    expect(backend.localFiles.has("/Users/me/FPP copies/sequences/Christmas Medley 2017.fseq")).toBe(true);
  });

  it("shows progress, and Cancel stops it with nothing saved", async () => {
    const { backend, user, library } = await openPage((b) => {
      b.path = SHOW;
      b.fppDownloadStepMs = 40;
    });
    const dialog = await openDownload(user, library);
    await within(dialog).findByRole("list", { name: "What will be saved" });
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    const bar = await within(dialog).findByRole("progressbar", { name: "Downloading from the FPP" });
    expect(await within(dialog).findByText("Downloading the sequence…")).toBeInTheDocument();
    expect(await within(dialog).findByText("25% · 5.7 MB of 22.9 MB")).toBeInTheDocument();
    expect(bar).toHaveAttribute("value", "25");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("The download was cancelled. Nothing was saved.");
    expect(backend.calls).toContain("cancelFppDownload");
    expect(backend.localFiles.size).toBe(0);
    // Ready to try again.
    expect(await within(dialog).findByRole("list", { name: "What will be saved" })).toBeInTheDocument();
  });

  it("asks Replace or Keep both when the folder already has that file, and Cancel leaves it alone", async () => {
    const existing = "/Shows/Home/sequences/Christmas Medley 2017.fseq";
    const { backend, user, library } = await openPage((b) => {
      b.path = SHOW;
      b.localFiles.add(existing);
    });
    let dialog = await openDownload(user, library);
    let clash = await within(dialog).findByRole("radiogroup", { name: "There's already a sequence called Christmas Medley 2017.fseq in sequences." });
    expect(within(dialog).getByRole("button", { name: "Download" })).toBeDisabled();
    expect(within(clash).getByRole("radio", { name: "Replace it" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.calls.some((c) => c.startsWith("fppDownload:"))).toBe(false);

    dialog = await openDownload(user, library);
    clash = await within(dialog).findByRole("radiogroup", { name: /already a sequence called/ });
    await user.click(within(clash).getByRole("radio", { name: "Keep both: save this one as Christmas Medley 2017 (2).fseq" }));
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(await within(dialog).findByText(/^Saved Christmas Medley 2017 \(2\)\.fseq/)).toBeInTheDocument();
    expect(backend.localFiles.has("/Shows/Home/sequences/Christmas Medley 2017 (2).fseq")).toBe(true);
  });

  it("warns plainly when the sequence's channels don't match the show's, and still saves it", async () => {
    const { user, library } = await openPage((b) => {
      b.path = SHOW;
      b.fppPlayers[FPP].sequences[0].channels = 6147;
      const show = emptyShow("Home");
      show.controllers = [
        {
          id: "c1",
          name: "Falcon",
          address: "192.0.2.20",
          adapter: "falcon",
          protocol: { type: "ddp" },
          ports: [],
          sequenceChannels: { start: 1, count: 1462 },
        },
      ];
      b.show = show;
    });
    const dialog = await openDownload(user, library);
    expect(
      await within(dialog).findByText("This sequence uses 6,147 channels; your show has 1,462. Its preview won't light the right props until your layout matches."),
    ).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(await within(dialog).findByText(/^Saved Christmas Medley 2017\.fseq/)).toBeInTheDocument();
  });

  it("says when the FPP doesn't have the music the sequence names", async () => {
    const { user, library } = await openPage((b) => {
      b.path = SHOW;
      b.fppSequenceMedia[FPP] = { "Christmas Medley 2017.fseq": "/Volumes/Show/Audio/Medley Remix.ogg" };
    });
    const dialog = await openDownload(user, library);
    expect(await within(dialog).findByText(/No music: the sequence names Medley Remix\.ogg, which isn't on FPP\./)).toBeInTheDocument();
    expect(within(dialog).getByRole("list", { name: "What will be saved" })).not.toHaveTextContent(".mp3");
  });
});
