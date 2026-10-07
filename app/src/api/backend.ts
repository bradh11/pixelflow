import type {
  DeviceDetails,
  Waveform,
  XlightsImported,
  PlaybackStatus,
  PreviewSet,
  PreviewSet3d,
  FppSequence,
  PlayerStatus,
  Discovery,
  Edit,
  FileRole,
  MenuAction,
  RecentShow,
  FilesFound,
  HistoryEntry,
  OutputStatus,
  PatternSpec,
  ShowSnapshot,
  TargetSpec,
} from "./types";

/** Everything the UI asks of the engine. Errors reject with a plain-language message. */
export interface Backend {
  getSnapshot(): Promise<ShowSnapshot>;
  applyEdits(edits: Edit[]): Promise<ShowSnapshot>;
  undo(): Promise<ShowSnapshot>;
  redo(): Promise<ShowSnapshot>;
  newShow(name: string): Promise<ShowSnapshot>;
  /** Opens the demo show built into the app as a new, unsaved show (saving asks where). */
  openSampleShow(): Promise<ShowSnapshot>;
  openShow(path: string): Promise<ShowSnapshot>;
  saveShow(): Promise<ShowSnapshot>;
  saveShowAs(path: string): Promise<ShowSnapshot>;
  /**
   * Looks for the show's missing files (or only `file`) by name in the show's folder and the
   * folders below it, and points the show at what it finds (one undo step). Rejects with a plain
   * message when the show hasn't been saved (it has no folder yet).
   */
  findMissingFiles(file?: FileRole): Promise<FilesFound>;
  /**
   * Looks at whether the show's files are there (those not looked at yet, or `all` of them) and
   * resolves with the show. It may take a while (a slow network drive); edits don't wait for it.
   */
  checkFiles(all: boolean): Promise<ShowSnapshot>;
  /**
   * Looks at whether the show's files are there (those not looked at yet, or `all` of them) and
   * resolves with the show. It may take a while (a slow network drive); edits don't wait for it.
   */
  checkFiles(all: boolean): Promise<ShowSnapshot>;
  /** Asks where a file is now (a native dialog) and points the show at it (one undo step); null when cancelled. */
  locateFile(file: FileRole): Promise<ShowSnapshot | null>;
  listHistory(): Promise<HistoryEntry[]>;
  restoreHistory(id: string): Promise<ShowSnapshot>;
  startOutput(pattern: PatternSpec, target: TargetSpec): Promise<OutputStatus>;
  stopOutput(): Promise<OutputStatus>;
  outputStatus(): Promise<OutputStatus>;
  /** Finds controllers on the network (plus any typed addresses). Takes a few seconds. */
  /** `network` false checks only the typed hosts (and what FPPs list); true also scans the network. */
  discoverDevices(hosts: string[], network: boolean): Promise<Discovery>;
  /** Reads a device's configuration and previews importing it (changes nothing). */
  inspectDevice(address: string): Promise<DeviceDetails>;
  /** Adds the device as a controller with starter props, as one undo step. */
  importDevice(address: string): Promise<ShowSnapshot>;
  /** Adds a controller an FPP sends to, from the FPP's output list (works while it's offline). */
  importFppDestination(address: string, destination: string, protocol: string): Promise<ShowSnapshot>;
  /** What an FPP is playing (changes nothing). */
  fppStatus(address: string): Promise<PlayerStatus>;
  /** The sequences stored on an FPP (changes nothing). */
  fppSequences(address: string): Promise<FppSequence[]>;
  /** Starts a playlist or sequence (e.g. "Show.fseq") on an FPP. Only when the user asks. */
  fppStart(address: string, name: string): Promise<void>;
  /** Stops an FPP now, or after the current sequence. Only when the user asks. */
  fppStop(address: string, gracefully: boolean): Promise<void>;
  /** Plays a rendered sequence (.fseq) to the controllers that know their sequence channels. */
  startPlayback(path: string, positionMs: number): Promise<PlaybackStatus>;
  pausePlayback(paused: boolean): Promise<PlaybackStatus | null>;
  seekPlayback(positionMs: number): Promise<PlaybackStatus | null>;
  stopPlayback(): Promise<void>;
  /** The playing sequence, or null when nothing is playing. */
  playbackStatus(): Promise<PlaybackStatus | null>;
  /** Why playback was stopped by an edit to the show (a plain sentence), or null. */
  playbackStopReason(): Promise<string | null>;
  /** The props' current colors (show frame bytes); empty when nothing is playing or testing. */
  liveFrame(): Promise<Uint8Array>;
  /** The playing sequence's current frame (every channel, as sent); empty when nothing plays. */
  sequenceFrame(): Promise<Uint8Array>;
  /** Every prop's pixel positions for the 2D preview, and the show revision they're for. */
  previewProps(): Promise<PreviewSet>;
  /** Every prop's pixel positions in 3D, and the show revision they're for. */
  previewProps3d(): Promise<PreviewSet3d>;
  /** The bytes of an image file (the layout's background photo). */
  readImage(path: string): Promise<Uint8Array>;
  /** Shows a native "choose photo" dialog; null when cancelled. */
  pickImagePath(): Promise<string | null>;
  /** The bytes of a 3D model file (the house model). */
  readHouseModel(path: string): Promise<Uint8Array>;
  /** Shows a native "choose 3D model" dialog; null when cancelled. */
  pickHouseModelPath(): Promise<string | null>;
  /** Imports the xLights show in `folder` as a new, unsaved show. */
  importXlights(folder: string): Promise<XlightsImported>;
  /** Shows a native folder picker for an xLights show folder; null when cancelled. */
  pickShowFolder(): Promise<string | null>;
  /** Adds the sequence file at `path` to the show, finding its music next to it (one undo step). */
  addSequence(path: string): Promise<ShowSnapshot>;
  /** Plays one of the show's sequences with its music. */
  playSequence(id: string, positionMs: number): Promise<PlaybackStatus>;
  /** Music volume (0–1) for playback. */
  setPlaybackVolume(volume: number): Promise<PlaybackStatus | null>;
  /** A music file's loudness over time, in `slices` slices. */
  audioWaveform(path: string, slices: number): Promise<Waveform>;
  /** Shows a native "choose music" dialog; null when cancelled. */
  pickAudioPath(): Promise<string | null>;
  /** Shows a native "open sequence" dialog; null when cancelled. */
  pickSequencePath(): Promise<string | null>;
  /** Shows a native "open file" dialog; null when cancelled. */
  pickOpenPath(): Promise<string | null>;
  /** Shows a native "save file" dialog; null when cancelled. */
  pickSavePath(defaultName: string): Promise<string | null>;
  /**
   * Calls `allow` when the window is asked to close; when it answers false the window stays open
   * (the app then asks about unsaved work, and calls closeWindow once that's settled). Resolves with
   * a function that stops listening.
   */
  onCloseRequested(allow: () => boolean): Promise<() => void>;
  /** Closes the window for good, without asking again. */
  closeWindow(): Promise<void>;
  /**
   * Shows opened or saved lately, newest first, each saying whether its file is still there.
   * Only opening, saving, and restoring a show put it on the list.
   */
  listRecentShows(): Promise<RecentShow[]>;
  /** Takes a show off the recent list (the file is left alone). */
  forgetRecentShow(path: string): Promise<void>;
  clearRecentShows(): Promise<void>;
  /**
   * For a recent show that isn't where it was: asks where it is now (a native dialog), opens
   * it, and lists it in place of the old entry. Null when cancelled.
   */
  locateRecentShow(path: string): Promise<ShowSnapshot | null>;
  /** Calls `handler` for each File menu item chosen in the menu bar; resolves with a function
   * that stops listening. */
  onMenu(handler: (action: MenuAction) => void): Promise<() => void>;
}

/** Turns anything thrown by a backend call into a message for the user. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Something went wrong.";
}
