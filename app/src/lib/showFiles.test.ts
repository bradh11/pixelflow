import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import { fileName, shownPath } from "./format";
import { filesOf, folderOf, missingFile, repointEdits, resolveAudio, sameFile } from "./showFiles";

const ID = "11111111-0000-4000-8000-000000000001";

function show() {
  const s = emptyShow("Files");
  s.sequences = [{ id: ID, name: "Medley", path: "/Shows/Medley.fseq", audio: "/Shows/Medley.mp3", offsetMs: 0 }];
  s.background = { path: "/Shows/house.jpg", x: 0, y: 0, width: 10, opacity: 1 };
  return s;
}

describe("show files", () => {
  it("lists every file with what it belongs to, like the engine", () => {
    expect(filesOf(show()).map((f) => [f.file.kind, f.owner])).toEqual([
      ["sequence", "Sequence file for Medley"],
      ["music", "Music for Medley"],
      ["photo", "Background photo"],
    ]);
    expect(missingFile({ kind: "music", id: ID }, "C:\\Music\\Christmas Medley 2017.mp3", "Music for Medley").message).toBe(
      "Christmas Medley 2017.mp3 isn't where it was.",
    );
  });

  it("tells files apart by kind and sequence", () => {
    expect(sameFile({ kind: "photo" }, { kind: "photo" })).toBe(true);
    expect(sameFile({ kind: "music", id: ID }, { kind: "music", id: ID })).toBe(true);
    expect(sameFile({ kind: "music", id: ID }, { kind: "sequence", id: ID })).toBe(false);
    expect(sameFile({ kind: "music", id: ID }, { kind: "music", id: "other" })).toBe(false);
  });

  it("re-points several files of one sequence with one edit each", () => {
    const edits = repointEdits(show(), [
      { file: { kind: "sequence", id: ID }, to: "/New/Medley.fseq" },
      { file: { kind: "music", id: ID }, to: "/New/Medley.mp3" },
      { file: { kind: "photo" }, to: "/New/house.jpg" },
    ]);
    expect(edits).toEqual([
      { type: "updateSequence", sequence: { id: ID, name: "Medley", path: "/New/Medley.fseq", audio: "/New/Medley.mp3", offsetMs: 0 } },
      { type: "setBackground", background: { path: "/New/house.jpg", x: 0, y: 0, width: 10, opacity: 1 } },
    ]);
    expect(repointEdits(show(), [])).toEqual([]);
  });

  it("reads paths plainly, even ones with bytes that aren't UTF-8", () => {
    expect(fileName("/music/Caf\u0000e9.mp3")).toBe("Caf\uFFFD.mp3");
    expect(shownPath("/music/a\u0000ffb")).toBe("/music/a\uFFFDb");
    expect(folderOf("/Shows/Audio/Song.mp3")).toBe("/Shows/Audio");
    expect(folderOf("Song.mp3")).toBe("");
    expect(resolveAudio("Music/Song.mp3", "/Shows/Song.pfseq.json")).toBe("/Shows/Music/Song.mp3");
    expect(resolveAudio("/abs/Song.mp3", "/Shows/Song.pfseq.json")).toBe("/abs/Song.mp3");
  });
});
