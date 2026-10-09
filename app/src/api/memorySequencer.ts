// An in-memory stand-in for the sequencer side of the engine, for tests and the plain-browser demo.
// It applies sequence edits with undo/redo and gesture merging like the engine, answers with the
// same light SequenceEditResult replies, and checks effect settings against the engine's catalog
// (effectCatalog.json, kept identical to the Rust table by a test in the desktop shell). It doesn't
// render, play, or check the sequence against the show.

import catalogJson from "./effectCatalog.json";
import type { MemoryBackend } from "./memory";
import { renderSequenceFrame } from "./memoryRender";
import { importVendor, inspectVendor, type MemoryVendorPackage } from "./memoryVendor";
import type {
  MissingFile,
  PlaybackStatus,
  SequenceImportSummary,
  ShowSnapshot,
  VendorImportOptions,
  VendorInspection,
  VendorMapping,
} from "./types";
import { fileName } from "../lib/format";
import { missingFile, resolveAudio } from "../lib/showFiles";
import {
  noChanges,
  type Analysis,
  type EffectInfo,
  type ExportLayout,
  type ExportProgress,
  type ExportSummary,
  type Mark,
  type Row,
  type Sequence,
  type SequenceChanges,
  type SequenceEdit,
  type SequenceEditResult,
  type SequenceIssue,
  type SequenceRecovery,
  type SequenceSnapshot,
  type TimingImported,
  type TimingTrack,
} from "./sequence";
import type { ProviderId } from "./assistant";
import type { LyricsCandidate, LyricsChoice, LyricsFound, LyricsGate, LyricsOptions, LyricsRetimed, MusicFound, SequencerApi } from "./sequencer";
import * as marks from "./timingMarks";
import { formatMs } from "./timingMarks";
import { wordPhonemes } from "../lib/submodels";

/** The engine's effect catalog (a copy of the Rust table). */
export const EFFECT_CATALOG = catalogJson as unknown as EffectInfo[];

const NO_SEQUENCE = "No sequence is open. Create or open one first.";

function fail(message: string): never {
  throw new Error(message);
}

export { formatMs };

/** A word's syllables, roughly: cut a letter before each vowel group after the first (the
 * desktop app says them with a pronunciation dictionary). */
function roughSyllables(word: string): string[] {
  const groups = [...word.matchAll(/[aeiouy]+/gi)].map((m) => m.index ?? 0);
  const cuts: number[] = [];
  for (const at of groups.slice(1)) {
    const cut = Math.max(at - 1, (cuts[cuts.length - 1] ?? 0) + 1, (groups[0] ?? 0) + 1);
    if (cut < word.length && /[a-z]/i.test(word.slice(cut))) cuts.push(cut);
  }
  return [0, ...cuts].map((start, i) => word.slice(start, cuts[i] ?? word.length));
}

/** Syllables and mouth shapes for timed words, roughly: each word's time shared evenly by its
 * syllables, and each syllable's by the shapes its letters make. */
function sungMarks(words: Mark[]): { syllables: Mark[]; phonemes: Mark[] } {
  const syllables: Mark[] = [];
  const phonemes: Mark[] = [];
  const spread = (startMs: number, endMs: number, labels: string[], out: Mark[]) => {
    const n = Math.min(labels.length, endMs - startMs);
    for (let k = 0; k < n; k++) {
      out.push({ startMs: startMs + Math.round(((endMs - startMs) * k) / n), endMs: startMs + Math.round(((endMs - startMs) * (k + 1)) / n), label: labels[k] });
    }
  };
  for (const word of words) {
    const parts: Mark[] = [];
    spread(word.startMs, word.endMs, roughSyllables(word.label), parts);
    syllables.push(...parts);
    for (const part of parts) {
      const shapes = wordPhonemes(part.label).map((p) => (p === "ETC" ? "etc" : p === "REST" ? "rest" : p));
      spread(part.startMs, part.endMs, shapes, phonemes);
    }
  }
  return { syllables, phonemes };
}

/** Characters in a name or label (the engine's MAX_TEXT_LEN). */
const MAX_TEXT_LEN = 4096;

/** The first timing size limit the sequence breaks, in the engine's words; null when it fits. */
export function timingLimitProblem(doc: Sequence): string | null {
  const count = doc.timingTracks.reduce((n, t) => n + t.marks.length, 0);
  if (count > marks.MAX_MARKS) return `The sequence has ${count} timing marks; at most ${marks.MAX_MARKS} are allowed.`;
  const tooLong = (s: string) => [...s].length > MAX_TEXT_LEN;
  if (doc.timingTracks.some((t) => tooLong(t.name) || t.marks.some((m) => tooLong(m.label)))) {
    return `A name, label, or file path in the sequence is longer than ${MAX_TEXT_LEN} characters.`;
  }
  return null;
}

/** The first effect setting outside its catalog range, in the engine's words; null when all fit. */
export function settingProblem(doc: Sequence): string | null {
  for (const row of doc.rows) {
    for (const layer of row.layers) {
      for (const effect of layer.effects) {
        const info = EFFECT_CATALOG.find((e) => e.kind === effect.params.kind);
        if (!info) continue;
        for (const setting of info.settings) {
          if (setting.type !== "number" && setting.type !== "int") continue;
          const value = (effect.params as Record<string, unknown>)[setting.key];
          if (value === undefined) continue;
          let why: string | null = null;
          if (typeof value !== "number" || !Number.isFinite(value)) {
            why = `isn't a usable number; use ${setting.min} to ${setting.max}`;
          } else if (value < setting.min || value > setting.max) {
            why = `is ${value}; use ${setting.min} to ${setting.max}`;
          }
          if (why) {
            return `The ${info.label} effect at ${formatMs(effect.startMs)} has a setting PixelFlow can't use: ${setting.label} ${why}.`;
          }
        }
        for (const [key, curve] of Object.entries(effect.curves ?? {})) {
          const setting = info.settings.find((s) => s.key === key);
          const range = key === "sparkles" ? { min: 0, max: 200 } : key === "blur" ? { min: 0, max: 14 } : setting?.type === "number" || setting?.type === "int" ? setting : null;
          if (!range) return `The ${info.label} effect at ${formatMs(effect.startMs)} has a setting PixelFlow can't use: '${key}' can't change over the effect.`;
          for (const v of [curve.from, curve.to]) {
            if (!Number.isFinite(v) || v < range.min || v > range.max) {
              return `The ${info.label} effect at ${formatMs(effect.startMs)} has a setting PixelFlow can't use: ${setting?.label ?? key}'s curve goes to ${v}; use ${range.min} to ${range.max}.`;
            }
          }
        }
      }
    }
  }
  return null;
}

function checkTiming(startMs: number, endMs: number) {
  if (endMs <= startMs) fail("An effect must end after it starts.");
}

function gone(kind: string): never {
  return fail(`That ${kind} isn't in the sequence anymore.`);
}

function findRow(doc: Sequence, id: string): Row {
  return doc.rows.find((r) => r.id === id) ?? gone("row");
}

