import type { Backend } from "./backend";
import type {
  ChannelMap,
  DeviceDetails,
  FppSequence,
  PlayerStatus,
  PlaybackStatus,
  ImportSummary,
  Waveform,
  Discovery,
  SilentPeer,
  Edit,
  HistoryEntry,
  OutputStatus,
  PatternSpec,
  PreviewSet,
  PreviewSet3d,
  Show,
  ShowSnapshot,
  TargetSpec,
} from "./types";
import { deepView, frontView } from "../lib/geometry";
import { mapControllers } from "./memoryMapping";
import { channelsPerPixel, newController, nodeCount } from "../lib/shows";

/**
 * An in-memory stand-in for the engine, used by tests and when the UI runs in a plain
 * browser. It applies edits and undo/redo like the engine, and maps channels much like it
 * (see memoryMapping.ts), but does not validate wiring.
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
  /** What the folder picker returns, and what importing any xLights folder produces. */
  nextShowFolder: string | null = null;
  xlightsImport: { show: Show; summary: ImportSummary; notes: string[] } | null = null;
  /** Length of any sequence "played" here, and the path the sequence dialog returns. */
  sequenceDurationMs = 60_000;
  nextSequencePath: string | null = null;
  nextAudioPath: string | null = null;
  /** Image files "on disk", keyed by path, and what the photo dialog returns. */
  images = new Map<string, Uint8Array>();
  nextImagePath: string | null = null;
  /** House model files by path, and the path the "choose model" dialog returns. */
  models = new Map<string, Uint8Array>();
  nextModelPath: string | null = null;
  private playbackStopReason_: string | null = null;
  private playing: {
    path: string;
    positionMs: number;
    since: number | null;
    sequence: string | null;
    music: string | null;
    /** An authored sequence (see playAuthored), rendered here instead of the rainbow. */
    authored?: AuthoredPlayback;
  } | null = null;
  private volume = 1;
  /** What a music file's waveform looks like here (a gentle wave), and its length. */
  waveformFor = (_path: string, slices: number): Waveform => ({
    durationMs: this.sequenceDurationMs,
    peaks: Array.from({ length: slices }, (_, i) => 0.35 + 0.3 * Math.abs(Math.sin(i / 7))),
  });
  /** Fake FPP players by address: what each is playing and the sequences stored on it. */
  fppPlayers: Record<string, { status: PlayerStatus; sequences: FppSequence[] }> = {};

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
    this.syncPlayback();
    return this.snapshot();
  }

  /** Like the engine: playback stops, saying why, when no controller can receive the sequence. */
  private syncPlayback() {
    if (this.playing && !this.playing.authored && !this.show.controllers.some((c) => c.sequenceChannels)) {
      this.playing = null;
      this.playbackStopReason_ = "Playback stopped because no controller has sequence channels anymore.";
    }
  }

  async undo() {
    this.calls.push("undo");
    const previous = this.undoStack.pop();
    if (previous) {
      this.redoStack.push(this.show);
      this.show = previous;
      this.revision++;
      this.syncPlayback();
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
      this.syncPlayback();
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
    this.playing = null; // a test pattern stops playback, like the engine
    this.playbackStopReason_ = null;
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
    // Like the engine: a controller that came from an FPP's output list is filled in, not copied.
    const here = this.show.controllers.filter((c) => c.address === address);
    details.plan.alreadyInShow = here.some((c) => !isPlaceholder(c));
    if (!details.plan.alreadyInShow && here.length > 0) {
      details.plan.notes.push(`Fills in ${here[0].name}, added from your FPP's output list.`);
    }
    return withFreshIds(details);
  }

  async importDevice(address: string) {
    const { device, plan } = await this.inspectDevice(address);
    if (!plan.canImport) throw new Error(`${device.name} has no pixel outputs to import.`);
    // Like the engine: a port-less controller at this address (added from an FPP) is filled in.
    const placeholder = this.show.controllers.find((c) => c.address === address && isPlaceholder(c));
    const controller = placeholder
      ? { ...plan.controller, id: placeholder.id, name: placeholder.name, sequenceChannels: placeholder.sequenceChannels }
      : plan.controller;
    return this.applyEdits([
      ...plan.props.map((prop) => ({ type: "addProp" as const, prop })),
      placeholder ? { type: "updateController" as const, controller } : { type: "addController" as const, controller },
    ]);
  }

  async importFppDestination(address: string, destination: string, protocol: string) {
    const { device, config } = await this.inspectDevice(address);
    const target = config.destinations.find((d) => d.address === destination && d.protocol === protocol);
    if (!target) throw new Error(`${device.name} doesn't send to ${destination}.`);
    if (protocol !== "DDP" && !protocol.startsWith("sACN")) throw new Error(`PixelFlow can't send ${protocol} yet.`);
    const existing = this.show.controllers.find((c) => c.address === destination);
    if (existing) {
      throw new Error(`${target.description || target.address} is already in your show as ${existing.name}.`);
    }
    const controller = {
      ...newController(target.description || target.address, target.address, protocol === "DDP" ? "ddp" : "sacn", 0),
      sequenceChannels:
        target.channels > 0
          ? {
              start: Math.max(1, target.startChannel),
              count: target.channels,
              ...(target.ddpRaw && protocol === "DDP" ? { rawDdpOffsets: true } : {}),
            }
          : null,
    };
    return this.applyEdits([{ type: "addController", controller }]);
  }

  private player(address: string) {
    const player = this.fppPlayers[address];
    if (!player) throw new Error(`Could not reach ${address}: no response`);
    return player;
  }

  async fppStatus(address: string) {
    return structuredClone(this.player(address).status);
  }

  async fppSequences(address: string) {
    return structuredClone(this.player(address).sequences);
  }

  async fppStart(address: string, name: string) {
    this.calls.push(`fppStart:${address}:${name}`);
    const player = this.player(address);
    const sequence = player.sequences.find((s) => `${s.name}.fseq` === name);
    player.status = {
      ...player.status,
      state: "playing",
      playlist: name,
      sequence: name,
      secondsElapsed: 0,
      secondsRemaining: sequence ? Math.round((sequence.frames * sequence.stepMs) / 1000) : 0,
    };
  }

  async fppStop(address: string, gracefully: boolean) {
    this.calls.push(`fppStop:${address}:${gracefully ? "gracefully" : "now"}`);
    const player = this.player(address);
    player.status = gracefully
      ? { ...player.status, state: "stopping" }
      : { ...player.status, state: "idle", playlist: null, sequence: null, secondsElapsed: 0, secondsRemaining: 0 };
  }

  private playbackNow(): PlaybackStatus | null {
    if (!this.playing) return null;
    const { path, positionMs, since, sequence, music, authored } = this.playing;
    const duration = authored?.durationMs ?? this.sequenceDurationMs;
    const position = Math.min(duration, positionMs + (since === null ? 0 : Date.now() - since));
    const ended = position >= duration;
    return {
      state: ended ? "ended" : since === null ? "paused" : "playing",
      path,
      positionMs: position,
      durationMs: duration,
      frameMs: authored?.frameMs ?? 50,
      controllers: this.show.controllers
        .filter((c) => c.sequenceChannels)
        .map((c) => ({ id: c.id, name: c.name, state: "ok" as const, packetsSent: 0, sendErrors: 0, lastError: null })),
      notes: [],
      error: null,
      sequence,
      music,
      offsetMs: this.show.sequences.find((s) => s.id === sequence)?.offsetMs ?? 0,
      volume: this.volume,
      authored: authored !== undefined,
    };
  }

  async startPlayback(path: string, positionMs: number) {
    this.calls.push(`startPlayback:${path}`);
    if (!this.show.controllers.some((c) => c.sequenceChannels)) {
      throw new Error(
        "None of your controllers knows which sequence channels are theirs yet. Add them from your FPP's output list on the Devices screen.",
      );
    }
    this.output = { ...this.output, running: false };
    this.playbackStopReason_ = null;
    this.playing = { path, positionMs, since: Date.now(), sequence: null, music: null };
    return this.playbackNow()!;
  }

  async addSequence(path: string) {
    this.calls.push(`addSequence:${path}`);
    const base = path.split(/[\\/]/).pop()!.replace(/\.fseq$/i, "");
    // Like the engine: a second sequence with the same name gets a number.
    const taken = (n: string) => this.show.sequences.some((s) => s.name === n);
    let name = base;
    for (let n = 2; taken(name); n++) name = `${base} (${n})`;
    const audio = path.replace(/\.fseq$/i, ".mp3");
    return this.applyEdits([
      { type: "addSequence", sequence: { id: crypto.randomUUID(), name, path, audio, offsetMs: 0 } },
    ]);
  }

  async playSequence(id: string, positionMs: number) {
    const entry = this.show.sequences.find((s) => s.id === id);
    if (!entry) throw new Error("There is no sequence with that id.");
    this.calls.push(`playSequence:${entry.name}@${positionMs}`);
    await this.startPlayback(entry.path, positionMs);
    this.playing = { ...this.playing!, sequence: id, music: entry.audio };
    return this.playbackNow()!;
  }

  /** Plays an authored sequence (for MemorySequencer): its frames come from `authored.frame`. */
  playAuthored(authored: AuthoredPlayback, positionMs: number): PlaybackStatus {
    this.calls.push(`playAuthored@${positionMs}`);
    this.output = { ...this.output, running: false };
    this.playbackStopReason_ = null;
    this.playing = { path: authored.path, positionMs, since: Date.now(), sequence: null, music: authored.music, authored };
    return this.playbackNow()!;
  }

  async setPlaybackVolume(volume: number) {
    this.volume = Math.min(1, Math.max(0, volume));
    return this.playbackNow();
  }

  async audioWaveform(path: string, slices: number) {
    return this.waveformFor(path, slices);
  }

  async pickAudioPath() {
    return this.nextAudioPath;
  }

  async pausePlayback(paused: boolean) {
    const now = this.playbackNow();
    if (!this.playing || !now) return null;
    this.playing = { ...this.playing, positionMs: now.positionMs, since: paused ? null : Date.now() };
    return this.playbackNow();
  }

  async seekPlayback(positionMs: number) {
    if (!this.playing) return null;
    const paused = this.playing.since === null;
    this.playing = { ...this.playing, positionMs, since: paused ? null : Date.now() };
    return this.playbackNow();
  }

  async stopPlayback() {
    this.calls.push("stopPlayback");
    this.playing = null;
    this.playbackStopReason_ = null;
  }

  async playbackStatus() {
    return this.playbackNow();
  }

  async playbackStopReason() {
    return this.playbackStopReason_;
  }

  /** A moving rainbow across every prop while something plays. */
  async liveFrame() {
    const status = this.playbackNow();
    if (this.playing?.authored) return status && status.state !== "ended" ? this.playing.authored.frame(status.positionMs) : new Uint8Array();
    const length = this.show.props.reduce((n, p) => n + nodeCount(p.shape) * channelsPerPixel(p), 0);
    const frame = new Uint8Array(status && status.state !== "ended" ? length : 0);
    const shift = (status?.positionMs ?? 0) / 20;
    for (let i = 0; i + 2 < frame.length; i += 3) {
      const hue = ((i / 3) * 4 + shift) % 360;
      const [r, g, b] = [0, 120, 240].map((o) => Math.round(127 + 127 * Math.cos(((hue - o) * Math.PI) / 180)));
      frame[i] = r;
      frame[i + 1] = g;
      frame[i + 2] = b;
    }
    return frame;
  }

  /** Channels for every controller's sequence block: a slow color wash while something plays. */
  async sequenceFrame() {
    const status = this.playbackNow();
    const end = Math.max(0, ...this.show.controllers.map((c) => (c.sequenceChannels ? c.sequenceChannels.start - 1 + c.sequenceChannels.count : 0)));
    const frame = new Uint8Array(status && status.state !== "ended" ? end : 0);
    const t = (status?.positionMs ?? 0) / 1000;
    for (let i = 0; i < frame.length; i++) frame[i] = Math.round(127 + 127 * Math.sin(t * 2 + i / 40 + (i % 3) * 2));
    return frame;
  }

  /** Each prop's pixels in the front view, from the same shapes and transforms as the engine. */
  async previewProps(): Promise<PreviewSet> {
    const layout = layoutOnly(this.show);
    const props = this.show.props.map((prop, i) => ({
      prop: prop.id,
      frameOffset: layout.props[i].frameOffset,
      channelsPerPixel: layout.props[i].channelsPerPixel,
      points: frontView(prop).slice(0, layout.props[i].nodes * 2),
    }));
    return { revision: this.revision, props };
  }

  /** Each prop's pixels in 3D, from the same shapes and transforms as the engine. */
  async previewProps3d(): Promise<PreviewSet3d> {
    const layout = layoutOnly(this.show);
    const props = this.show.props.map((prop, i) => ({
      prop: prop.id,
      frameOffset: layout.props[i].frameOffset,
      channelsPerPixel: layout.props[i].channelsPerPixel,
      xyz: deepView(prop).subarray(0, layout.props[i].nodes * 3),
    }));
    return { revision: this.revision, props };
  }

  async readImage(path: string) {
    const image = this.images.get(path);
    if (!image) throw new Error("This photo was moved or deleted. Choose it again with Replace…");
    return image.slice();
  }

  async pickImagePath() {
    return this.nextImagePath;
  }

  async readHouseModel(path: string) {
    const model = this.models.get(path);
    if (!model) throw new Error("This model was moved or deleted. Choose it again with Replace…");
    return model.slice();
  }

  async pickHouseModelPath() {
    return this.nextModelPath;
  }

  async importXlights(folder: string) {
    this.calls.push(`importXlights:${folder}`);
    if (!this.xlightsImport) throw new Error(`${folder} doesn't look like an xLights show folder (no xlights_rgbeffects.xml).`);
    const { show, summary, notes } = structuredClone(this.xlightsImport);
    this.replace(show, null);
    this.revision++; // unsaved
    return { snapshot: this.snapshot(), summary, notes };
  }

  async pickShowFolder() {
    return this.nextShowFolder;
  }

  async pickSequencePath() {
    return this.nextSequencePath;
  }

  async pickOpenPath() {
    return this.nextOpenPath;
  }

  /** Window close requests (the close button), for tests: see requestClose. */
  private closeHandlers: (() => boolean)[] = [];

  async onCloseRequested(allow: () => boolean) {
    this.closeHandlers.push(allow);
    return () => {
      this.closeHandlers = this.closeHandlers.filter((h) => h !== allow);
    };
  }

  /** Acts like the window's close button: true (and closes) when nothing held it open. */
  requestClose(): boolean {
    const allowed = this.closeHandlers.every((allow) => allow());
    if (allowed) this.calls.push("closeWindow");
    return allowed;
  }

  async closeWindow() {
    this.calls.push("closeWindow");
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
    this.playing = null;
    this.playbackStopReason_ = null;
  }

  private snapshot(): ShowSnapshot {
    const channelMap = { ...layoutOnly(this.show), controllers: mapControllers(this.show) };
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
        universes: channelMap.controllers.reduce((sum, c) => sum + (c.addressing.type === "sacn" ? c.addressing.universes.length : 0), 0),
      },
    };
  }
}

