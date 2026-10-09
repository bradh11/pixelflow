import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ProviderId } from "./assistant";
import { whileFileDialog } from "./fileDialogs";
import { pickPath } from "./tauri";
import type {
  FoundFile,
  MissingFile,
  PlaybackStatus,
  ShowSnapshot,
  VendorImportOptions,
  VendorInspection,
  VendorMapping,
  XlightsSequenceImported,
  XmapRead,
} from "./types";
import type {
  Analysis,
  EffectInfo,
  ExportLayout,
  ExportProgress,
  ExportSummary,
  Row,
  SequenceEdit,
  SequenceEditResult,
  SequenceRecovery,
  SequenceSnapshot,
  TimingImported,
} from "./sequence";

/** The event the engine sends while exporting. */
export const EXPORT_PROGRESS_EVENT = "sequence-export-progress";

/** The event Find lyrics sends at each step. */
export const LYRICS_PROGRESS_EVENT = "lyrics-progress";

/** Whether Find lyrics can run: only once the assistant is set up (its provider's key is there). */
export interface LyricsGate {
  ready: boolean;
  /** Why not, in one line. */
  reason: string | null;
  /** Whether the song's audio can be sent to OpenAI to hear the words (an OpenAI key). */
  recognizer: boolean;
}

/** Published lyrics that could be the song. */
export interface LyricsCandidate {
  id: number;
  artist: string;
  title: string;
  durationS: number;
  /** Its language's name ("English"), when it can be told. */
  language: string | null;
  synced: boolean;
}

/** Lyrics the user picks instead: another candidate, or pasted text (plain lines or LRC). */
export type LyricsChoice = { candidate: number } | { pasted: string };

/** What Find lyrics added (Lyrics, Lyrics (words), Lyrics (syllables), Lyrics (phonemes), and
 * Vocals, as one undo step). */
export interface LyricsFound {
  result: SequenceEditResult;
  /** Where the words and their timing came from: "Lyrics from LRCLIB, word timing from OpenAI." */
  summary: string;
  /** "Lyrics: Lantern Band — Lantern Song (LRCLIB) · word timing: OpenAI" */
  source: string;
  notes: string[];
  lines: number;
  words: number;
  /** Words whose timing is a guess. */
  unsureWords: number;
  /** The published lyrics that could be the song, best first. */
  candidates: LyricsCandidate[];
  /** The candidate used. */
  chosen: number | null;
  /** Whether pasted lyrics were used. */
  pasted: boolean;
  /** "Word timing locked to the vocals (average shift 120 ms).", when it was. */
  timingNote: string | null;
}

/** What Re-time to vocals did (one undo step), and what to tell the user. */
export interface LyricsRetimed {
  result: SequenceEditResult;
  /** "Word timing locked to the vocals (average shift 40 ms)." */
  note: string;
}

/** How Find lyrics runs: the language it expects when nothing else says (ISO 639-1), and
 * whether to find again without what's kept for the song. */