function locate(doc: Sequence, id: string): [number, number, number] | null {
  for (let r = 0; r < doc.rows.length; r++) {
    const layers = doc.rows[r].layers;
    for (let l = 0; l < layers.length; l++) {
      const e = layers[l].effects.findIndex((x) => x.id === id);
      if (e >= 0) return [r, l, e];
    }
  }
  return null;
}

function layerOf(row: Row, layer: number) {
  if (layer === row.layers.length) row.layers.push({ effects: [] });
  return row.layers[layer] ?? fail(`That row has no layer ${layer + 1}.`);
}

/** Applies one edit to `doc` in place, like the engine's SequenceEdit::apply. */
export function applySequenceEdit(doc: Sequence, edit: SequenceEdit) {
  switch (edit.type) {
    case "updateInfo":
      Object.assign(doc, { name: edit.name, audio: edit.audio, durationMs: edit.durationMs, frameMs: edit.frameMs });
      return;
    case "addRow": {
      if (doc.rows.some((r) => r.id === edit.row.id)) fail("A row with that id already exists.");
      const at = Math.min(edit.index ?? doc.rows.length, doc.rows.length);
      doc.rows.splice(at, 0, structuredClone(edit.row));
      return;
    }
    case "removeRow":
      findRow(doc, edit.id);
      doc.rows = doc.rows.filter((r) => r.id !== edit.id);
      return;
    case "moveRow": {
      const row = findRow(doc, edit.id);
      doc.rows = doc.rows.filter((r) => r.id !== edit.id);
      doc.rows.splice(Math.min(edit.index, doc.rows.length), 0, row);
      return;
    }
    case "addLayer": {
      const row = findRow(doc, edit.row);
      row.layers.splice(Math.min(edit.index ?? row.layers.length, row.layers.length), 0, { effects: [] });
      return;
    }
    case "removeLayer": {
      const row = findRow(doc, edit.row);
      if (edit.layer >= row.layers.length) fail(`That row has no layer ${edit.layer + 1}.`);
      row.layers.splice(edit.layer, 1);
      return;
    }
    case "addEffect": {
      checkTiming(edit.effect.startMs, edit.effect.endMs);
      if (locate(doc, edit.effect.id)) fail("An effect with that id already exists.");
      layerOf(findRow(doc, edit.row), edit.layer).effects.push(structuredClone(edit.effect));
      return;
    }
    case "updateEffect": {
      checkTiming(edit.effect.startMs, edit.effect.endMs);
      const [r, l, e] = locate(doc, edit.effect.id) ?? gone("effect");
      doc.rows[r].layers[l].effects[e] = structuredClone(edit.effect);
      return;
    }
    case "setEffectTiming": {
      checkTiming(edit.startMs, edit.endMs);
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      Object.assign(doc.rows[r].layers[l].effects[e], { startMs: edit.startMs, endMs: edit.endMs });
      return;
    }
    case "setEffectParams": {
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      doc.rows[r].layers[l].effects[e].params = structuredClone(edit.params);
      return;
    }
    case "moveEffect": {
      checkTiming(edit.startMs, edit.endMs);
      const target = findRow(doc, edit.row);
      if (edit.layer > target.layers.length) fail(`That row has no layer ${edit.layer + 1}.`);
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      const [effect] = doc.rows[r].layers[l].effects.splice(e, 1);
      layerOf(target, edit.layer).effects.push({ ...effect, startMs: edit.startMs, endMs: edit.endMs });
      return;
    }
    case "removeEffect": {
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      doc.rows[r].layers[l].effects.splice(e, 1);
      return;
    }
    case "addTimingTrack":
      marks.checkMarks(edit.track);
      if (doc.timingTracks.some((t) => t.id === edit.track.id)) fail("A timing track with that id already exists.");
      doc.timingTracks.push(structuredClone(edit.track));
      return;
    case "updateTimingTrack": {
      marks.checkMarks(edit.track);
      const at = doc.timingTracks.findIndex((t) => t.id === edit.track.id);
      if (at < 0) gone("timing track");
      doc.timingTracks[at] = structuredClone(edit.track);
      return;
    }
    case "removeTimingTrack":
      findTrack(doc, edit.id);
      doc.timingTracks = doc.timingTracks.filter((t) => t.id !== edit.id);
      return;
    case "renameTimingTrack": {
      const name = edit.name.trim();
      if (!name) fail("A timing track needs a name.");
      findTrack(doc, edit.id).name = name;
      return;
    }
    case "moveTimingTrack": {
      const track = findTrack(doc, edit.id);
      doc.timingTracks = doc.timingTracks.filter((t) => t.id !== edit.id);
      doc.timingTracks.splice(Math.min(edit.index, doc.timingTracks.length), 0, track);
      return;
    }
    case "addMarks": {
      const track = editableTrack(doc, edit.track);
      if (edit.marks.length > 0) marks.checkInside(Math.max(...edit.marks.map((m) => m.endMs)), doc.durationMs);
      marks.addMarks(track, edit.marks);
      return;
    }
    case "setMark": {
      const track = editableTrack(doc, edit.track);
      if (edit.index >= track.marks.length) markGone();
      marks.checkMark(edit.mark);
      // A mark already past the end (the sequence was shortened) can still be relabelled.
      if (edit.mark.endMs > track.marks[edit.index].endMs) marks.checkInside(edit.mark.endMs, doc.durationMs);
      const other = marks.overlapWith(track.marks, edit.mark, [edit.index]);
      if (other >= 0) fail(marks.overlapMessage(track, track.marks[other]));
      // A new label is sung as it's spelled, unless told otherwise.
      const old = track.marks[edit.index];
      const mark = { ...edit.mark };
      if (mark.label !== old.label && mark.sung === old.sung) delete mark.sung;
      track.marks.splice(edit.index, 1);
      track.marks.splice(marks.insertIndex(track.marks, mark.startMs), 0, mark);
      return;
    }
    case "removeMarks": {
      const track = editableTrack(doc, edit.track);
      if (edit.indices.some((i) => i >= track.marks.length)) markGone();
      const goneAt = new Set(edit.indices);
      track.marks = track.marks.filter((_, i) => !goneAt.has(i));
      return;
    }
    case "splitMark": {
      const track = editableTrack(doc, edit.track);
      const mark = track.marks[edit.index] ?? markGone();
      if (edit.atMs <= mark.startMs || edit.atMs >= mark.endMs) {
        fail(`Split a mark at a time inside it (between ${formatMs(mark.startMs)} and ${formatMs(mark.endMs)}).`);
      }
      track.marks.splice(edit.index, 1, { ...mark, endMs: edit.atMs }, { startMs: edit.atMs, endMs: mark.endMs, label: "" });
      return;
    }
    case "mergeMarks": {
      const track = editableTrack(doc, edit.track);
      const first = track.marks[edit.index] ?? markGone();
      const next = track.marks[edit.index + 1] ?? fail("There's no mark after that one to merge it with.");
      const label = [first.label.trim(), next.label.trim()].filter(Boolean).join(" ");
      track.marks.splice(edit.index, 2, { startMs: first.startMs, endMs: Math.max(first.endMs, next.endMs), label });
      return;
    }
    case "generateMarks": {
      marks.checkInside(edit.toMs, doc.durationMs);
      const made = marks.fixedMarks(edit.everyMs, edit.fromMs, edit.toMs);
      replaceRange(editableTrack(doc, edit.track), edit.fromMs, edit.toMs, made);
      return;
    }
    case "copyMarks": {
      const made = marks.everyNthMark(findTrack(doc, edit.from).marks, edit.every);
      const track = editableTrack(doc, edit.to);
      track.marks = [];
      marks.addMarks(track, made);
      return;
    }
    case "spreadLyrics": {
      marks.checkInside(edit.toMs, doc.durationMs);
      const made = marks.spreadPhrases(edit.lines, edit.fromMs, edit.toMs);
      replaceRange(editableTrack(doc, edit.track), edit.fromMs, edit.toMs, made);
      return;
    }
    case "labelMarks": {
      const track = editableTrack(doc, edit.track);
      if (edit.indices.length !== edit.labels.length) {
        fail(
          `There are ${count(edit.labels.length, "line of lyrics", "lines of lyrics")} and ${count(edit.indices.length, "chosen mark", "chosen marks")}; choose one mark per line, or spread the lyrics over a time range instead.`,
        );
      }
      if (edit.indices.some((i) => i >= track.marks.length)) markGone();
      edit.indices.forEach((i, k) => (track.marks[i] = { ...track.marks[i], label: edit.labels[k].trim() }));
      return;
    }
    case "breakIntoWords": {
      if (edit.track === edit.words) fail("Put the words on a different timing track from the phrases.");
      const phrases = findTrack(doc, edit.track);
      const chosen = edit.indices.map((i) => phrases.marks[i] ?? markGone());
      const target = editableTrack(doc, edit.words);
      let made = 0;
      for (const phrase of chosen) {
        const words = marks.splitWords(phrase);
        if (words.length === 0) continue;
        made += words.length;
        replaceRange(target, phrase.startMs, phrase.endMs, words);
      }
      if (made === 0) fail("Those marks have no words in them yet. Give them lyrics first.");
      return;
    }
  }
}

