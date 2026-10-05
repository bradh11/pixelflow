import type {
  DeviceDetails,
  Waveform,
  XlightsImported,
  PlaybackStatus,
  PreviewProp,
  FppSequence,
  PlayerStatus,
  Discovery,
  Edit,
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
  openShow(path: string): Promise<ShowSnapshot>;
  saveShow(): Promise<ShowSnapshot>;
  saveShowAs(path: string): Promise<ShowSnapshot>;
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
  /** Every prop's pixel positions for the 2D preview. */
  previewProps(): Promise<PreviewProp[]>;
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
}

/** Turns anything thrown by a backend call into a message for the user. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Something went wrong.";
}
