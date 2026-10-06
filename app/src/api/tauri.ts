import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Backend } from "./backend";
import { whileFileDialog } from "./fileDialogs";
import { decodePreview, decodePreview3d } from "./previewBytes";
import type { FppSendProgress, FppSendResult, MenuAction, PickKind } from "./types";

/** The event the shell sends when a File menu item is chosen in the menu bar. */
const MENU_EVENT = "menu";
/** The event a send to an FPP reports its progress with. */
const FPP_SEND_PROGRESS_EVENT = "fpp-send-progress";

/**
 * Shows a native file dialog of `kind` (a save dialog suggests `name`). The shell shows it as a
 * sheet starting in a sensible folder, and answers with lossless path text; null when cancelled.
 */
export function pickPath(kind: PickKind, name?: string): Promise<string | null> {
  return whileFileDialog(() => invoke("pick_path", name === undefined ? { kind } : { kind, name }));
}

/** The real engine, running in the Tauri desktop shell. */
export const tauriBackend: Backend = {
  getSnapshot: () => invoke("get_snapshot"),
  applyEdits: (edits) => invoke("apply_edits", { edits }),
  undo: () => invoke("undo"),
  redo: () => invoke("redo"),
  newShow: (name) => invoke("new_show", { name }),
  openSampleShow: () => invoke("open_sample_show"),
  openShow: (path) => invoke("open_show", { path }),
  saveShow: () => invoke("save_show"),
  saveShowAs: (path) => invoke("save_show_as", { path }),
  checkFiles: (all) => invoke("check_files", { all }),
  findMissingFiles: (file) => invoke("find_missing_files", file ? { file } : {}),
  // The shell asks where the file is with its own dialog: the window never names the new place.
  locateFile: (file) => whileFileDialog(() => invoke("locate_file", { file })),
  listHistory: () => invoke("list_history"),
  restoreHistory: (id) => invoke("restore_history", { id }),
  startOutput: (pattern, target) => invoke("start_output", { pattern, target }),
  stopOutput: () => invoke("stop_output"),
  outputStatus: () => invoke("output_status"),
  discoverDevices: (hosts, network) => invoke("discover_devices", { hosts, network }),
  inspectDevice: (address) => invoke("inspect_device", { address }),
  importDevice: (address) => invoke("import_device", { address }),
  importFppDestination: (address, destination, protocol) =>
    invoke("import_fpp_destination", { address, destination, protocol }),
  fppStatus: (address) => invoke("fpp_status", { address }),
  fppSequences: (address) => invoke("fpp_sequences", { address }),
  fppStart: (address, name) => invoke("fpp_start", { address, name }),
  fppStop: (address, gracefully) => invoke("fpp_stop", { address, gracefully }),
  fppSequenceNames: (address) => invoke("fpp_sequence_names", { address }),
  fppSendPlan: (address, source, music) => invoke("fpp_send_plan", { address, source, music }),
  fppSend: async (address, request, onProgress) => {
    const unlisten = onProgress
      ? await listen<FppSendProgress>(FPP_SEND_PROGRESS_EVENT, (event) => {
          if (event.payload.sendId === request.sendId) onProgress(event.payload);
        })
      : null;
    try {
      return await invoke<FppSendResult>("fpp_send", { address, request });
    } finally {
      unlisten?.();
    }
  },
  cancelFppSend: () => invoke("cancel_fpp_send"),
  startPlayback: (path, positionMs) => invoke("start_playback", { path, positionMs }),
  pausePlayback: (paused) => invoke("pause_playback", { paused }),
  seekPlayback: (positionMs) => invoke("seek_playback", { positionMs }),
  stopPlayback: () => invoke("stop_playback"),
  playbackStatus: () => invoke("playback_status"),
  playbackStopReason: () => invoke("playback_stop_reason"),
  liveFrame: async () => new Uint8Array(await invoke<ArrayBuffer>("live_frame")),
  sequenceFrame: async () => new Uint8Array(await invoke<ArrayBuffer>("sequence_frame")),
  previewProps: async () => decodePreview(await invoke<ArrayBuffer | number[]>("preview_props")),
  previewProps3d: async () => decodePreview3d(await invoke<ArrayBuffer | number[]>("preview_props_3d")),
  readImage: async (path) => new Uint8Array(await invoke<ArrayBuffer>("read_image", { path })),
  // Picked by the shell, which then lets the window read that photo (and no other files).
  pickImagePath: () => whileFileDialog(() => invoke("pick_image")),
  readHouseModel: async (path) => new Uint8Array(await invoke<ArrayBuffer>("read_house_model", { path })),
  // Picked by the shell, which then lets the window read that model (and no other files).
  pickHouseModelPath: () => whileFileDialog(() => invoke("pick_house_model")),
  importXlights: (folder) => invoke("import_xlights", { folder }),
  pickShowFolder: () => pickPath("xlightsFolder"),
  addSequence: (path) => invoke("add_sequence", { path }),
  playSequence: (id, positionMs) => invoke("play_sequence", { id, positionMs }),
  setPlaybackVolume: (volume) => invoke("set_playback_volume", { volume }),
  audioWaveform: (path, slices) => invoke("audio_waveform", { path, slices }),
  pickAudioPath: () => pickPath("music"),
  pickSequencePath: () => pickPath("fseq"),
  pickOpenPath: () => pickPath("show"),
  pickSavePath: (defaultName) => pickPath("showSave", defaultName),
  onCloseRequested: (allow) =>
    getCurrentWindow().onCloseRequested((event) => {
      if (!allow()) event.preventDefault();
    }),
  closeWindow: () => getCurrentWindow().destroy(),
  listRecentShows: () => invoke("list_recent_shows"),
  forgetRecentShow: (path) => invoke("forget_recent_show", { path }),
  clearRecentShows: () => invoke("clear_recent_shows"),
  // The shell asks where it is with its own dialog, and opens it like any other show.
  locateRecentShow: (path) => whileFileDialog(() => invoke("locate_recent_show", { path })),
  onMenu: (handler) => listen<MenuAction>(MENU_EVENT, (event) => handler(event.payload)),
};

/** True when running inside the Tauri shell (not a plain browser). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
