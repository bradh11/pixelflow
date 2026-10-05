import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Backend } from "./backend";

const SHOW_FILTER = [{ name: "PixelFlow show", extensions: ["json"] }];
const SEQUENCE_FILTER = [{ name: "FPP sequence", extensions: ["fseq"] }];

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
  previewProps: () => invoke("preview_props"),
  importXlights: (folder) => invoke("import_xlights", { folder }),
  pickShowFolder: async () => {
    const path = await open({ multiple: false, directory: true, title: "Choose your xLights show folder" });
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
};

/** True when running inside the Tauri shell (not a plain browser). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