export interface LyricsOptions {
  language: string;
  fresh: boolean;
}

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
   * Starts a new sequence with `audio` as its music (or none) and `rows` (none when left out),
   * replacing the open one (ask before discarding changes). It starts with no unsaved changes and
   * nothing to undo.
   */
  newSequenceDoc(name: string, durationMs: number, audio: string | null, rows?: Row[]): Promise<SequenceSnapshot>;
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
  /** Whether Find lyrics can run with the assistant's `provider`. The key is only checked for. */
  lyricsGate(provider: ProviderId | null): Promise<LyricsGate>;
  /**
   * Finds the song's lyrics (LRCLIB, and with `upload` and an OpenAI key, OpenAI hearing the song)
   * and adds them as timing tracks (one undo step), calling `onProgress` at each step. Rejects with
   * "Stopped." after cancelLyrics, and with "No lyrics found for this song." when there are none.
   */
  findLyrics(provider: ProviderId | null, upload: boolean, options: LyricsOptions, onProgress?: (label: string) => void): Promise<LyricsFound>;
  /** Lines the lyrics up again with another candidate or pasted lyrics, from what Find lyrics
   * gathered (nothing is looked up or sent), replacing the tracks as one undo step. */
  chooseLyrics(choice: LyricsChoice): Promise<LyricsFound>;
  cancelLyrics(): Promise<void>;
  /**
   * Makes the "<name> (syllables)" and "<name> (phonemes)" tracks again from the words on the
   * words track `track` (one undo step, replacing tracks of those names); nothing is looked up.
   */
  syllablesFromWords(track: string): Promise<SequenceEditResult>;
  /** Moves the lyrics tracks `track` belongs with (its lines, words, syllables, and phonemes) by
   * `ms` (negative: earlier) together, as one undo step. */
  nudgeLyrics(track: string, ms: number): Promise<SequenceEditResult>;
  /** Locks the words already on the lyrics tracks `track` belongs with onto the song's voice
   * again, and makes the lines, syllables, and phonemes again from them (one undo step). Reads
   * only the song file: nothing is looked up or sent. */
  retimeLyrics(track: string): Promise<LyricsRetimed>;
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
   * for unsaved changes first (the store's importXlightsSequence does). With `options` (from
   * inspectXlightsSequence), `path` may be a vendor package (.zip, .xsqz) or folder: effects go
   * where the mapping says, the mapping is remembered for the vendor, and a zip's music is copied
   * next to the show (or into `musicFolder` while the show isn't saved). */
  importXlightsSequence(path: string, options?: VendorImportOptions): Promise<XlightsSequenceImported>;
  /** Looks inside a sequence, vendor package, or folder (its sequence `sequence`, or its best one)
   * and suggests how its models map onto the open show. Reads only. */
  inspectXlightsSequence(path: string, sequence?: string): Promise<VendorInspection>;
  /** Reads an xLights mapping file (.xmap). */
  readXmap(path: string): Promise<XmapRead>;
  /** Saves a mapping as an xLights mapping file (.xmap). */
  writeXmap(path: string, mapping: VendorMapping): Promise<void>;
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
  pickXlightsPackageFolder(): Promise<string | null>;
  pickXmapPath(): Promise<string | null>;
  pickXmapSavePath(defaultName: string): Promise<string | null>;
  pickSequenceDocPath(): Promise<string | null>;
  pickSequenceDocSavePath(defaultName: string): Promise<string | null>;
  pickExportPath(defaultName: string): Promise<string | null>;
}

/** The real engine, in the Tauri desktop shell. */
export const tauriSequencer: SequencerApi = {
  newSequenceDoc: (name, durationMs, audio, rows) => invoke("new_sequence_doc", rows?.length ? { name, durationMs, audio, rows } : { name, durationMs, audio }),
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
  lyricsGate: (provider) => invoke("lyrics_gate", { provider }),
  findLyrics: async (provider, upload, options, onProgress) => {
    const unlisten = onProgress ? await listen<{ label: string }>(LYRICS_PROGRESS_EVENT, (event) => onProgress(event.payload.label)) : null;
    try {
      return await invoke<LyricsFound>("find_lyrics", { provider, upload, language: options.language, fresh: options.fresh });
    } finally {
      unlisten?.();
    }
  },
  chooseLyrics: (choice) => invoke("choose_lyrics", { choice }),
  cancelLyrics: () => invoke("cancel_lyrics"),
  syllablesFromWords: (track) => invoke("syllables_from_words", { track }),
  nudgeLyrics: (track, ms) => invoke("nudge_lyrics", { track, ms: Math.round(ms) }),
  retimeLyrics: (track) => invoke("retime_lyrics", { track }),
  importTimingFile: (path) => invoke("import_timing_file", { path }),
  exportTimingTrack: (id, path) => invoke("export_timing_track", { id, path }),
  pickTimingFilePath: () => pickPath("timingFile"),
  pickTimingExportPath: (defaultName) => pickPath("timingExport", defaultName),
  importXlightsSequence: (path, options) => invoke("import_xlights_sequence", options ? { path, ...options } : { path }),
  inspectXlightsSequence: (path, sequence) => invoke("inspect_xlights_sequence", sequence ? { path, sequence } : { path }),
  readXmap: (path) => invoke("read_xmap", { path }),
  writeXmap: (path, mapping) => invoke("write_xmap", { path, mapping }),
  sequenceMusicMissing: () => invoke("sequence_music_missing"),
  findSequenceMusic: () => invoke("find_sequence_music"),
  locateSequenceMusic: () => whileFileDialog(() => invoke("locate_sequence_music")),
  pickXlightsSequencePath: () => pickPath("xlightsSequence"),
  pickXlightsPackageFolder: () => pickPath("xlightsPackageFolder"),
  pickXmapPath: () => pickPath("xmap"),
  pickXmapSavePath: (defaultName) => pickPath("xmapSave", defaultName),
  pickSequenceDocPath: () => pickPath("sequenceDoc"),
  pickSequenceDocSavePath: (defaultName) => pickPath("sequenceDocSave", defaultName),
  pickExportPath: (defaultName) => pickPath("fseqExport", defaultName),
};
