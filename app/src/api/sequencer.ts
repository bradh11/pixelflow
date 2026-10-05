import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { PlaybackStatus } from "./types";
import type { Analysis, ExportLayout, ExportSummary, SequenceEdit, SequenceSnapshot } from "./sequence";

const DOCUMENT_FILTER = [{ name: "PixelFlow sequence", extensions: ["json"] }];
const FSEQ_FILTER = [{ name: "FPP sequence", extensions: ["fseq"] }];

/** Everything the sequencer asks of the engine. Errors reject with a plain-language message. */
export interface SequencerApi {
  /** Starts a new, unsaved sequence, replacing the open one (ask before discarding changes). */
  newSequenceDoc(name: string, durationMs: number): Promise<SequenceSnapshot>;
  openSequenceDoc(path: string): Promise<SequenceSnapshot>;
  saveSequenceDoc(): Promise<SequenceSnapshot>;
  saveSequenceDocAs(path: string): Promise<SequenceSnapshot>;
  closeSequenceDoc(): Promise<void>;
  /** The open sequence, or null. */
  getSequenceDoc(): Promise<SequenceSnapshot | null>;
  /** Applies edits as one undo step; a playing sequence shows them from its next frame. */
  editSequence(edits: SequenceEdit[]): Promise<SequenceSnapshot>;
  undoSequence(): Promise<SequenceSnapshot>;
  redoSequence(): Promise<SequenceSnapshot>;
  /** The sequence at `positionMs` as show frame bytes (same layout as liveFrame), for scrubbing. */
  sequenceDocFrame(positionMs: number): Promise<Uint8Array>;
  /** Plays the open sequence live with its music; control it with the playback commands. */
  playSequenceDoc(positionMs: number): Promise<PlaybackStatus>;
  /** How an export would lay out the controllers' channels. */
  sequenceExportLayout(): Promise<ExportLayout>;
  /** Renders the sequence to an `.fseq` file for FPP. */
  exportSequenceDoc(path: string): Promise<ExportSummary>;
  /** Tempo, beats, bars, and onsets in a music file. */
  analyzeAudio(path: string): Promise<Analysis>;
  /** Adds Beats, Bars, and Onsets timing tracks from the sequence's music (one undo step). */
  detectBeats(): Promise<SequenceSnapshot>;
  /** Native dialogs; null when cancelled. */
  pickSequenceDocPath(): Promise<string | null>;
  pickSequenceDocSavePath(defaultName: string): Promise<string | null>;
  pickExportPath(defaultName: string): Promise<string | null>;
}

/** The real engine, in the Tauri desktop shell. */
export const tauriSequencer: SequencerApi = {
  newSequenceDoc: (name, durationMs) => invoke("new_sequence_doc", { name, durationMs }),
  openSequenceDoc: (path) => invoke("open_sequence_doc", { path }),
  saveSequenceDoc: () => invoke("save_sequence_doc"),
  saveSequenceDocAs: (path) => invoke("save_sequence_doc_as", { path }),
  closeSequenceDoc: () => invoke("close_sequence_doc"),
  getSequenceDoc: () => invoke("get_sequence_doc"),
  editSequence: (edits) => invoke("edit_sequence", { edits }),
  undoSequence: () => invoke("undo_sequence"),
  redoSequence: () => invoke("redo_sequence"),
  sequenceDocFrame: async (positionMs) =>
    new Uint8Array(await invoke<ArrayBuffer>("sequence_doc_frame", { positionMs })),
  playSequenceDoc: (positionMs) => invoke("play_sequence_doc", { positionMs }),
  sequenceExportLayout: () => invoke("sequence_export_layout"),
  exportSequenceDoc: (path) => invoke("export_sequence_doc", { path }),
  analyzeAudio: (path) => invoke("analyze_audio", { path }),
  detectBeats: () => invoke("detect_beats"),
  pickSequenceDocPath: async () => {
    const path = await open({ multiple: false, directory: false, filters: DOCUMENT_FILTER });
    return typeof path === "string" ? path : null;
  },
  pickSequenceDocSavePath: async (defaultName) =>
    (await save({ defaultPath: defaultName, filters: DOCUMENT_FILTER })) ?? null,
  pickExportPath: async (defaultName) => (await save({ defaultPath: defaultName, filters: FSEQ_FILTER })) ?? null,
};
