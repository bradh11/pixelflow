import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { MemoryBackend, emptyShow } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import type { Show } from "../api/types";
import { newController } from "../lib/shows";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

const FSEQ = "/Shows/Haas 2024/Christmas Medley 2017.fseq";
const SONG = "/Shows/Haas 2024/MP3 Music/Christmas Medley 2017.mp3";
const SONG_NOW = "/Shows/Haas 2024/Audio/Christmas Medley 2017.mp3";
const PHOTO = "/Shows/Haas 2024/photos/house.jpg";
const SHOW_FILE = "/Shows/Haas 2024/show.pixelflow.json";

function showWithFiles(): Show {
  const show = emptyShow("Haas 2024");
  show.controllers = [{ ...newController("Falcon", "192.0.2.20", "ddp", 0), sequenceChannels: { start: 1, count: 600 } }];
  show.sequences = [{ id: "11111111-0000-4000-8000-000000000001", name: "Medley", path: FSEQ, audio: SONG, offsetMs: 0 }];
  show.background = { path: PHOTO, x: -10, y: 10, width: 20, opacity: 1 };
  return show;
}

/** The show opened from its file, with the music and photo moved away (only the music findable). */
async function openMoved(saved = true) {
  const backend = new MemoryBackend(showWithFiles());
  if (saved) await backend.saveShowAs(SHOW_FILE);
  backend.missingPaths = new Set([SONG, PHOTO]);
  backend.findable.set(SONG, SONG_NOW);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<App />);
  return { backend, user };
}