function findTrack(doc: Sequence, id: string): TimingTrack {
  return doc.timingTracks.find((t) => t.id === id) ?? gone("timing track");
}

/** A track whose marks may change (phonemes from xLights stay as they are). */
function editableTrack(doc: Sequence, id: string): TimingTrack {
  const track = findTrack(doc, id);
  if (track.kind === "phonemes") fail("Phoneme tracks can't be edited mark by mark; edit the words, then use Break into syllables on the words track.");
  return track;
}

function markGone(): never {
  return fail("That mark isn't on the timing track anymore.");
}

function replaceRange(track: TimingTrack, fromMs: number, toMs: number, made: Mark[]) {
  marks.clearRange(track, fromMs, toMs);
  marks.addMarks(track, made);
}

function count(n: number, one: string, many: string): string {
  return n === 1 ? `1 ${one}` : `${n} ${many}`;
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

function listChanges<T extends { id: string }>(before: T[], after: T[]) {
  const was = new Map(before.map((x) => [x.id, x]));
  const now = new Set(after.map((x) => x.id));
  return {
    changed: after.filter((x) => !same(was.get(x.id), x)),
    removed: before.filter((x) => !now.has(x.id)).map((x) => x.id),
    order: same(
      before.map((x) => x.id),
      after.map((x) => x.id),
    )
      ? null
      : after.map((x) => x.id),
  };
}

/** What changed from `before` to `after`, as whole rows and tracks (always a valid reply). */
export function diffSequences(before: Sequence, after: Sequence): SequenceChanges {
  const changes = noChanges();
  if (
    before.name !== after.name ||
    before.audio !== after.audio ||
    before.durationMs !== after.durationMs ||
    before.frameMs !== after.frameMs
  ) {
    changes.info = { name: after.name, audio: after.audio, durationMs: after.durationMs, frameMs: after.frameMs };
  }
  const rows = listChanges(before.rows, after.rows);
  changes.rows = rows.changed;
  changes.removedRows = rows.removed;
  changes.rowOrder = rows.order;
  const tracks = listChanges<TimingTrack>(before.timingTracks, after.timingTracks);
  changes.timingTracks = tracks.changed;
  changes.removedTimingTracks = tracks.removed;
  changes.trackOrder = tracks.order;
  return changes;
}

function newSequence(name: string, durationMs: number): Sequence {
  return { schemaVersion: 7, name, audio: null, durationMs, frameMs: 25, timingTracks: [], rows: [] };
}

/** Find lyrics' steps that read the whole song, and so report how far they've got. */
const READS_THE_SONG = ["Lining up the words", "Separating the vocals", "Aligning the words"];

/** The sequencer in memory. Each instance holds one open sequence, like the engine. */
export class MemorySequencer implements SequencerApi {
  doc: Sequence | null = null;
  path: string | null = null;
  revision = 0;
  savedRevision = 0;
  undoStack: { before: Sequence; gesture: string | null }[] = [];
  redoStack: { after: Sequence; gesture: string | null }[] = [];
  /** Files "on disk", keyed by path. */
  files = new Map<string, Sequence>();
  /** The problems every reply lists (the engine's check against the show; set by tests). */
  issues: SequenceIssue[] = [];
  /** Unsaved work an earlier run "kept", with the document each holds. */
  recoveries: (SequenceRecovery & { doc: Sequence })[] = [];
  /** What the file dialogs return. */
  nextOpenPath: string | null = null;
  nextSavePath: string | null = null;
  /** Calls made, for test assertions. */
  calls: string[] = [];
  /** What the xLights sequence dialog returns, and what importing any .xsq produces. */
  nextXlightsSequencePath: string | null = null;
  xlightsSequenceImport: { sequence: Sequence; summary: SequenceImportSummary; notes: string[] } | null = null;
  /** Vendor packages "on disk", by path, and the mappings remembered for each vendor. */
  vendorPackages = new Map<string, MemoryVendorPackage>();
  savedVendorMappings = new Map<string, VendorMapping>();
  /** xLights mapping files "on disk", and what the folder and .xmap dialogs return. */
  xmapFiles = new Map<string, VendorMapping>();
  nextXlightsPackageFolder: string | null = null;
  nextXmapPath: string | null = null;
  nextXmapSavePath: string | null = null;
  /** Timing files "on disk" (what importing each gives), the tracks exported to each path, and
   * what the timing file dialog returns. */
  timingFiles = new Map<string, { tracks: TimingTrack[]; notes: string[] }>();
  exportedTimingFiles = new Map<string, TimingTrack[]>();
  nextTimingPath: string | null = null;
  private lastGesture: string | null = null;
  /** Changes with every new or opened document (like the engine's sequence_doc_id). */
  private docId = 0;
  private exportCancels = 0;
  /** Whether a playing sequence would go out to the controllers. */
  sendToControllers = true;
  /** Whether a playing sequence goes round again from the top at the end. */
  looping = false;
  /** How long edit, undo, and redo replies take to come back (tests of a slow engine). The edit
   * itself lands at once, as in the engine; only the answer is late. */
  replyDelayMs = 0;
  /** How long beat detection takes (reporting progress as it goes, with a memory backend). */
  analysisDelayMs = 0;
  /** How long working out an opened sequence's audio track takes in the background (0: it
   * isn't, as if it were kept from before). */
  audioTrackMs = 0;
  /** Whether the assistant has a key for a provider (the stand-in assistant answers this). */
  hasAssistantKey: (provider: ProviderId) => boolean = () => false;
  /** How long each step of Find lyrics takes, and whether published lyrics are "found". */
  lyricsStepMs = 0;
  lyricsFound = true;
  private lyricsCancels = 0;
  /** The published lyrics Find lyrics "finds", best first, and each one's (made-up) lines. */
  lyricsCandidates: { candidate: LyricsCandidate; lines: string[] }[] = [
    {
      candidate: { id: 1, artist: "Lantern Band", title: "Lantern Song", durationS: 238, language: "English", synced: true },
      lines: ["Paper lanterns glowing", "Snowy rooftops shine", "Bells across the valley", "Ring the winter night"],
    },
    {
      candidate: { id: 2, artist: "Lantern Band", title: "Lantern Song (Live)", durationS: 240.5, language: "English", synced: false },
      lines: ["Paper lanterns glowing", "Snowy rooftops shining", "Bells across the valley", "Ringing in the night"],
    },
    {
      candidate: { id: 3, artist: "Cover Band", title: "Lantern Song", durationS: 237, language: "Russian", synced: true },
      lines: ["Привет молоко", "Молоко и снег", "Привет привет", "Снег и свет"],
    },
  ];
  /** What the last Find lyrics was asked with, and whether the audio was "heard". */
  lastLyricsOptions: LyricsOptions | null = null;
  private lyricsHeard: boolean | null = null;
  /** The last lyrics were timed by on-device alignment. */
  private lyricsAligned = false;

  /** With a memory backend, frames are drawn (roughly) from its show and playback runs on its clock. */
  constructor(readonly backend: MemoryBackend | null = null) {}

  private async reply<T>(value: T, delayMs = this.replyDelayMs): Promise<T> {
    if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs));
    return value;
  }

  private open_(): Sequence {
    return this.doc ?? fail(NO_SEQUENCE);
  }

  private snapshot(): SequenceSnapshot {
    const sequence = this.open_();
    return {
      revision: this.revision,
      path: this.path,
      dirty: this.revision !== this.savedRevision,
      canUndo: this.undoStack.length > 0,
      canRedo: this.redoStack.length > 0,
      sequence: structuredClone(sequence),
      issues: structuredClone(this.issues),
    };
  }

  private result(changes: SequenceChanges | null): SequenceEditResult {
    return {
      revision: this.revision,
      dirty: this.revision !== this.savedRevision,
      canUndo: this.undoStack.length > 0,
      canRedo: this.redoStack.length > 0,
      changed: changes !== null,
      changes: changes ?? noChanges(),
      issues: structuredClone(this.issues),
    };
  }

  private replace(doc: Sequence, path: string | null) {
    this.docId++;
    this.doc = doc;
    this.path = path;
    this.revision++;
    this.savedRevision = this.revision;
    this.undoStack = [];
    this.redoStack = [];
    this.lastGesture = null;
    // The engine works out the music's audio track (what effects that follow it read) in the
    // background once a sequence with music opens.
    if (doc.audio && this.audioTrackMs > 0) void this.backend?.readMusic("audioTrack", doc.audio, undefined, this.audioTrackMs);
  }

  async newSequenceDoc(name: string, durationMs: number, audio: string | null = null, rows: Row[] = []) {
    this.calls.push("newSequenceDoc");
    if (durationMs > 4 * 60 * 60 * 1000) fail("PixelFlow sequences can be at most 4 hours.");
    if (new Set(rows.map((r) => r.id)).size !== rows.length) fail("A row with that id already exists.");
    this.replace({ ...newSequence(name, durationMs), audio: audio?.trim() ? audio : null, rows: structuredClone(rows) }, null);
    return this.snapshot();
  }

  async sequenceRecoveries(): Promise<SequenceRecovery[]> {
    return this.recoveries.map(({ doc: _doc, ...recovery }) => recovery).sort((a, b) => b.savedAtMs - a.savedAtMs);
  }

  async recoverSequence(id: string) {
    this.calls.push(`recoverSequence:${id}`);
    const kept = this.recoveries.find((r) => r.id === id) ?? fail("That unsaved sequence isn't there anymore.");
    this.replace(structuredClone(kept.doc), kept.path);
    // Opened with unsaved changes.
    this.savedRevision = -1;
    this.recoveries = this.recoveries.filter((r) => r.id !== id);
    return this.snapshot();
  }

  async discardSequenceRecovery(id: string) {
    this.calls.push(`discardSequenceRecovery:${id}`);
    this.recoveries = this.recoveries.filter((r) => r.id !== id);
  }

  async openSequenceDoc(path: string) {
    this.calls.push("openSequenceDoc");
    const doc = this.files.get(path) ?? fail(`Could not read ${path}: no such file`);
    this.replace(structuredClone(doc), path);
    return this.snapshot();
  }

  async saveSequenceDoc() {
    this.open_();
    if (!this.path) fail("This sequence has not been saved yet. Choose where to save it.");
    return this.saveSequenceDocAs(this.path);
  }

  async saveSequenceDocAs(path: string) {
    this.calls.push("saveSequenceDocAs");
    this.files.set(path, structuredClone(this.open_()));
    this.path = path;
    this.savedRevision = this.revision;
    return this.snapshot();
  }

  async closeSequenceDoc() {
    this.doc = null;
    this.path = null;
  }

  async getSequenceDoc() {
    return this.doc ? this.snapshot() : null;
  }

  async editSequence(edits: SequenceEdit[], gesture?: string) {
    this.calls.push("editSequence");
    const before = this.open_();
    const next = structuredClone(before);
    for (const edit of edits) applySequenceEdit(next, edit);
    const problem = settingProblem(next) ?? timingLimitProblem(next);
    if (problem) fail(problem);
    if (same(before, next)) return this.result(null);
    const changes = diffSequences(before, next);
    const top = this.undoStack.at(-1);
    const merge = gesture !== undefined && gesture === this.lastGesture && top?.gesture === gesture;
    if (!merge) this.undoStack.push({ before, gesture: gesture ?? null });
    this.lastGesture = gesture ?? null;
    this.redoStack = [];
    this.doc = next;
    this.revision++;
    return this.reply(this.result(changes));
  }

  async undoSequence() {
    this.calls.push("undoSequence");
    const now = this.open_();
    const step = this.undoStack.pop();
    if (!step) return this.result(null);
    this.redoStack.push({ after: now, gesture: step.gesture });
    this.doc = step.before;
    this.lastGesture = null;
    this.revision++;
    return this.reply(this.result(diffSequences(now, step.before)));
  }

  async redoSequence() {
    this.calls.push("redoSequence");
    const now = this.open_();
    const step = this.redoStack.pop();
    if (!step) return this.result(null);
    this.undoStack.push({ before: now, gesture: step.gesture });
    this.doc = step.after;
    this.lastGesture = null;
    this.revision++;
    return this.reply(this.result(diffSequences(now, step.after)));
  }

  async effectCatalog() {
    return structuredClone(EFFECT_CATALOG);
  }

  async sequenceDocFrame(positionMs: number) {
    const doc = this.open_();
    return this.backend ? renderSequenceFrame(doc, this.backend.show, positionMs) : new Uint8Array();
  }

  async playSequenceDoc(positionMs: number): Promise<PlaybackStatus> {
    const doc = this.open_();
    this.calls.push(`playSequenceDoc@${positionMs}`);
    const backend = this.backend ?? fail("Playing an authored sequence needs the PixelFlow desktop app.");
    const live = () => this.doc ?? doc;
    return backend.playAuthored(
      {
        path: this.path ?? "",
        music: doc.audio,
        durationMs: doc.durationMs,
        frameMs: doc.frameMs,
        looping: this.looping,
        frame: (ms) => renderSequenceFrame(live(), backend.show, ms),
      },
      positionMs,
    );
  }

  async setSequenceDocOutput(send: boolean) {
    this.calls.push(`setSequenceDocOutput:${send}`);
    this.sendToControllers = send;
    return (await this.backend?.playbackStatus()) ?? null;
  }

  async setSequenceDocLoop(looping: boolean) {
    this.calls.push(`setSequenceDocLoop:${looping}`);
    this.looping = looping;
    return this.backend?.setAuthoredLooping(looping) ?? null;
  }

  async addSequenceDocToShow(path: string): Promise<ShowSnapshot> {
    const doc = this.open_();
    this.calls.push(`addSequenceDocToShow:${path}`);
    const backend = this.backend ?? fail("Adding to the show needs a show.");
    const base = doc.name.trim() || "Sequence";
    // Exported to the same file again: that entry is brought up to date instead.
    const existing = backend.show.sequences.find((s) => s.path === path);
    const taken = (n: string) => backend.show.sequences.some((s) => s.name === n && s.id !== existing?.id);
    if (existing) {
      const updated = { ...existing, name: taken(base) ? existing.name : base, audio: doc.audio };
      return backend.applyEdits([{ type: "updateSequence", sequence: updated }]);
    }
    let name = base;
    for (let n = 2; taken(name); n++) name = `${base} (${n})`;
    return backend.applyEdits([
      { type: "addSequence", sequence: { id: crypto.randomUUID(), name, path, audio: doc.audio, offsetMs: 0 } },
    ]);
  }

  async sequenceExportLayout(): Promise<ExportLayout> {
    this.open_();
    return { channels: 0, blocks: [], notes: [] };
  }

  async exportSequenceDoc(path: string, onProgress?: (progress: ExportProgress) => void): Promise<ExportSummary> {
    this.calls.push("exportSequenceDoc");
    const doc = this.open_();
    const started = this.exportCancels;
    const frames = Math.ceil(doc.durationMs / doc.frameMs);
    let last = -1;
    for (let done = 1; done <= frames; done++) {
      const percent = Math.floor((done * 100) / frames);
      if (percent !== last || done === frames) {
        last = percent;
        onProgress?.({ path, framesDone: done, frames, percent });
        // Let a cancel from the UI (or the callback) land between steps.
        await Promise.resolve();
      }
      if (this.exportCancels !== started) fail("The export was cancelled.");
    }
    return {
      frames,
      frameMs: doc.frameMs,
      durationMs: doc.durationMs,
      channels: 0,
      media: doc.audio?.split(/[\\/]/).pop() ?? null,
      blocks: [],
      notes: [],
    };
  }

  async cancelSequenceExport() {
    this.exportCancels++;
  }

  async analyzeAudio(_path: string): Promise<Analysis> {
    const durationMs = this.doc?.durationMs ?? 60_000;
    const beats = Array.from({ length: Math.floor(durationMs / 500) }, (_, i) => i * 500);
    const bars = beats.filter((_, i) => i % 4 === 0);
    // A quiet intro, a verse, a loud chorus, and an outro, on bar lines.
    const at = (fraction: number) => bars.reduce((best, b) => (Math.abs(b - durationMs * fraction) < Math.abs(best - durationMs * fraction) ? b : best), 0);
    const cuts = [0, at(0.125), at(0.5), at(0.875), durationMs];
    const parts = [
      { label: "Intro", group: "A", level: "low", energy: 0.3 },
      { label: "Verse", group: "B", level: "medium", energy: 0.6 },
      { label: "Chorus", group: "C", level: "high", energy: 0.95 },
      { label: "Outro", group: "D", level: "low", energy: 0.3 },
    ] as const;
    const sections = parts
      .map((p, i) => ({ ...p, startMs: cuts[i], endMs: cuts[i + 1], confidence: 0.8 }))
      .filter((s) => s.endMs > s.startMs);
    const energyAt = (ms: number) => sections.find((s) => ms >= s.startMs && ms < s.endMs)?.energy ?? 0.3;
    const chorus = sections.find((s) => s.label === "Chorus");
    const events: Analysis["events"] = chorus
      ? [
          { timeMs: chorus.startMs, kind: "drop", strength: 1 },
          { timeMs: chorus.startMs + 4000, kind: "hit", strength: 0.8 },
        ]
      : [];
    const moments: Analysis["moments"] = chorus
      ? [
          { timeMs: chorus.startMs, kind: "drop", strength: 1, importance: 0.9, label: "Chorus", suggest: "burst" },
          { timeMs: chorus.startMs + 4000, kind: "impact", strength: 0.8, importance: 0.7, suggest: "hit" },
        ]
      : [];
    return {
      durationMs,
      tempoBpm: 120,
      beats,
      bars,
      onsets: beats,
      sections,
      events,
      barEnergy: bars.map((b) => {
        const e = energyAt(b);
        return { overall: e, low: e, mid: e, high: e };
      }),
      moments,
      drums: chorus ? [{ timeMs: chorus.startMs, drum: "crash", strength: 1 }] : [],
      barDrums: bars.map((b) => {
        const n = energyAt(b) > 0.5 ? 2 : 0;
        return { kick: n, snare: n, hat: 2 * n, crash: b === chorus?.startMs ? 1 : 0 };
      }),
      confidence: { tempo: 0.9, downbeat: 0.8, sections: 0.8 },
    };
  }

  async detectBeats() {
    const doc = this.open_();
    const docId = this.docId;
    if (!doc.audio) fail("This sequence has no music yet. Choose a song for it first.");
    const analysis = await this.analyzeAudio(doc.audio);
    if (this.backend) await this.backend.readMusic("beats", doc.audio, undefined, this.analysisDelayMs);
    else await this.reply(undefined, this.analysisDelayMs);
    if (this.docId !== docId || this.doc?.audio !== doc.audio) {
      fail("The sequence or its music changed while the beats were being found. Run beat detection again.");
    }
    const latest = this.open_();
    const marks = (times: number[], label: (i: number) => string) =>
      times.map((t, i) => ({ startMs: t, endMs: times[i + 1] ?? doc.durationMs, label: label(i) }));
    const beatMs = 60_000 / (analysis.tempoBpm ?? 120);
    const found: TimingTrack[] = [
      { id: crypto.randomUUID(), name: "Sections", kind: "sections", marks: analysis.sections.map((s) => ({ startMs: s.startMs, endMs: s.endMs, label: s.label })) },
      {
        id: crypto.randomUUID(),
        name: "Accents",
        kind: "custom",
        marks: analysis.events.map((e) => ({ startMs: e.timeMs, endMs: e.timeMs + (e.durationMs ?? beatMs), label: e.kind[0].toUpperCase() + e.kind.slice(1) })),
      },
      {
        id: crypto.randomUUID(),
        name: "Moments",
        kind: "custom",
        marks: analysis.moments.map((m) => {
          const kind = m.kind.replace("_", " ");
          const word = kind[0].toUpperCase() + kind.slice(1);
          return { startMs: m.timeMs, endMs: m.endMs ?? m.timeMs + beatMs / 2, label: m.label ? `${word}: ${m.label}` : word };
        }),
      },
    ];
    const drums: TimingTrack = {
      id: crypto.randomUUID(),
      name: "Drums",
      kind: "custom",
      marks: analysis.drums
        .filter((d) => d.drum !== "hat")
        .map((d) => ({ startMs: d.timeMs, endMs: d.timeMs + beatMs / 4, label: d.drum[0].toUpperCase() + d.drum.slice(1) })),
    };
    // Sections, Accents, and Moments already there are the user's own: kept as they are.
    const tracks: TimingTrack[] = [
      { id: crypto.randomUUID(), name: "Beats", kind: "beats", marks: marks(analysis.beats, (i) => String((i % 4) + 1)) },
      { id: crypto.randomUUID(), name: "Bars", kind: "bars", marks: marks(analysis.bars, (i) => String(i + 1)) },
      ...found.filter((t) => t.marks.length > 0 && !latest.timingTracks.some((have) => have.name === t.name)),
      ...(drums.marks.length > 0 ? [drums] : []),
    ];
    const edits: SequenceEdit[] = [
      ...latest.timingTracks
        .filter((t) => tracks.some((n) => n.name === t.name))
        .map((t) => ({ type: "removeTimingTrack" as const, id: t.id })),
      ...tracks.map((track) => ({ type: "addTimingTrack" as const, track })),
    ];
    return this.editSequence(edits);
  }

  async lyricsGate(provider: ProviderId | null): Promise<LyricsGate> {
    if (!provider || !this.hasAssistantKey(provider)) {
      const what = provider ? `add your ${provider === "openai" ? "OpenAI" : "Anthropic"} key` : "set it up";
      return { ready: false, reason: `Finding lyrics needs the assistant: ${what} in Settings → AI.`, recognizer: false };
    }
    return { ready: true, reason: null, recognizer: provider === "openai" };
  }

  /** Made-up lyrics spread over the sequence, as if LRCLIB (and OpenAI) had found them. */
  async findLyrics(provider: ProviderId | null, upload: boolean, options: LyricsOptions, onProgress?: (label: string, fraction: number | null) => void): Promise<LyricsFound> {
    this.calls.push(`findLyrics:${provider}:${upload}`);
    this.lastLyricsOptions = options;
    const gate = await this.lyricsGate(provider);
    if (!gate.ready) fail(gate.reason ?? "");
    const doc = this.open_();
    if (!doc.audio) fail("This sequence has no music yet. Choose a song for it first.");
    const started = this.lyricsCancels;
    // Aligned on this computer, published lyrics need nothing sent to OpenAI.
    const aligned = options.align === true;
    const heard = gate.recognizer && upload && !(aligned && this.lyricsFound);
    const steps = [
      "Reading the song",
      "Looking up published lyrics",
      ...(heard ? ["Sending the audio to OpenAI to hear the words"] : []),
      "Lining up the words",
      ...(aligned ? ["Separating the vocals", "Aligning the words"] : []),
    ];
    for (const label of steps) {
      onProgress?.(label, null);
      if (READS_THE_SONG.includes(label) && this.lyricsStepMs > 0) {
        // Reading the whole song: progress as it goes.
        const parts = Math.max(1, Math.round(this.lyricsStepMs / 100));
        for (let i = 0; i <= parts; i++) {
          onProgress?.(label, i / parts);
          if (i < parts) await this.reply(undefined, this.lyricsStepMs / parts);
          if (this.lyricsCancels !== started) fail("Stopped.");
        }
        continue;
      }
      await this.reply(undefined, this.lyricsStepMs);
      if (this.lyricsCancels !== started) fail("Stopped.");
    }
    if (!this.lyricsFound && !heard) fail("No lyrics found for this song.");
    this.lyricsHeard = heard;
    this.lyricsAligned = aligned;
    return this.writeLyrics(this.lyricsFound ? this.lyricsCandidates[0] : null, null);
  }

  /** Lines the lyrics up again with another candidate or pasted lyrics, asking no one. */
  async chooseLyrics(choice: LyricsChoice, options?: { align?: boolean }): Promise<LyricsFound> {
    this.calls.push(`chooseLyrics:${"candidate" in choice ? choice.candidate : "pasted"}`);
    if (this.lyricsHeard === null) fail("Find this song's lyrics first.");
    this.lyricsAligned = options?.align === true;
    if ("pasted" in choice) {
      const lines = choice.pasted
        .split("\n")
        .map((l) => l.replace(/\[[^\]]*\]/g, "").trim())
        .filter((l) => l.length > 0);
      if (lines.length === 0) fail("Paste the song's lyrics first.");
      return this.writeLyrics(null, lines);
    }
    const picked = this.lyricsCandidates.find((c) => c.candidate.id === choice.candidate);
    if (!picked) fail("Those lyrics aren't among the ones found. Find lyrics again.");
    return this.writeLyrics(picked, null);
  }

  /** The lyrics tracks for `picked`'s lines (or `pasted` ones), spread over the sequence. */
  private async writeLyrics(picked: { candidate: LyricsCandidate; lines: string[] } | null, pasted: string[] | null): Promise<LyricsFound> {
    const heard = this.lyricsHeard === true;
    const lines = pasted ?? picked?.lines ?? ["Paper lanterns glowing", "Snowy rooftops shine"];
    const latest = this.open_();
    const verse = Math.min(16_000, latest.durationMs / 2);
    const start = Math.min(4_000, latest.durationMs / 8);
    const each = verse / lines.length;
    const phrases: Mark[] = [];
    const words: Mark[] = [];
    lines.forEach((line, i) => {
      const from = Math.round(start + i * each);
      const to = Math.round(from + each * 0.85);
      phrases.push({ startMs: from, endMs: to, label: line });
      const parts = line.split(" ");
      const step = (to - from) / parts.length;
      parts.forEach((w, k) => words.push({ startMs: Math.round(from + k * step), endMs: Math.round(from + (k + 1) * step), label: w }));
    });
    const vocals: Mark[] = [{ startMs: phrases[0].startMs, endMs: phrases[phrases.length - 1].endMs, label: "Vocals" }];
    const sung = sungMarks(words);
    const found: TimingTrack[] = [
      { id: crypto.randomUUID(), name: "Lyrics", kind: "lyrics", marks: phrases },
      { id: crypto.randomUUID(), name: "Lyrics (words)", kind: "words", marks: words },
      { id: crypto.randomUUID(), name: "Lyrics (syllables)", kind: "custom", marks: sung.syllables },
      { id: crypto.randomUUID(), name: "Lyrics (phonemes)", kind: "phonemes", marks: sung.phonemes },
      { id: crypto.randomUUID(), name: "Vocals", kind: "custom", marks: vocals },
    ];
    // A track already there by name and kind takes the new marks (keeping its id).
    const edits: SequenceEdit[] = found.map((track) => {
      const had = latest.timingTracks.find((t) => t.name === track.name && t.kind === track.kind);
      return had ? { type: "updateTimingTrack" as const, track: { ...track, id: had.id } } : { type: "addTimingTrack" as const, track };
    });
    const result = await this.editSequence(edits);
    const c = picked?.candidate;
    const text = pasted ? "Lyrics: pasted" : c ? `Lyrics: ${c.artist} — ${c.title} (LRCLIB)` : "Lyrics: OpenAI speech recognition";
    const own = pasted ? "pasted" : "LRCLIB";
    const aligned = this.lyricsAligned;
    const timing = aligned ? "word timing: on this computer" : heard ? "word timing: OpenAI" : `line timing: ${own}`;
    const summary = aligned
      ? "Lyrics from LRCLIB, word timing found on this computer."
      : heard
        ? "Lyrics from LRCLIB, word timing from OpenAI."
        : "Lyrics and line timing from LRCLIB; words are spread over each line.";
    const timingNote = aligned
      ? `Word timing found on this computer for ${words.length} of ${words.length} words.`
      : heard
        ? "Word timing locked to the vocals (average shift 40 ms)."
        : null;
    return {
      result,
      summary,
      source: `${text} · ${timing}`,
      notes: [],
      lines: phrases.length,
      words: words.length,
      unsureWords: heard || aligned ? 0 : words.length,
      candidates: this.lyricsFound ? this.lyricsCandidates.map((x) => x.candidate) : [],
      chosen: pasted ? null : (c?.id ?? null),
      pasted: pasted !== null,
      timingNote,
    };
  }

  /** The lyrics track `track` belongs with, and its words, syllables, and phonemes tracks. */
  private lyricFamily(doc: Sequence, track: string): TimingTrack[] {
    const picked = doc.timingTracks.find((t) => t.id === track);
    if (!picked) return [];
    const suffixes: [TimingTrack["kind"], string][] = [
      ["lyrics", ""],
      ["words", " (words)"],
      ["custom", " (syllables)"],
      ["phonemes", " (phonemes)"],
    ];
    const own = suffixes.find(([kind, suffix]) => kind === picked.kind && picked.name.endsWith(suffix));
    if (!own) return [];
    const base = picked.name.slice(0, picked.name.length - own[1].length);
    return suffixes.flatMap(([kind, suffix]) => doc.timingTracks.filter((t) => t.kind === kind && t.name === `${base}${suffix}`));
  }

  async nudgeLyrics(track: string, ms: number): Promise<SequenceEditResult> {
    this.calls.push(`nudgeLyrics:${track}:${ms}`);
    const doc = this.open_();
    const family = this.lyricFamily(doc, track).filter((t) => t.marks.length > 0);
    if (family.length === 0 || ms === 0) fail("That isn't a lyrics track with marks to move.");
    const at = (t: number) => Math.max(0, Math.min(doc.durationMs, t + ms));
    const edits: SequenceEdit[] = family.map((t) => ({
      type: "updateTimingTrack" as const,
      track: { ...t, marks: t.marks.map((m) => ({ ...m, startMs: at(m.startMs), endMs: at(m.endMs) })).filter((m) => m.endMs > m.startMs) },
    }));
    return this.editSequence(edits);
  }

  /** There's no song to listen to here: the words stay where they are. */
  async retimeLyrics(track: string): Promise<LyricsRetimed> {
    this.calls.push(`retimeLyrics:${track}`);
    const doc = this.open_();
    const words = this.lyricFamily(doc, track).find((t) => t.kind === "words" && t.marks.length > 0);
    if (!words) fail("There are no words to re-time on that lyrics track.");
    const result = await this.editSequence([{ type: "updateTimingTrack", track: { ...words } }]);
    return { result, note: "The words already sit where the vocals start." };
  }

  async cancelLyrics() {
    this.lyricsCancels += 1;
  }

  /** Syllables and mouth shapes made again from a words track, roughly (see `sungMarks`). */
  async syllablesFromWords(track: string): Promise<SequenceEditResult> {
    this.calls.push(`syllablesFromWords:${track}`);
    const doc = this.open_();
    const words = doc.timingTracks.find((t) => t.id === track);
    if (!words || words.kind !== "words") fail("That isn't a words track.");
    if (words.marks.length === 0) fail(`${words.name} has no words yet.`);
    const base = words.name.endsWith(" (words)") ? words.name.slice(0, -" (words)".length) : words.name;
    const sung = sungMarks(words.marks);
    const made: TimingTrack[] = [
      { id: crypto.randomUUID(), name: `${base} (syllables)`, kind: "custom", marks: sung.syllables },
      { id: crypto.randomUUID(), name: `${base} (phonemes)`, kind: "phonemes", marks: sung.phonemes },
    ];
    const edits: SequenceEdit[] = made.map((t) => {
      const had = doc.timingTracks.find((h) => h.name === t.name && h.kind === t.kind);
      return had ? { type: "updateTimingTrack" as const, track: { ...t, id: had.id } } : { type: "addTimingTrack" as const, track: t };
    });
    return this.editSequence(edits);
  }

  async importTimingFile(path: string): Promise<TimingImported> {
    this.calls.push(`importTimingFile:${path}`);
    this.open_();
    const docId = this.docId;
    const file = this.timingFiles.get(path) ?? fail(`Could not read ${path}: no such file`);
    await this.reply(undefined, this.analysisDelayMs);
    if (this.docId !== docId) fail("Another sequence was opened while the timing file was being read. Import it again.");
    if (file.tracks.length === 0) fail("That file has no timing marks PixelFlow can use.");
    // Named as the engine names them: a taken name gets its number on the part a lyrics timing's
    // layers share ("Vocals 2", "Vocals 2 (words)"), so they stay paired.
    const taken = new Set(this.open_().timingTracks.map((t) => t.name));
    const layers = ["", " (words)", " (phonemes)"];
    const renamed = new Map<string, string>();
    const tracks = file.tracks.map((track) => {
      const suffix = layers.slice(1).find((l) => track.name.endsWith(l) && track.name.length > l.length) ?? "";
      const base = track.name.slice(0, track.name.length - suffix.length);
      let next = renamed.get(base);
      if (next === undefined || taken.has(`${next}${suffix}`)) {
        const free = (b: string) => layers.every((l) => !taken.has(`${b}${l}`));
        next = base;
        for (let n = 2; !free(next); n++) next = `${base} ${n}`;
        renamed.set(base, next);
      }
      const name = `${next}${suffix}`;
      taken.add(name);
      return { ...structuredClone(track), id: crypto.randomUUID(), name };
    });
    const result = await this.editSequence(tracks.map((track) => ({ type: "addTimingTrack" as const, track })));
    return { result, tracks: tracks.map((t) => t.name), notes: [...file.notes] };
  }

  async exportTimingTrack(id: string, path: string): Promise<number> {
    this.calls.push(`exportTimingTrack:${path}`);
    const doc = this.open_();
    const track = doc.timingTracks.find((t) => t.id === id) ?? fail("That timing track isn't in the sequence anymore.");
    if (!/\.(xtiming|xml|txt)$/i.test(path)) fail("Timing tracks are saved as xLights timing files (.xtiming) or Audacity labels (.txt).");
    const layers = [track];
    if (track.kind === "lyrics" && /\.(xtiming|xml)$/i.test(path)) {
      for (const [suffix, kind] of [
        ["words", "words"],
        ["phonemes", "phonemes"],
      ] as const) {
        const layer = doc.timingTracks.find((t) => t.name === `${track.name} (${suffix})` && t.kind === kind);
        if (!layer) break;
        layers.push(layer);
      }
    }
    this.exportedTimingFiles.set(path, structuredClone(layers));
    return track.marks.length;
  }

  async pickTimingFilePath() {
    return this.nextTimingPath;
  }

  async pickTimingExportPath(_defaultName: string) {
    return this.nextSavePath;
  }

  /** Where a package's music goes: the saved show's music folder, else that of the folder picked. */
  private vendorMusicFolder(picked: string | null): string | null {
    const show = this.backend?.path;
    if (show) return `${show.slice(0, show.lastIndexOf("/"))}/music`;
    return picked ? `${picked}/music` : null;
  }

  async inspectXlightsSequence(path: string, sequence?: string): Promise<VendorInspection> {
    this.calls.push(`inspectXlightsSequence:${path}`);
    const pkg = this.vendorPackages.get(path);
    if (pkg) {
      const show = this.backend?.show ?? fail("Open a show first.");
      return inspectVendor(pkg, show, sequence, this.savedVendorMappings.get(pkg.key) ?? null, this.vendorMusicFolder(null));
    }
    // A plain .xsq of the user's own: every model is in the show.
    const imported = this.xlightsSequenceImport ?? fail(`Could not read ${path}: no such file`);
    return {
      sequences: [fileName(path)],
      sequence: fileName(path),
      song: imported.sequence.name,
      hasLayout: false,
      items: [],
      targets: [],
      suggestions: [],
      mapping: { items: {} },
      key: `sequence:${path}`,
      allExact: true,
      music: null,
      musicFolder: null,
    };
  }

  async readXmap(path: string) {
    this.calls.push(`readXmap:${path}`);
    const mapping = this.xmapFiles.get(path) ?? fail(`Couldn't read ${path}: no such file`);
    return { mapping: structuredClone(mapping), nodesSkipped: 0 };
  }

  async writeXmap(path: string, mapping: VendorMapping) {
    this.calls.push(`writeXmap:${path}`);
    if (!path.toLowerCase().endsWith(".xmap")) fail("Mappings are saved as .xmap files.");
    this.xmapFiles.set(path, structuredClone(mapping));
  }

  async pickXlightsPackageFolder() {
    return this.nextXlightsPackageFolder;
  }

  async pickXmapPath() {
    return this.nextXmapPath;
  }

  async pickXmapSavePath(_defaultName: string) {
    return this.nextXmapSavePath;
  }

  async importXlightsSequence(path: string, options?: VendorImportOptions) {
    this.calls.push(`importXlightsSequence:${path}`);
    const pkg = options ? this.vendorPackages.get(path) : undefined;
    if (pkg && options) {
      const show = this.backend?.show ?? fail("Open a show first.");
      const built = importVendor(pkg, show, options.sequence, options.mapping, this.vendorMusicFolder(options.musicFolder));
      this.savedVendorMappings.set(options.key, structuredClone(options.mapping));
      this.replace(built.sequence, null);
      this.savedRevision = this.revision - 1;
      return { snapshot: this.snapshot(), summary: built.summary, notes: built.notes };
    }
    const imported = this.xlightsSequenceImport ?? fail(`Could not read ${path}: no such file`);
    this.replace(structuredClone(imported.sequence), null);
    this.savedRevision = this.revision - 1; // an import has unsaved changes
    return { snapshot: this.snapshot(), summary: { ...imported.summary }, notes: [...imported.notes] };
  }

  async pickXlightsSequencePath() {
    return this.nextXlightsSequencePath;
  }

  /** The open sequence's music file (relative music next to the document), like the engine. */
  private musicPath(): string | null {
    return resolveAudio(this.doc?.audio ?? null, this.path);
  }

  async sequenceMusicMissing(): Promise<MissingFile | null> {
    const music = this.musicPath();
    if (!music || !this.backend?.missingPaths.has(music)) return null;
    return missingFile({ kind: "sequenceDocMusic" }, music, `Music for ${this.open_().name.trim() || "this sequence"}`);
  }

  private setMusic(audio: string) {
    const doc = this.open_();
    return this.editSequence([{ type: "updateInfo", name: doc.name, audio, durationMs: doc.durationMs, frameMs: doc.frameMs }]);
  }

  async findSequenceMusic(): Promise<MusicFound> {
    this.calls.push("findSequenceMusic");
    const missing = await this.sequenceMusicMissing();
    const to = missing ? this.backend?.findable.get(missing.path) : undefined;
    if (!missing || !to) return { found: null, result: null, gaveUp: false };
    const result = await this.setMusic(to);
    return { found: { file: missing.file, name: missing.name, from: missing.path, to, also: [] }, result, gaveUp: false };
  }

  async locateSequenceMusic() {
    this.calls.push("locateSequenceMusic");
    const to = this.backend?.nextLocatePath;
    if (!to) return null;
    if (this.backend?.missingPaths.has(to)) fail(`${fileName(to)} isn't there anymore. Choose another file.`);
    return this.setMusic(to);
  }

  async pickSequenceDocPath() {
    return this.nextOpenPath;
  }

  async pickSequenceDocSavePath(_defaultName: string) {
    return this.nextSavePath;
  }

  async pickExportPath(_defaultName: string) {
    return this.nextSavePath;
  }
}
