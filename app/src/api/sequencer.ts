import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { pickPath } from "./tauri";
import type { FoundFile, MissingFile, PlaybackStatus, ShowSnapshot, XlightsSequenceImported } from "./types";
import type {
  Analysis,
  EffectInfo,
  ExportLayout,
  ExportProgress,
  ExportSummary,
  SequenceEdit,
  SequenceEditResult,
  SequenceRecovery,
  SequenceSnapshot,
  TimingImported,
} from "./sequence";

/** The event the engine sends while exporting. */
export const EXPORT_PROGRESS_EVENT = "sequence-export-progress";

/** What looking for the open sequence's music found: where (now used), and the edit that did it. */
export interface MusicFound {
  found: FoundFile | null;
  result: SequenceEditResult | null;
  /** True when the search stopped before looking everywhere (it took too long). */
  gaveUp: boolean;
}

/** Everything the sequencer asks of the engine. Errors reject with a plain-language message. */
export interface SequencerApi {
  /**
   * Starts a new sequence with `audio` as its music (or none), replacing the open one (ask before
   * discarding changes). It starts with no unsaved changes and nothing to undo.
   */
  newSequenceDoc(name: string, durationMs: number, audio: string | null): Promise<SequenceSnapshot>;
  /** Unsaved sequences an earlier run of PixelFlow kept (newest first). */
  sequenceRecoveries(): Promise<SequenceRecovery[]>;
  /** Opens a kept sequence, with unsaved changes, replacing the open one (ask first). */
  recoverSequence(id: string): Promise<SequenceSnapshot>;
  /** Throws a kept sequence away. */
  discardSequenceRecovery(id: string): Promise<void>;
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
  /** Whether the open sequence plays again from the top each time it reaches the end (switches
   * at once while it plays); the playback state, if it's playing. */
  setSequenceDocLoop(looping: boolean): Promise<PlaybackStatus | null>;
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
  /**
   * Adds the timing tracks in an xLights `.xtiming` file or Audacity labels (`.txt`) after the
   * others (one undo step); a name already taken gets a number. Rejects if another sequence was
   * opened while the file was being read.
   */
  importTimingFile(path: string): Promise<TimingImported>;
  /** Writes a timing track to `.xtiming` (a lyrics track with its words and phonemes) or Audacity
   * labels (any other extension); resolves with how many marks were written. */
  exportTimingTrack(id: string, path: string): Promise<number>;
  pickTimingFilePath(): Promise<string | null>;
  pickTimingExportPath(defaultName: string): Promise<string | null>;
  /** Imports the xLights sequence (.xsq) at `path` onto the open show and opens it as a new,
   * unsaved sequence. It replaces the open sequence without asking: check `getSequenceDoc()`
   * for unsaved changes first (the store's importXlightsSequence does). */
  importXlightsSequence(path: string): Promise<XlightsSequenceImported>;
  /** The open sequence's music, when it isn't where the sequence says; else null. */
  sequenceMusicMissing(): Promise<MissingFile | null>;
  /**
   * Looks for the open sequence's missing music by name in the sequence's folder and the show's
   * (and the folders below them), and uses it when found (one undo step on the sequence).
   */
  findSequenceMusic(): Promise<MusicFound>;
  /** Asks where the open sequence's music is now (a native dialog) and uses it; null when cancelled. */
  locateSequenceMusic(): Promise<SequenceEditResult | null>;
  /** Native dialogs; null when cancelled. */
  pickXlightsSequencePath(): Promise<string | null>;
  pickSequenceDocPath(): Promise<string | null>;
  pickSequenceDocSavePath(defaultName: string): Promise<string | null>;
  pickExportPath(defaultName: string): Promise<string | null>;
}

/** The real engine, in the Tauri desktop shell. */
export const tauriSequencer: SequencerApi = {
  newSequenceDoc: (name, durationMs, audio) => invoke("new_sequence_doc", { name, durationMs, audio }),
  sequenceRecoveries: () => invoke("sequence_recoveries"),
  recoverSequence: (id) => invoke("recover_sequence", { id }),
  discardSequenceRecovery: (id) => invoke("discard_sequence_recovery", { id }),
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
  setSequenceDocLoop: (looping) => invoke("set_sequence_doc_loop", { looping }),
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
  importTimingFile: (path) => invoke("import_timing_file", { path }),
  exportTimingTrack: (id, path) => invoke("export_timing_track", { id, path }),
  pickTimingFilePath: () => pickPath("timingFile"),
  pickTimingExportPath: (defaultName) => pickPath("timingExport", defaultName),
  importXlightsSequence: (path) => invoke("import_xlights_sequence", { path }),
  sequenceMusicMissing: () => invoke("sequence_music_missing"),
  findSequenceMusic: () => invoke("find_sequence_music"),
  locateSequenceMusic: () => invoke("locate_sequence_music"),
  pickXlightsSequencePath: () => pickPath("xlightsSequence"),
  pickSequenceDocPath: () => pickPath("sequenceDoc"),
  pickSequenceDocSavePath: (defaultName) => pickPath("sequenceDocSave", defaultName),
  pickExportPath: (defaultName) => pickPath("fseqExport", defaultName),
};
