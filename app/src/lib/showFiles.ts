import type { Edit, FileRole, MissingFile, Show } from "../api/types";
import { fileName } from "./format";

/** True when `a` and `b` are the same file of the show. */
export function sameFile(a: FileRole, b: FileRole): boolean {
  if (a.kind !== b.kind) return false;
  return "id" in a && "id" in b ? a.id === b.id : true;
}

/** Every file the show refers to, with what it belongs to (in the engine's order). */
export function filesOf(show: Show): { file: FileRole; owner: string; path: string }[] {
  const files: { file: FileRole; owner: string; path: string }[] = [];
  for (const s of show.sequences) {
    files.push({ file: { kind: "sequence", id: s.id }, owner: `Sequence file for ${s.name}`, path: s.path });
    if (s.audio) files.push({ file: { kind: "music", id: s.id }, owner: `Music for ${s.name}`, path: s.audio });
  }
  if (show.background) files.push({ file: { kind: "photo" }, owner: "Background photo", path: show.background.path });
  if (show.houseModel) files.push({ file: { kind: "houseModel" }, owner: "House model", path: show.houseModel.path });
  return files;
}

/** A missing file, described like the engine describes it. */
export function missingFile(file: FileRole, path: string, owner: string, wasAt = path): MissingFile {
  const name = fileName(path);
  return { file, name, path, wasAt, owner, message: `${name} isn't where it was.` };
}

/** The edits that point `file` at `to` (one undo step when applied together). */
export function repointEdits(show: Show, changes: { file: FileRole; to: string }[]): Edit[] {
  const sequences = new Map(show.sequences.map((s) => [s.id, { ...s }]));
  let background = show.background ?? null;
  let houseModel = show.houseModel ?? null;
  for (const { file, to } of changes) {
    if (file.kind === "sequence" || file.kind === "music") {
      const entry = sequences.get(file.id);
      if (!entry) throw new Error("There is no sequence with that id.");
      if (file.kind === "sequence") entry.path = to;
      else entry.audio = to;
    } else if (file.kind === "photo" && background) background = { ...background, path: to };
    else if (file.kind === "houseModel" && houseModel) houseModel = { ...houseModel, path: to };
  }
  const edits: Edit[] = show.sequences
    .filter((s) => {
      const after = sequences.get(s.id)!;
      return after.path !== s.path || after.audio !== s.audio;
    })
    .map((s) => ({ type: "updateSequence", sequence: sequences.get(s.id)! }));
  if (background !== (show.background ?? null)) edits.push({ type: "setBackground", background });
  if (houseModel !== (show.houseModel ?? null)) edits.push({ type: "setHouseModel", houseModel });
  return edits;
}

/** The folder part of a path, for saying where a file was found. */
export function folderOf(path: string): string {
  const cut = path.replace(/[\\/][^\\/]*$/, "");
  return cut === path ? "" : cut;
}

/** Music paths relative to the sequence file are found next to it. */
export function resolveAudio(audio: string | null, docPath: string | null): string | null {
  if (!audio) return null;
  if (/^([a-zA-Z]:[\\/]|[\\/])/.test(audio) || !docPath) return audio;
  return `${folderOf(docPath)}/${audio}`;
}
