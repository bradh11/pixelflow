import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { PlaybackStatus, ShowSnapshot, XlightsSequenceImported } from "./types";
import type {
  Analysis,
  EffectInfo,
  ExportLayout,
  ExportProgress,
  ExportSummary,
  SequenceEdit,
  SequenceEditResult,
  SequenceSnapshot,
} from "./sequence";

/** The event the engine sends while exporting. */
export const EXPORT_PROGRESS_EVENT = "sequence-export-progress";

const DOCUMENT_FILTER = [{ name: "PixelFlow sequence", extensions: ["json"] }];
const FSEQ_FILTER = [{ name: "FPP sequence", extensions: ["fseq"] }];
const XSQ_FILTER = [{ name: "xLights sequence", extensions: ["xsq"] }];

/** Everything the sequencer asks of the engine. Errors reject with a plain-language message. */
export interface SequencerApi {
  /** Starts a new, unsaved sequence, replacing the open one (ask before discarding changes). */
  newSequenceDoc(name: string, durationMs: number): Promise<SequenceSnapshot>;
  openSequenceDoc(path: string): Promise<SequenceSnapshot>;
  saveSequenceDoc(): Promise<SequenceSnapshot>;
  saveSequenceDocAs(path: string): Promise<SequenceSnapshot>;
  closeSequenceDoc(): Promise<void>;
  /** The whole open sequence, or null: for opening a screen or a full resync. */
  getSequenceDoc(): Promise<SequenceSnapshot | null>;
  /**
   * Applies edits as one undo step; a playing sequence shows them from its next frame. Edits with
   * the same `gesture` id as the previous edit (one per pointer move in a drag) merge into its undo
   * step. The reply lists what changed (see applySequenceChanges), not the whole document.
   */
  editSequence(edits: SequenceEdit[], gesture?: string): Promise<SequenceEditResult>;
  undoSequence(): Promise<SequenceEditResult>;
  redoSequence(): Promise<SequenceEditResult>;
  /** Every effect kind with its settings' labels, ranges, defaults, and choices. */
  effectCatalog(): Promise<EffectInfo[]>;
  /** The sequence at `positionMs` as show frame bytes (same layout as liveFrame), for scrubbing. */
  sequenceDocFrame(positionMs: number): Promise<Uint8Array>;
  /** Plays the open sequence live with its music; control it with the playback commands. */
  playSequenceDoc(positionMs: number): Promise<PlaybackStatus>;
  /** Whether a playing sequence goes out to the controllers (true) or only to the preview. */
  setSequenceDocOutput(send: boolean): Promise<PlaybackStatus | null>;
  /** Adds an exported `.fseq` of the open sequence to the show's playlist (one undo step on the show). */
  addSequenceDocToShow(path: string): Promise<ShowSnapshot>;
  /** How an export would lay out the controllers' channels. */
  sequenceExportLayout(): Promise<ExportLayout>;
  /**
   * Renders the sequence to an `.fseq` file for FPP, calling `onProgress` about once per percent.
   * Rejects with "The export was cancelled." after cancelSequenceExport (no file is written).
   */
  exportSequenceDoc(path: string, onProgress?: (progress: ExportProgress) => void): Promise<ExportSummary>;
  /** Stops the exports running now. */
  cancelSequenceExport(): Promise<void>;
  /** Tempo, beats, bars, and onsets in a music file. */
  analyzeAudio(path: string): Promise<Analysis>;
  /**
   * Adds Beats, Bars, and Onsets timing tracks from the sequence's music (one undo step). Rejects if
   * another sequence was opened, or the music changed, while the beats were being found.
   */
  detectBeats(): Promise<SequenceEditResult>;
  /** Imports the xLights sequence (.xsq) at `path` onto the open show and opens it as a new,
   * unsaved sequence, replacing the open one (ask before discarding changes). */
  importXlightsSequence(path: string): Promise<XlightsSequenceImported>;
  /** Native dialogs; null when cancelled. */
  pickXlightsSequencePath(): Promise<string | null>;
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
  editSequence: (edits, gesture) => invoke("edit_sequence", gesture === undefined ? { edits } : { edits, gesture }),
  undoSequence: () => invoke("undo_sequence"),
  redoSequence: () => invoke("redo_sequence"),
  effectCatalog: () => invoke("effect_catalog"),
  sequenceDocFrame: async (positionMs) =>
    new Uint8Array(await invoke<ArrayBuffer>("sequence_doc_frame", { positionMs })),
  playSequenceDoc: (positionMs) => invoke("play_sequence_doc", { positionMs }),
  setSequenceDocOutput: (send) => invoke("set_sequence_doc_output", { send }),
  addSequenceDocToShow: (path) => invoke("add_sequence_doc_to_show", { path }),
  sequenceExportLayout: () => invoke("sequence_export_layout"),
  exportSequenceDoc: async (path, onProgress) => {
    const unlisten = onProgress
      ? await listen<ExportProgress>(EXPORT_PROGRESS_EVENT, (event) => {
          if (event.payload.path === path) onProgress(event.payload);
        })
      : null;
    try {
      return await invoke<ExportSummary>("export_sequence_doc", { path });
    } finally {
      unlisten?.();
    }
  },
  cancelSequenceExport: () => invoke("cancel_sequence_export"),
  analyzeAudio: (path) => invoke("analyze_audio", { path }),
  detectBeats: () => invoke("detect_beats"),
  importXlightsSequence: (path) => invoke("import_xlights_sequence", { path }),
  pickXlightsSequencePath: async () => {
    const path = await open({ multiple: false, directory: false, filters: XSQ_FILTER });
    return typeof path === "string" ? path : null;
  },
  pickSequenceDocPath: async () => {
    const path = await open({ multiple: false, directory: false, filters: DOCUMENT_FILTER });
    return typeof path === "string" ? path : null;
  },
  pickSequenceDocSavePath: async (defaultName) =>
    (await save({ defaultPath: defaultName, filters: DOCUMENT_FILTER })) ?? null,
  pickExportPath: async (defaultName) => (await save({ defaultPath: defaultName, filters: FSEQ_FILTER })) ?? null,
};
