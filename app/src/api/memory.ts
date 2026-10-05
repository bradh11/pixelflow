import type { Backend } from "./backend";
import type {
  ChannelMap,
  DeviceDetails,
  Discovery,
  SilentPeer,
  Edit,
  HistoryEntry,
  OutputStatus,
  PatternSpec,
  Show,
  ShowSnapshot,
  TargetSpec,
} from "./types";
import { channelsPerPixel, nodeCount } from "../lib/shows";

/**
 * An in-memory stand-in for the engine, used by tests and when the UI runs in a plain
 * browser. It applies edits and undo/redo like the engine but does not validate wiring.
 */
export class MemoryBackend implements Backend {
  show: Show;
  path: string | null = null;
  revision = 0;
  savedRevision = 0;
  undoStack: Show[] = [];
  redoStack: Show[] = [];
  /** Files "on disk", keyed by path. */
  files = new Map<string, Show>();
  history: { entry: HistoryEntry; show: Show }[] = [];
  output: OutputStatus = stoppedOutput(0);
  lastTarget: TargetSpec | null = null;
  /** What the next file dialogs return. */
  nextOpenPath: string | null = null;
  nextSavePath: string | null = null;
  /** Calls made, for test assertions. */
  calls: string[] = [];
  /** Devices "on the network" (see `demoDevices()`); empty by default. */
  deviceNetwork: { details: DeviceDetails[]; silent: SilentPeer[] } = { details: [], silent: [] };

  constructor(show?: Show) {
    this.show = show ?? emptyShow("Untitled Show");
  }

  async getSnapshot() {
    return this.snapshot();
  }

  async applyEdits(edits: Edit[]) {
    this.calls.push("applyEdits");
    const next = structuredClone(this.show);
    for (const edit of edits) applyEdit(next, edit);
    this.undoStack.push(this.show);
    this.redoStack = [];
    this.show = next;
    this.revision++;
    return this.snapshot();
  }

  async undo() {
    this.calls.push("undo");
    const previous = this.undoStack.pop();
    if (previous) {
      this.redoStack.push(this.show);
      this.show = previous;
      this.revision++;
    }
    return this.snapshot();
  }

  async redo() {
    this.calls.push("redo");
    const next = this.redoStack.pop();
    if (next) {
      this.undoStack.push(this.show);
      this.show = next;
      this.revision++;
    }
    return this.snapshot();
  }

  async newShow(name: string) {
    this.calls.push("newShow");
    this.replace(emptyShow(name), null);
    return this.snapshot();
  }

  async openShow(path: string) {
    this.calls.push(`openShow:${path}`);
    const show = this.files.get(path);
    if (!show) throw new Error(`Could not read ${path}: file not found`);
    this.replace(structuredClone(show), path);
    return this.snapshot();
  }

  async saveShow() {
    if (!this.path) throw new Error("This show has not been saved yet. Choose where to save it.");
    return this.saveShowAs(this.path);
  }

  async saveShowAs(path: string) {
    this.calls.push(`saveShowAs:${path}`);
    this.files.set(path, structuredClone(this.show));
    this.path = path;
    this.savedRevision = this.revision;
    return this.snapshot();
  }

  async listHistory() {
    return this.history.map((h) => h.entry);
  }

  async restoreHistory(id: string) {
    const found = this.history.find((h) => h.entry.id === id);
    if (!found) throw new Error("There is no saved version with that id.");
    this.undoStack.push(this.show);
    this.show = structuredClone(found.show);
    this.revision++;
    return this.snapshot();
  }

  async startOutput(pattern: PatternSpec, target: TargetSpec) {
    this.calls.push("startOutput");
    this.lastTarget = target;
    this.output = {
      ...stoppedOutput(this.output.generation + 1),
      running: true,
      pattern,
      target,
      controllers: this.show.controllers.map((c) => ({
        id: c.id,
        name: c.name,
        state: "ok",
        packetsSent: 0,
        sendErrors: 0,
        lastError: null,
      })),
    };
    return this.output;
  }

  async stopOutput() {
    this.calls.push("stopOutput");
    this.output = stoppedOutput(this.output.generation);
    return this.output;
  }

  async outputStatus() {
    return this.output;
  }

  async discoverDevices(hosts: string[], network: boolean): Promise<Discovery> {
    this.calls.push(`discoverDevices:${hosts.join(",")}${network ? ":network" : ""}`);
    return structuredClone({
      devices: this.deviceNetwork.details.map((d) => d.device),
      silent: this.deviceNetwork.silent,
    });
  }

  async inspectDevice(address: string): Promise<DeviceDetails> {
    const found = this.deviceNetwork.details.find((d) => d.device.address === address);
    if (!found) throw new Error(`Could not reach ${address}: no response`);
    const details = structuredClone(found);
    details.plan.alreadyInShow = this.show.controllers.some((c) => c.address === address);
    return withFreshIds(details);
  }

