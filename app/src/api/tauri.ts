import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Backend } from "./backend";

const SHOW_FILTER = [{ name: "PixelFlow show", extensions: ["json"] }];

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
  fppStatus: (address) => invoke("fpp_status", { address }),
  fppSequences: (address) => invoke("fpp_sequences", { address }),
  fppStart: (address, name) => invoke("fpp_start", { address, name }),
  fppStop: (address, gracefully) => invoke("fpp_stop", { address, gracefully }),
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
