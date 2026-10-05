import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Backend } from "./backend";
import { decodePreview, decodePreview3d } from "./previewBytes";

const SHOW_FILTER = [{ name: "PixelFlow show", extensions: ["json"] }];
const SEQUENCE_FILTER = [{ name: "FPP sequence", extensions: ["fseq"] }];
const AUDIO_FILTER = [{ name: "Music", extensions: ["mp3", "m4a", "wav", "ogg", "flac"] }];

/** The real engine, running in the Tauri desktop shell. */
export const tauriBackend: Backend = {
  getSnapshot: () => invoke("get_snapshot"),
  applyEdits: (edits) => invoke("apply_edits", { edits }),
  undo: () => invoke("undo"),
  redo: () => invoke("redo"),
  newShow: (name) => invoke("new_show", { name }),
  openShow: (path) => invoke("open_show", { path }),
  saveShow: () => invoke("save_show"),
  saveShowAs: (path) => invoke("save_show_as", { path }),
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
  pickImagePath: () => invoke("pick_image"),
  readHouseModel: async (path) => new Uint8Array(await invoke<ArrayBuffer>("read_house_model", { path })),
  // Picked by the shell, which then lets the window read that model (and no other files).
  pickHouseModelPath: () => invoke("pick_house_model"),
  importXlights: (folder) => invoke("import_xlights", { folder }),
  pickShowFolder: async () => {
    const path = await open({ multiple: false, directory: true, title: "Choose your xLights show folder" });
    return typeof path === "string" ? path : null;
  },
  addSequence: (path) => invoke("add_sequence", { path }),
  playSequence: (id, positionMs) => invoke("play_sequence", { id, positionMs }),
  setPlaybackVolume: (volume) => invoke("set_playback_volume", { volume }),
  audioWaveform: (path, slices) => invoke("audio_waveform", { path, slices }),
  pickAudioPath: async () => {
    const path = await open({ multiple: false, directory: false, filters: AUDIO_FILTER });
    return typeof path === "string" ? path : null;
  },
  pickSequencePath: async () => {
    const path = await open({ multiple: false, directory: false, filters: SEQUENCE_FILTER });
    return typeof path === "string" ? path : null;
  },
  pickOpenPath: async () => {
    const path = await open({ multiple: false, directory: false, filters: SHOW_FILTER });
    return typeof path === "string" ? path : null;
  },
  pickSavePath: async (defaultName) => (await save({ defaultPath: defaultName, filters: SHOW_FILTER })) ?? null,
  onCloseRequested: (allow) =>
    getCurrentWindow().onCloseRequested((event) => {
      if (!allow()) event.preventDefault();
    }),
  closeWindow: () => getCurrentWindow().destroy(),
};

/** True when running inside the Tauri shell (not a plain browser). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
