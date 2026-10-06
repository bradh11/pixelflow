import { describe, expect, it } from "vitest";
import { fppFileName } from "./fppNames";

// The same cases as pf-devices' `fpp_file_name`, so local files match what the shell sends.
describe("fppFileName", () => {
  it("keeps only what FPP keeps", () => {
    expect(fppFileName("Christmas Medley 2017", "fseq")).toBe("Christmas Medley 2017.fseq");
    expect(fppFileName("Rock'n \"Roll\"/Mix?", "fseq")).toBe("Rockn Roll Mix.fseq");
    expect(fppFileName("Café ✓", "fseq")).toBe("Caf.fseq");
    expect(fppFileName("..a..b..", "mp3")).toBe("a.b.mp3");
    expect(fppFileName("   ", "fseq")).toBe("Sequence.fseq");
    expect(fppFileName("Song", "MP3")).toBe("Song.mp3");
  });
});
