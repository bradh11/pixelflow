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
import type { MusicFound, SequencerApi } from "./sequencer";
import * as marks from "./timingMarks";
import { formatMs } from "./timingMarks";

/** The engine's effect catalog (a copy of the Rust table). */
export const EFFECT_CATALOG = catalogJson as unknown as EffectInfo[];

const NO_SEQUENCE = "No sequence is open. Create or open one first.";

function fail(message: string): never {
  throw new Error(message);
}

export { formatMs };

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
      track.marks.splice(edit.index, 1);
      track.marks.splice(marks.insertIndex(track.marks, edit.mark.startMs), 0, { ...edit.mark });
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
  if (track.kind === "phonemes") fail("Phoneme tracks come from xLights and can't be edited here; edit the words instead.");
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
  return { schemaVersion: 5, name, audio: null, durationMs, frameMs: 25, timingTracks: [], rows: [] };
}

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
  /** How long beat detection takes. */
  analysisDelayMs = 0;

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
    return { durationMs, tempoBpm: 120, beats, bars: beats.filter((_, i) => i % 4 === 0), onsets: beats };
  }

  async detectBeats() {
    const doc = this.open_();
    const docId = this.docId;
    if (!doc.audio) fail("This sequence has no music yet. Choose a song for it first.");
    const analysis = await this.reply(await this.analyzeAudio(doc.audio), this.analysisDelayMs);
    if (this.docId !== docId || this.doc?.audio !== doc.audio) {
      fail("The sequence or its music changed while the beats were being found. Run beat detection again.");
    }
    const latest = this.open_();
    const marks = (times: number[], label: (i: number) => string) =>
      times.map((t, i) => ({ startMs: t, endMs: times[i + 1] ?? doc.durationMs, label: label(i) }));
    const tracks: TimingTrack[] = [
      { id: crypto.randomUUID(), name: "Beats", kind: "beats", marks: marks(analysis.beats, (i) => String((i % 4) + 1)) },
      { id: crypto.randomUUID(), name: "Bars", kind: "bars", marks: marks(analysis.bars, (i) => String(i + 1)) },
    ];
    const edits: SequenceEdit[] = [
      ...latest.timingTracks
        .filter((t) => tracks.some((n) => n.name === t.name))
        .map((t) => ({ type: "removeTimingTrack" as const, id: t.id })),
      ...tracks.map((track) => ({ type: "addTimingTrack" as const, track })),
    ];
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