  async importDevice(address: string) {
    const { device, plan } = await this.inspectDevice(address);
    if (!plan.canImport) throw new Error(`${device.name} has no pixel outputs to import.`);
    return this.applyEdits([
      ...plan.props.map((prop) => ({ type: "addProp" as const, prop })),
      { type: "addController" as const, controller: plan.controller },
    ]);
  }

  async pickOpenPath() {
    return this.nextOpenPath;
  }

  async pickSavePath() {
    return this.nextSavePath;
  }

  private replace(show: Show, path: string | null) {
    this.show = show;
    this.path = path;
    this.undoStack = [];
    this.redoStack = [];
    this.revision++;
    this.savedRevision = this.revision;
    this.output = stoppedOutput(this.output.generation);
  }

  private snapshot(): ShowSnapshot {
    const channelMap = layoutOnly(this.show);
    return {
      revision: this.revision,
      path: this.path,
      dirty: this.revision !== this.savedRevision,
      canUndo: this.undoStack.length > 0,
      canRedo: this.redoStack.length > 0,
      show: structuredClone(this.show),
      issues: [],
      channelMap,
      summary: {
        props: this.show.props.length,
        pixels: channelMap.props.reduce((sum, p) => sum + p.nodes, 0),
        controllers: this.show.controllers.length,
        universes: 0,
      },
    };
  }
}

/** New ids for an import plan's controller and props, as the engine creates for each import. */
function withFreshIds(details: DeviceDetails): DeviceDetails {
  const ids = new Map(details.plan.props.map((p) => [p.id, crypto.randomUUID()]));
  details.plan.props = details.plan.props.map((p) => ({ ...p, id: ids.get(p.id)! }));
  details.plan.controller.id = crypto.randomUUID();
  for (const port of details.plan.controller.ports) {
    port.slots = port.slots.map((slot) => ({ ...slot, prop: ids.get(slot.prop) ?? slot.prop }));
  }
  return details;
}

export function emptyShow(name: string): Show {
  return { schemaVersion: 2, name, settings: { frameRate: 40 }, props: [], groups: [], controllers: [] };
}

function stoppedOutput(generation: number): OutputStatus {
  return {
    running: false,
    generation,
    pattern: null,
    target: null,
    frames: 0,
    lateFrames: 0,
    achievedFps: 0,
    controllers: [],
    stopReason: null,
  };
}

/** Frame layout only (no controller mapping) — enough for the UI's pixel counts. */
function layoutOnly(show: Show): ChannelMap {
  let offset = 0;
  const props = show.props.map((prop) => {
    const layout = { prop: prop.id, frameOffset: offset, nodes: nodeCount(prop.shape), channelsPerPixel: channelsPerPixel(prop) };
    offset += layout.nodes * layout.channelsPerPixel;
    return layout;
  });
  return { frameLen: offset, props, controllers: [] };
}

function applyEdit(show: Show, edit: Edit): void {
  const replaceById = <T extends { id: string }>(items: T[], item: T, kind: string) => {
    const i = items.findIndex((x) => x.id === item.id);
    if (i < 0) throw new Error(`There is no ${kind} with that id.`);
    items[i] = structuredClone(item);
  };
  const removeById = <T extends { id: string }>(items: T[], id: string, kind: string) => {
    const i = items.findIndex((x) => x.id === id);
    if (i < 0) throw new Error(`There is no ${kind} with that id.`);
    items.splice(i, 1);
  };
  const addUnique = <T extends { id: string }>(items: T[], item: T, kind: string) => {
    if (items.some((x) => x.id === item.id)) throw new Error(`A ${kind} with that id already exists.`);
    items.push(structuredClone(item));
  };
  switch (edit.type) {
    case "renameShow":
      show.name = edit.name;
      break;
    case "setFrameRate":
      show.settings.frameRate = edit.fps;
      break;
    case "addProp":
      addUnique(show.props, edit.prop, "prop");
      break;
    case "updateProp":
      replaceById(show.props, edit.prop, "prop");
      break;
    case "removeProp":
      removeById(show.props, edit.id, "prop");
      for (const c of show.controllers) for (const p of c.ports) p.slots = p.slots.filter((s) => s.prop !== edit.id);
      for (const g of show.groups) g.members = g.members.filter((m) => m !== edit.id);
      break;
    case "addGroup":
      addUnique(show.groups, edit.group, "group");
      break;
    case "updateGroup":
      replaceById(show.groups, edit.group, "group");
      break;
    case "removeGroup":
      removeById(show.groups, edit.id, "group");
      break;
    case "addController":
      addUnique(show.controllers, edit.controller, "controller");
      break;
    case "updateController":
      replaceById(show.controllers, edit.controller, "controller");
      break;
    case "removeController":
      removeById(show.controllers, edit.id, "controller");
      break;
  }
}