/** New ids for an import plan's controller and props, as the engine creates for each import. */
/** A controller added from an FPP's output list: no ports yet, but it knows its sequence channels. */
function isPlaceholder(c: Show["controllers"][number]): boolean {
  return c.ports.length === 0 && c.sequenceChannels !== null;
}

function withFreshIds(details: DeviceDetails): DeviceDetails {
  const ids = new Map(details.plan.props.map((p) => [p.id, crypto.randomUUID()]));
  details.plan.props = details.plan.props.map((p) => ({ ...p, id: ids.get(p.id)! }));
  details.plan.controller.id = crypto.randomUUID();
  for (const port of details.plan.controller.ports) {
    port.slots = port.slots.map((slot) => ({ ...slot, prop: ids.get(slot.prop) ?? slot.prop }));
  }
  return details;
}

/** An authored sequence playing in the memory backend. */
export interface AuthoredPlayback {
  path: string;
  music: string | null;
  durationMs: number;
  frameMs: number;
  /** The show frame at a moment. */
  frame(positionMs: number): Uint8Array;
}

export function emptyShow(name: string): Show {
  return {
    schemaVersion: 7,
    name,
    settings: { frameRate: 40 },
    props: [],
    groups: [],
    controllers: [],
    sequences: [],
    background: null,
  };
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
    case "updateProp": {
      replaceById(show.props, edit.prop, "prop");
      // A deleted submodel leaves the groups it was in.
      const regions = new Set(edit.prop.regions.map((r) => r.id));
      for (const g of show.groups) if (g.submodels) g.submodels = g.submodels.filter((m) => m.prop !== edit.prop.id || regions.has(m.region));
      break;
    }
    case "removeProp":
      removeById(show.props, edit.id, "prop");
      for (const c of show.controllers) for (const p of c.ports) p.slots = p.slots.filter((s) => s.prop !== edit.id);
      for (const g of show.groups) {
        g.members = g.members.filter((m) => m !== edit.id);
        if (g.submodels) g.submodels = g.submodels.filter((m) => m.prop !== edit.id);
      }
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
    case "addSequence":
      addUnique(show.sequences, edit.sequence, "sequence");
      break;
    case "updateSequence":
      replaceById(show.sequences, edit.sequence, "sequence");
      break;
    case "removeSequence":
      removeById(show.sequences, edit.id, "sequence");
      break;
    case "moveSequence": {
      const from = show.sequences.findIndex((s) => s.id === edit.id);
      if (from < 0) throw new Error("There is no sequence with that id.");
      const [moved] = show.sequences.splice(from, 1);
      show.sequences.splice(Math.min(edit.index, show.sequences.length), 0, moved);
      break;
    }
    case "setBackground": {
      const bg = edit.background;
      // The same checks as the engine.
      if (bg) {
        if (!bg.path.trim()) throw new Error("Choose a photo file for the background.");
        if (!Number.isFinite(bg.x) || !Number.isFinite(bg.y)) throw new Error("The background photo's position must be a number.");
        if (!Number.isFinite(bg.width) || bg.width <= 0) throw new Error("The background photo must be wider than zero.");
        if (!(bg.opacity >= 0 && bg.opacity <= 1)) throw new Error("The background photo's strength must be between 0% and 100%.");
      }
      show.background = bg ? structuredClone(bg) : null;
      break;
    }
    case "setHouseModel": {
      const m = edit.houseModel;
      // The same checks as the engine.
      if (m) {
        const finite = (v: { x: number; y: number; z: number }) => [v.x, v.y, v.z].every(Number.isFinite);
        if (!m.path.trim()) throw new Error("Choose a model file for the house.");
        if (!finite(m.position) || !finite(m.rotationDeg)) throw new Error("The house model's position and rotation must be numbers.");
        if (!Number.isFinite(m.scale) || m.scale <= 0) throw new Error("The house model's scale must be more than zero.");
        if (!(m.opacity >= 0 && m.opacity <= 1)) throw new Error("The house model's strength must be between 0% and 100%.");
      }
      show.houseModel = m ? structuredClone(m) : null;
      break;
    }
  }
}