describe("missing files", () => {
  it("offers to find every missing file when the show opens, and finding them is one undo step", async () => {
    const { backend, user } = await openMoved();
    const banner = screen.getByRole("region", { name: "Missing files" });
    expect(banner).toHaveTextContent("2 files aren't where they were: Christmas Medley 2017.mp3, house.jpg.");
    await user.click(within(banner).getByRole("button", { name: /Find all missing files/ }));
    expect(backend.calls).toContain("findMissingFiles");

    const report = await screen.findByRole("dialog", { name: "Found 1 file" });
    expect(report).toHaveTextContent("Christmas Medley 2017.mp3 is now in /Shows/Haas 2024/Audio");
    expect(report).toHaveTextContent("Undo puts the old places back");
    expect(within(report).getByText("Still missing")).toBeInTheDocument();
    expect(within(report).getByText("house.jpg")).toBeInTheDocument();
    expect(useApp.getState().snapshot?.show.sequences[0].audio).toBe(SONG_NOW);
    expect(screen.getByRole("region", { name: "Missing files" })).toHaveTextContent(
      "house.jpg isn't where it was. PixelFlow can look for it in the show's folder.",
    );

    // Locate the photo from the report: it comes off the list.
    backend.nextLocatePath = "/Shows/Haas 2024/house.jpg";
    await user.click(within(report).getByRole("button", { name: "Locate house.jpg" }));
    expect(backend.calls).toContain("locateFile:photo");
    await waitFor(() => expect(within(report).queryByText("Still missing")).not.toBeInTheDocument());
    await user.click(within(report).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("region", { name: "Missing files" })).not.toBeInTheDocument();
    expect(useApp.getState().snapshot?.show.background?.path).toBe("/Shows/Haas 2024/house.jpg");

    // Each change was one undo step: undoing the locate, then the find, brings back both.
    await useApp.getState().undo();
    await useApp.getState().undo();
    expect(useApp.getState().snapshot?.missingFiles).toHaveLength(2);
    expect(useApp.getState().snapshot?.show.sequences[0].audio).toBe(SONG);
  });

  it("can be put away until another show opens", async () => {
    const { user } = await openMoved();
    const banner = screen.getByRole("region", { name: "Missing files" });
    await user.click(within(banner).getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("region", { name: "Missing files" })).not.toBeInTheDocument();
    // Still listed with the show's problems.
    await user.click(screen.getByRole("button", { name: /0 errors, 2 warnings/ }));
    const problems = screen.getByRole("dialog", { name: "Problems" });
    expect(within(problems).getByText("2 files aren't where they were")).toBeInTheDocument();
    expect(within(problems).getByRole("group", { name: "house.jpg isn't where it was." })).toHaveTextContent("Background photo");
  });

  it("finds or locates one file at a time from the problems list", async () => {
    const { backend, user } = await openMoved();
    await user.click(screen.getByRole("button", { name: /2 warnings/ }));
    const problems = screen.getByRole("dialog", { name: "Problems" });
    await user.click(within(problems).getByRole("button", { name: "Find house.jpg again" }));
    expect(backend.calls).toContain("findMissingFiles:photo");
    expect(await screen.findByRole("dialog", { name: "No files found" })).toHaveTextContent(
      "PixelFlow looked in the show's folder and the folders inside it, but couldn't find:",
    );
    expect(useApp.getState().snapshot?.show.sequences[0].audio).toBe(SONG);
    await user.click(screen.getByRole("button", { name: "Done" }));

    // Cancelling Locate… changes nothing.
    await user.click(within(problems).getByRole("button", { name: "Locate house.jpg" }));
    expect(useApp.getState().snapshot?.missingFiles).toHaveLength(2);
    // A file that isn't there either is refused plainly.
    backend.nextLocatePath = SONG;
    await user.click(within(problems).getByRole("button", { name: "Locate house.jpg" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Christmas Medley 2017.mp3 isn't there anymore. Choose another file.");
  });

  it("asks for the show to be saved before searching its folder", async () => {
    const { user } = await openMoved(false);
    const banner = screen.getByRole("region", { name: "Missing files" });
    expect(banner).toHaveTextContent("Save the show, or locate them one by one.");
    expect(within(banner).queryByRole("button", { name: /Find all/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /2 warnings/ }));
    await user.click(within(screen.getByRole("dialog", { name: "Problems" })).getByRole("button", { name: "Find house.jpg again" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Save the show first, so PixelFlow knows which folder to look in.");
  });

  it("shows a missing sequence or song on the Play screen", async () => {
    const { backend, user } = await openMoved();
    backend.missingPaths.add(FSEQ);
    await useApp.getState().connect(backend);
    await user.click(screen.getByRole("button", { name: "Play" }));
    const list = screen.getByRole("complementary", { name: "Sequences" });
    expect(within(list).getByLabelText("A file isn't where it was")).toBeInTheDocument();
    const transport = screen.getByRole("region", { name: "Transport" });
    expect(within(transport).getByRole("group", { name: "Christmas Medley 2017.fseq isn't where it was." })).toBeInTheDocument();
    const song = within(transport).getByRole("group", { name: "Christmas Medley 2017.mp3 isn't where it was." });
    expect(song).toHaveTextContent("It was in /Shows/Haas 2024/MP3 Music.");
    await user.click(within(song).getByRole("button", { name: "Find Christmas Medley 2017.mp3 again" }));
    await screen.findByRole("dialog", { name: "Found 1 file" });
    expect(within(transport).queryByRole("group", { name: /mp3 isn't where it was/ })).not.toBeInTheDocument();
  });

  it("shows a missing photo in the layout's photo panel", async () => {
    const { user } = await openMoved();
    await user.click(await screen.findByRole("button", { name: /^Photo/ }));
    const panel = await screen.findByRole("group", { name: "house.jpg isn't where it was." });
    expect(panel).toHaveTextContent("It was in /Shows/Haas 2024/photos.");
    expect(within(panel).getByRole("button", { name: "Locate house.jpg" })).toBeInTheDocument();
  });

  it("finds a sequence's missing music again on the Sequence screen", async () => {
    const { backend, user } = await openMoved();
    const seq = new MemorySequencer(backend);
    seq.files.set("/Seq/Carol.pfseq.json", { schemaVersion: 2, name: "Carol", audio: "Music/Carol.mp3", durationMs: 60_000, frameMs: 25, timingTracks: [], rows: [] });
    backend.missingPaths.add("/Seq/Music/Carol.mp3");
    await seq.openSequenceDoc("/Seq/Carol.pfseq.json");
    await useSequencer.getState().connect(seq);
    await user.click(screen.getByRole("button", { name: "Sequence" }));
    const missing = await screen.findByRole("group", { name: "Carol.mp3 isn't where it was." });
    await user.click(within(missing).getByRole("button", { name: "Find Carol.mp3 again" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "PixelFlow couldn't find Carol.mp3 in the sequence's or the show's folder. Use Locate… to choose it.",
    );
    backend.findable.set("/Seq/Music/Carol.mp3", "/Seq/Audio/Carol.mp3");
    await user.click(within(missing).getByRole("button", { name: "Find Carol.mp3 again" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Found Carol.mp3 in /Seq/Audio. Undo puts the old place back."));
    await waitFor(() => expect(screen.queryByRole("group", { name: /Carol.mp3 isn't/ })).not.toBeInTheDocument());
    expect(useSequencer.getState().doc?.audio).toBe("/Seq/Audio/Carol.mp3");
    expect(useSequencer.getState().canUndo).toBe(true);
  });

  it("asks for the files to be checked when the show hasn't looked at them yet, and on focus", async () => {
    const backend = new MemoryBackend(showWithFiles());
    await backend.saveShowAs(SHOW_FILE);
    backend.missingPaths = new Set([SONG]);
    backend.filesChecked = false;
    await useApp.getState().connect(backend);
    await waitFor(() => expect(backend.calls).toContain("checkFiles"));
    await waitFor(() => expect(useApp.getState().snapshot?.missingFiles).toHaveLength(1));
    useApp.setState({ started: true });
    render(<App />);
    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(backend.calls).toContain("checkFiles:all"));
  });

  it("says where a file really was, what else fits, and when the search gave up", async () => {
    const { backend, user } = await openMoved();
    backend.wasAt.set(SONG, "/Old Disk/Haas 2024/MP3 Music/Christmas Medley 2017.mp3");
    backend.alsoFound.set(SONG, ["/Shows/Haas 2024/Backup/Christmas Medley 2017.mp3"]);
    backend.searchGivesUp = true;
    await useApp.getState().connect(backend);
    await user.click(screen.getByRole("button", { name: /2 warnings/ }));
    const problems = screen.getByRole("dialog", { name: "Problems" });
    expect(within(problems).getByRole("group", { name: "Christmas Medley 2017.mp3 isn't where it was." })).toHaveTextContent(
      "It was in /Old Disk/Haas 2024/MP3 Music.",
    );
    await user.click(within(problems).getByRole("button", { name: "Find all missing files" }));
    const report = await screen.findByRole("dialog", { name: "Found 1 file" });
    expect(report).toHaveTextContent("Also found in /Shows/Haas 2024/Backup. If that's the right one, use Locate… to choose it.");
    expect(report).toHaveTextContent("PixelFlow stopped looking before it had checked every folder.");
  });
});
