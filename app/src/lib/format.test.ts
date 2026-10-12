import { describe, expect, it } from "vitest";
import { sequenceTitle } from "./format";

describe("sequenceTitle", () => {
  it("names a sequence by its file, without the file type", () => {
    expect(sequenceTitle("/Shows/Chimes Medley 2020.pfseq.json")).toBe("Chimes Medley 2020");
    expect(sequenceTitle("C:\\Shows\\Wizards.fseq")).toBe("Wizards");
    expect(sequenceTitle("/Shows/notes.json")).toBe("notes.json");
  });
});
