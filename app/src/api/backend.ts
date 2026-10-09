import type {
  AudioInfo,
  AudioProgress,
  DeviceComparison,
  DeviceDetails,
  RestorePlan,
  RestoreReport,
  SendPlan,
  SendReport,
  UseProps,
  Waveform,
  XlightsImported,
  PlaybackStatus,
  PreviewSet,
  PreviewSet3d,
  FppSequence,
  FppFile,
  FppFolder,
  FppSetupPlan,
  ScheduleEntry,
  FppSoftware,
  FppSendPlan,
  FppDownloadPlan,
  FppDownloadProgress,
  FppDownloadRequest,
  FppDownloadResult,
  FppSendProgress,
  FppSendRequest,
  FppSendResult,
  SendSource,
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
  ControllerCheck,
  BrightnessSample,
  CameraMapFrames,
  CameraMapPlan,
  CameraMapSync,
  CameraMapTargetInfo,
  CodeBase,
  DecodedCapture,
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
  /** The pixels a camera-mapping capture of `target` covers, and how long its sequence runs. */
  cameraMapTarget(target: TargetSpec, base: CodeBase): Promise<CameraMapTargetInfo>;
  /** Finds where the camera-mapping sequence starts in a video, from each frame's brightness. */
  cameraMapSync(samples: BrightnessSample[], pixels: number, base: CodeBase): Promise<CameraMapSync>;
  /** Finds and reads the pixels in the averaged slot frames (raw RGB, one frame per slot). */
  cameraMapDecode(frames: Uint8Array, info: CameraMapFrames): Promise<DecodedCapture>;
  /** Lines a decoded capture up with the layout (by `anchors`, sequence indexes, or every
   * pixel) and works out each prop's points. Changes nothing. */
  cameraMapPlan(target: TargetSpec, pixels: number, decoded: DecodedCapture, anchors: number[]): Promise<CameraMapPlan>;
  /** Finds controllers on the network (plus any typed addresses). Takes a few seconds. */
  /** `network` false checks only the typed hosts (and what FPPs list); true also scans the network. */
  discoverDevices(hosts: string[], network: boolean): Promise<Discovery>;
  /** Reads a device's configuration and previews importing it (changes nothing). */
  inspectDevice(address: string): Promise<DeviceDetails>;
  /** Adds the device as a controller with starter props, as one undo step. `useProps` wires
   * props already in the show to strings instead (by string key, "port1/string2"). */
  importDevice(address: string, useProps?: UseProps): Promise<ShowSnapshot>;
  /** Reads the device and compares it with the show's controller at its address. Changes
   * nothing. */
  compareDevice(address: string): Promise<DeviceComparison>;
  /** Takes the picked differences (by Change id) from the device, as last compared, into the
   * show as one undo step. `useProps` wires existing props to new strings. */
  takeFromDevice(address: string, picks: string[], useProps?: UseProps): Promise<ShowSnapshot>;
  /** Reads the device's setup and plans sending the show's. Changes nothing. */
  planDeviceSetup(address: string): Promise<SendPlan>;
  /** Sends what planDeviceSetup showed (`expected`: the ids of the rows shown). Changes the
   * device: only from the user's Send click. Keeps a copy of its setup first. */
  sendDeviceSetup(address: string, expected: string[]): Promise<SendReport>;
  /** Identifies the device at `address` and says what putting the kept copy of *its* setup back
   * would change. Changes nothing. */
  planDeviceRestore(address: string): Promise<RestorePlan>;
  /** Puts the kept copy back, as planDeviceRestore showed it (`expected`: the ids of the rows
   * shown), only on the device it came from. Changes the device. */
  restoreDeviceSetup(address: string, expected: string[]): Promise<RestoreReport>;
  /** Dismisses a kept copy (by its key): Put back is no longer offered for that device. */
  forgetDeviceSetupCopy(key: string): Promise<void>;
  /** Adds a controller an FPP sends to, from the FPP's output list (works while it's offline). */
  importFppDestination(address: string, destination: string, protocol: string): Promise<ShowSnapshot>;
  /**
   * Whether each controller answers, and whether it's on this computer's network. Changes
   * nothing (a connection to its web port is opened and closed); takes up to a second.
   */
  checkControllers(addresses: string[]): Promise<ControllerCheck[]>;
  /** What an FPP is playing (changes nothing). */
  fppStatus(address: string): Promise<PlayerStatus>;
  /** The sequences stored on an FPP (changes nothing). */
  fppSequences(address: string): Promise<FppSequence[]>;
  /** Starts a playlist or sequence (e.g. "Show.fseq") on an FPP. Only when the user asks. */
  fppStart(address: string, name: string): Promise<void>;
  /** Stops an FPP now, or after the current sequence. Only when the user asks. */
  fppStop(address: string, gracefully: boolean): Promise<void>;
  /** One of an FPP's folders, with each file's length, size, and date (changes nothing). */
  fppFolder(address: string, folder: FppFolder): Promise<FppFile[]>;
  /** An FPP's schedule entries (changes nothing). */
  fppSchedule(address: string): Promise<ScheduleEntry[]>;
  /** What setting up the show from an FPP would add: a controller per output target it sends
   * to, with its channels (and its own outputs, if any). Changes nothing. */
  fppSetupPlan(address: string): Promise<FppSetupPlan>;
  /** Adds what fppSetupPlan showed as one undo step; `expected` is the addresses it showed, and
   * nothing is added if the plan has changed since. */
  fppSetUpShow(address: string, expected: string[]): Promise<ShowSnapshot>;
  /** What an FPP runs and whether a newer release fits it (changes nothing; reads FPP's public
   * release list, kept for hours). */
  fppSoftware(address: string): Promise<FppSoftware>;
  /** Opens a controller's own web page in the system browser (or its About page, where the user
   * upgrades FPP). */
  openDevicePage(address: string, page?: "about.php"): Promise<void>;
  /** The sequence files on an FPP, with ".fseq" (one quick request; changes nothing). */
  fppSequenceNames(address: string): Promise<string[]>;
  /** What sending `source` (and `music`) to an FPP would do: names taken, playlists, free space.
   * Changes nothing. */
  fppSendPlan(address: string, source: SendSource, music: string | null): Promise<FppSendPlan>;
  /**
   * Puts a sequence and its music on an FPP, and on a playlist if chosen, calling `onProgress` as
   * it goes. Rejects with "The upload was cancelled." after cancelFppSend. Changes the FPP: only
   * from the user's Send click. It never starts playback.
   */
  fppSend(address: string, request: FppSendRequest, onProgress?: (progress: FppSendProgress) => void): Promise<FppSendResult>;
  /** Stops the sends running now. */
  cancelFppSend(): Promise<void>;
  /** Shows the shell's folder dialog for where to save a download (while the show isn't
   * saved); null when cancelled. Only a folder picked here can be downloaded into. */
  pickDownloadFolder(): Promise<string | null>;
  /** What downloading `sequence` from an FPP would save, and where: the show's folder, else
   * `folder` (picked with pickDownloadFolder). Changes nothing. */
  fppDownloadPlan(address: string, sequence: string, folder: string | null): Promise<FppDownloadPlan>;
  /**
   * Downloads a sequence and its music from an FPP into the folder's "sequences" and "music"
   * subfolders, calling `onProgress` as it goes. Only reads from the FPP. Rejects with "The
   * download was cancelled. Nothing was saved." after cancelFppDownload; a cancelled or failed
   * download leaves nothing behind.
   */
  fppDownload(address: string, request: FppDownloadRequest, onProgress?: (progress: FppDownloadProgress) => void): Promise<FppDownloadResult>;
  /** Stops the downloads running now. */
  cancelFppDownload(): Promise<void>;
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
  /** Adds the sequence file at `path` to the show with `music`, else the music found next to it
   * (one undo step). */
  addSequence(path: string, music?: string | null): Promise<ShowSnapshot>;
  /** Plays one of the show's sequences with its music. */
  playSequence(id: string, positionMs: number): Promise<PlaybackStatus>;
  /** Music volume (0–1) for playback. */
  setPlaybackVolume(volume: number): Promise<PlaybackStatus | null>;
  /** A music file's loudness over time, in `slices` slices (progress goes to onAudioProgress). */
  audioWaveform(path: string, slices: number): Promise<Waveform>;
  /**
   * How long a music file plays, from what it says about itself: quick, except for a file that
   * doesn't say, which is read through, calling `onProgress` as it goes.
   */
  probeAudio(path: string, onProgress?: (progress: AudioProgress) => void): Promise<AudioInfo>;
  /**
   * Calls `handler` as long work on music files gets along (waveforms, the audio track effects
   * follow, beats), about ten times a second at most; resolves with a function that stops
   * listening.
   */
  onAudioProgress(handler: (progress: AudioProgress) => void): Promise<() => void>;
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
