import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Backend } from "./backend";
import { whileFileDialog } from "./fileDialogs";
import { decodePreview, decodePreview3d } from "./previewBytes";
import type { AudioInfo, AudioProgress, FppDownloadProgress, FppDownloadResult, FppSendProgress, FppSendResult, MenuAction, PickKind } from "./types";

/** The event the shell sends when a File menu item is chosen in the menu bar. */
const MENU_EVENT = "menu";
/** The event a send to an FPP reports its progress with. */
const FPP_SEND_PROGRESS_EVENT = "fpp-send-progress";
/** The event a download from an FPP reports its progress with. */
const FPP_DOWNLOAD_PROGRESS_EVENT = "fpp-download-progress";
/** The event long work on a music file reports its progress with. */
export const AUDIO_PROGRESS_EVENT = "audio-progress";

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
  cameraMapTarget: (target, base) => invoke("camera_map_target", { target, base }),
  cameraMapSync: (samples, pixels, base) => invoke("camera_map_sync", { samples, pixels, base }),
  // Raw bytes (tens of megabytes), described by a header.
  cameraMapDecode: (frames, info) => invoke("camera_map_decode", frames, { headers: { "x-camera-map": JSON.stringify(info) } }),
  cameraMapPlan: (target, pixels, decoded, anchors) => invoke("camera_map_plan", { target, pixels, decoded, anchors }),
  discoverDevices: (hosts, network) => invoke("discover_devices", { hosts, network }),
  inspectDevice: (address) => invoke("inspect_device", { address }),
  importDevice: (address, useProps) => invoke("import_device", { address, useProps: useProps ?? null }),
  compareDevice: (address) => invoke("compare_device", { address }),
  takeFromDevice: (address, picks, useProps) => invoke("take_from_device_setup", { address, picks, useProps: useProps ?? null }),
  planDeviceSetup: (address) => invoke("plan_device_setup", { address }),
  sendDeviceSetup: (address, expected) => invoke("send_device_setup", { address, expected }),
  planDeviceRestore: (address) => invoke("plan_device_restore", { address }),
  restoreDeviceSetup: (address, expected) => invoke("restore_device_setup", { address, expected }),
  forgetDeviceSetupCopy: (key) => invoke("forget_device_setup_copy", { key }),
  importFppDestination: (address, destination, protocol) =>
    invoke("import_fpp_destination", { address, destination, protocol }),
  fppStatus: (address) => invoke("fpp_status", { address }),
  checkControllers: (addresses) => invoke("check_controllers", { addresses }),
  fppSequences: (address) => invoke("fpp_sequences", { address }),
  fppStart: (address, name) => invoke("fpp_start", { address, name }),
  fppStop: (address, gracefully) => invoke("fpp_stop", { address, gracefully }),
  fppFolder: (address, folder) => invoke("fpp_files", { address, folder }),
  fppSchedule: (address) => invoke("fpp_schedule", { address }),
  fppSetupPlan: (address) => invoke("fpp_setup_plan", { address }),
  fppSetUpShow: (address, expected) => invoke("fpp_set_up_show", { address, expected }),
  fppSoftware: (address) => invoke("fpp_software", { address }),
  openDevicePage: (address, page) => invoke("open_device_page", { address, page: page ?? null }),
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
  // Picked by the shell, which then lets downloads be saved in that folder (and no other).
  pickDownloadFolder: () => pickPath("downloadFolder"),
  fppDownloadPlan: (address, sequence, folder) => invoke("fpp_download_plan", { address, sequence, folder }),
  fppDownload: async (address, request, onProgress) => {
    const unlisten = onProgress
      ? await listen<FppDownloadProgress>(FPP_DOWNLOAD_PROGRESS_EVENT, (event) => {
          if (event.payload.downloadId === request.downloadId) onProgress(event.payload);
        })
      : null;
    try {
      return await invoke<FppDownloadResult>("fpp_download", { address, request });
    } finally {
      unlisten?.();
    }
  },
  cancelFppDownload: () => invoke("cancel_fpp_download"),
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
  addSequence: (path, music) => invoke("add_sequence", music ? { path, music } : { path }),
  playSequence: (id, positionMs) => invoke("play_sequence", { id, positionMs }),
  setPlaybackVolume: (volume) => invoke("set_playback_volume", { volume }),
  audioWaveform: (path, slices) => invoke("audio_waveform", { path, slices }),
  probeAudio: async (path, onProgress) => {
    const unlisten = onProgress
      ? await listen<AudioProgress>(AUDIO_PROGRESS_EVENT, (event) => {
          if (event.payload.task === "probe" && event.payload.path === path) onProgress(event.payload);
        })
      : null;
    try {
      return await invoke<AudioInfo>("probe_audio", { path });
    } finally {
      unlisten?.();
    }
  },
  onAudioProgress: (handler) => listen<AudioProgress>(AUDIO_PROGRESS_EVENT, (event) => handler(event.payload)),
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
