import type { Backend } from "./backend";
import type {
  FppSoftware,
  ChannelMap,
  DeviceComparison,
  DeviceConfig,
  DeviceDetails,
  DeviceKind,
  RestorePlan,
  RestoreReport,
  SendPlan,
  SendReport,
  UseProps,
  FppSequence,
  FppFile,
  FppFolder,
  FppSetupPlan,
  ScheduleEntry,
  Controller,
  FppSendPlan,
  FppSendProgress,
  FppSendRequest,
  FppSendResult,
  FppSendStep,
  NameCheck,
  SendSource,
  PlayerStatus,
  PlaybackStatus,
  ImportSummary,
  Waveform,
  Discovery,
  SilentPeer,
  Edit,
  FileRole,
  FilesFound,
  HistoryEntry,
  MenuAction,
  MissingFile,
  OutputStatus,
  PatternSpec,
  PreviewSet,
  PreviewSet3d,
  RecentShow,
  Show,
  ShowSnapshot,
  TargetSpec,
  BrightnessSample,
  CameraMapFrames,
  CameraMapPlan,
  CameraMapSync,
  CameraMapTargetInfo,
  CodeBase,
  DecodedCapture,
} from "./types";
import { deepView, frontView } from "../lib/geometry";
import { type SampleCapture, cameraOwners, ownerProps, planInMemory, sampleCapture } from "./memoryCameraMap";
import { sequenceSeconds, slotCount, SLOT_SECONDS } from "../lib/cameraMap";
import type { FrameSource } from "../lib/captureFrames";
import { mapControllers } from "./memoryMapping";
import { channelsPerPixel, memberProp, newController, nodeCount } from "../lib/shows";
import { fileName, thousands } from "../lib/format";
import { fppFileName } from "../lib/fppNames";
import { filesOf, missingFile, repointEdits, sameFile } from "../lib/showFiles";
import { sampleShow } from "./sampleShow";
import { MAX_UNIVERSE_SIZE, isUniverseSize } from "../lib/controllerEdit";
import { type Setup, applySetup, compareSetup, deviceSetup, diffPorts, oneStringPerPort, showSetup, stringKey, takeFromDevice } from "../lib/deviceSetup";

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
  /** Whether a made-up camera-mapping capture is offered in place of a video (the demo). */
  cameraMapSamples = false;
  /** The capture being read: what syncing and decoding it gives. */
  cameraMapCapture: SampleCapture | null = null;
  /** What the next file dialogs return. */
  nextOpenPath: string | null = null;
  nextSavePath: string | null = null;
  /** Calls made, for test assertions. */
  calls: string[] = [];
  /** Changes whenever another show replaces the open one (like the engine's show generation). */
  generation = 0;
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
  /** Files that aren't "on disk" any more (moved or deleted), by path. */
  missingPaths = new Set<string>();
  /** Where a search of the show's folder finds a missing file, by the path it had. */
  findable = new Map<string, string>();
  /** What the "Locate…" dialog returns. */
  nextLocatePath: string | null = null;
  /** Whether snapshots say every file has been looked at (checkFiles sets it). */
  filesChecked = true;
  /** Where a missing file really was, when the show file moved without it, by its path. */
  wasAt = new Map<string, string>();
  /** Other files a search finds that fit as well, by the missing file's path. */
  alsoFound = new Map<string, string[]>();
  /** Whether a search stops before looking everywhere. */
  searchGivesUp = false;
  /** Shows opened or saved lately, newest first (like the shell's list). */
  recent: RecentShow[] = [];
  /** What "Locate…" for a recent show returns. */
  nextRecentLocatePath: string | null = null;
  private menuHandlers: ((action: MenuAction) => void)[] = [];
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

  async openSampleShow() {
    this.calls.push("openSampleShow");
    // Unsaved, but nothing to ask about until it's changed (like a new show).
    this.replace(sampleShow(), null);
    return this.snapshot();
  }

  async openShow(path: string) {
    this.calls.push(`openShow:${path}`);
    const show = this.files.get(path);
    if (!show) throw new Error(`Could not read ${path}: file not found`);
    this.replace(structuredClone(show), path);
    return this.remember(this.snapshot());
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
    return this.remember(this.snapshot());
  }

  /** Puts a saved show at the top of the recent list (at most 10), like the shell does. */
  private remember(snapshot: ShowSnapshot): ShowSnapshot {
    const path = snapshot.path;
    if (!path) return snapshot;
    const entry: RecentShow = {
      path,
      name: snapshot.show.name,
      openedAt: Date.now(),
      props: snapshot.summary.props,
      pixels: snapshot.summary.pixels,
      controllers: snapshot.summary.controllers,
      thumbnail: layoutThumbnail(snapshot.show),
      status: "here",
    };
    this.recent = [entry, ...this.recent.filter((r) => r.path !== path)].slice(0, 10);
    return snapshot;
  }

  async listRecentShows() {
    this.calls.push("listRecentShows");
    return this.recent.map((r) => ({
      ...r,
      status: this.missingPaths.has(r.path) || !this.files.has(r.path) ? ("missing" as const) : ("here" as const),
    }));
  }

  async forgetRecentShow(path: string) {
    this.calls.push(`forgetRecentShow:${path}`);
    this.recent = this.recent.filter((r) => r.path !== path);
  }

  async clearRecentShows() {
    this.calls.push("clearRecentShows");
    this.recent = [];
  }

  async locateRecentShow(path: string) {
    this.calls.push(`locateRecentShow:${path}`);
    if (!this.recent.some((r) => r.path === path)) throw new Error("That show isn't on your recent list any more.");
    const to = this.nextRecentLocatePath;
    if (!to) return null;
    const snapshot = await this.openShow(to);
    if (to !== path) this.recent = this.recent.filter((r) => r.path !== path);
    return snapshot;
  }

  async onMenu(handler: (action: MenuAction) => void) {
    this.menuHandlers.push(handler);
    return () => {
      this.menuHandlers = this.menuHandlers.filter((h) => h !== handler);
    };
  }

  /** Acts like choosing a File menu item in the menu bar, for tests. */
  chooseMenu(action: MenuAction) {
    for (const handler of this.menuHandlers) handler(action);
  }

  /** The show's files that aren't "on disk", like the engine lists them. */
  missingFiles(): MissingFile[] {
    return filesOf(this.show)
      .filter((f) => f.path.trim() && this.missingPaths.has(f.path))
      .map((f) => missingFile(f.file, f.path, f.owner, this.wasAt.get(f.path)));
  }

  async checkFiles(all: boolean) {
    this.calls.push(all ? "checkFiles:all" : "checkFiles");
    this.filesChecked = true;
    return this.snapshot();
  }

  async findMissingFiles(file?: FileRole): Promise<FilesFound> {
    this.calls.push(file ? `findMissingFiles:${file.kind}` : "findMissingFiles");
    if (!this.path) throw new Error("Save the show first, so PixelFlow knows which folder to look in. Or use Locate… to choose the file.");
    const found = this.missingFiles()
      .filter((m) => !file || sameFile(m.file, file))
      .flatMap((m) => {
        const to = this.findable.get(m.path);
        return to ? [{ file: m.file, name: m.name, from: m.path, to, also: this.alsoFound.get(m.path) ?? [] }] : [];
      });
    const snapshot = found.length ? await this.applyEdits(repointEdits(this.show, found)) : this.snapshot();
    return { snapshot, found, stillMissing: snapshot.missingFiles, gaveUp: this.searchGivesUp };
  }

  async locateFile(file: FileRole) {
    this.calls.push(`locateFile:${file.kind}`);
    const to = this.nextLocatePath;
    if (!to) return null;
    if (this.missingPaths.has(to)) throw new Error(`${fileName(to)} isn't there anymore. Choose another file.`);
    return this.applyEdits(repointEdits(this.show, [{ file, to }]));
  }

  async listHistory() {
    return this.history.map((h) => h.entry);
  }

  async restoreHistory(id: string) {
    const found = this.history.find((h) => h.entry.id === id);
    if (!found) throw new Error("There is no saved version with that id.");
    this.undoStack.push(this.show);
    this.show = structuredClone(found.show);
    this.generation++;
    this.revision++;
    return this.remember(this.snapshot());
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

  async cameraMapTarget(target: TargetSpec, base: CodeBase): Promise<CameraMapTargetInfo> {
    const owners = cameraOwners(this.show, target);
    if (owners.length === 0) throw new Error("Nothing on this target is set up to light. Pick a prop, port, or controller with pixels.");
    return {
      pixels: owners.length,
      seconds: sequenceSeconds(owners.length, base),
      props: ownerProps(owners).map((p) => ({ prop: p.id, name: p.name, nodes: nodeCount(p.shape), covered: owners.filter((o) => o.prop === p).length })),
    };
  }

  /** A made-up video of `target` flashing the sequence (see `cameraMapSamples`). */
  sampleCapture(target: TargetSpec, base: CodeBase): FrameSource {
    this.cameraMapCapture = sampleCapture(cameraOwners(this.show, target), base);
    return this.cameraMapCapture.source;
  }

  async cameraMapSync(_samples: BrightnessSample[], pixels: number, base: CodeBase): Promise<CameraMapSync> {
    const start = this.cameraMapCapture?.start;
    if (start === undefined) throw new Error("Couldn't find the flashing sequence in this video.");
    const windows = Array.from({ length: slotCount(pixels, base) }, (_, k): [number, number] => [
      start + (k + 0.2) * SLOT_SECONDS,
      start + (k + 0.8) * SLOT_SECONDS,
    ]);
    return { start, score: 0.99, windows };
  }

  async cameraMapDecode(_frames: Uint8Array, info: CameraMapFrames): Promise<DecodedCapture> {
    this.calls.push("cameraMapDecode");
    return this.cameraMapCapture?.decoded ?? { width: info.width, height: info.height, pixels: [], duplicates: [], unreadable: [] };
  }

  async cameraMapPlan(target: TargetSpec, pixels: number, decoded: DecodedCapture): Promise<CameraMapPlan> {
    const owners = cameraOwners(this.show, target);
    if (owners.length !== pixels) throw new Error("The show changed since this video was recorded. Record it again.");
    return planInMemory(owners, decoded, this.cameraMapCapture?.toLayout ?? null);
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

  async importDevice(address: string, useProps: UseProps = {}) {
    const { device, config, plan } = await this.inspectDevice(address);
    if (!plan.canImport) throw new Error(`${device.name} has no pixel outputs to import.`);
    // Like the engine: slots remember the controller's own color order, and strings can wire
    // props already in the show instead of starter props.
    const replaced = new Set<string>();
    for (const port of plan.controller.ports) {
      const strings = config.ports.find((p) => p.number === port.number)?.strings ?? [];
      port.slots.forEach((slot, i) => {
        slot.controllerColorOrder = strings[i]?.colorOrder ?? null;
        const existing = useProps[stringKey(port.number, i)];
        if (existing && this.show.props.some((p) => p.id === existing)) {
          replaced.add(slot.prop);
          slot.prop = existing;
        }
      });
    }
    plan.props = plan.props.filter((p) => !replaced.has(p.id));
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

  /** Controllers that answer a reachability check, and this computer's networks (as "a.b.c."
   * prefixes); none by default, as in a browser with no controllers around. */
  answering = new Set<string>();
  localPrefixes: string[] = [];

  async checkControllers(addresses: string[]) {
    this.calls.push("checkControllers");
    const unique = [...new Set(addresses.map((a) => a.trim()).filter(Boolean))];
    return unique.map((address) => ({
      address,
      answering: this.answering.has(address),
      onLocalNetwork: this.localPrefixes.length === 0 || !/^\d+\.\d+\.\d+\.\d+$/.test(address) ? null : this.localPrefixes.some((p) => address.startsWith(p)),
    }));
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

  /** Sizes, dates, and lengths of the fake FPPs' files, by address and then file name (a
   * playlist's without ".json"). */
  fppFileDetails: Record<string, Record<string, Partial<Pick<FppFile, "sizeBytes" | "modified" | "durationMs">>>> = {};
  /** The fake FPPs' schedules, by address (none by default). */
  fppSchedules: Record<string, ScheduleEntry[]> = {};
  /** Opens a web page (the demo opens a browser tab); nothing by default. */
  openUrl: ((url: string) => void) | null = null;

  async fppFolder(address: string, folder: FppFolder): Promise<FppFile[]> {
    const player = this.player(address);
    const files = this.fppFilesOf(address);
    const details = this.fppFileDetails[address] ?? {};
    const file = (name: string, extra: Partial<FppFile> = {}): FppFile => ({
      name,
      sizeBytes: null,
      modified: null,
      durationMs: null,
      channels: null,
      items: null,
      ...extra,
      ...details[name],
    });
    const byName = (a: FppFile, b: FppFile) => a.name.toLowerCase().localeCompare(b.name.toLowerCase());
    if (folder === "sequences") {
      return player.sequences
        .map((s) => file(`${s.name}.fseq`, { durationMs: s.frames * s.stepMs || null, channels: s.channels || null }))
        .sort(byName);
    }
    if (folder === "music") return files.media.map((name) => file(name)).sort(byName);
    return Object.entries(files.playlists)
      .map(([name, items]) => {
        const lengths = items.map((item) => player.sequences.find((s) => `${s.name}.fseq` === item));
        const known = lengths.every((s) => s && s.frames > 0);
        return file(name, { items: items.length, durationMs: known && items.length > 0 ? lengths.reduce((t, s) => t + s!.frames * s!.stepMs, 0) : null });
      })
      .sort(byName);
  }

  async fppSchedule(address: string) {
    this.player(address);
    return structuredClone(this.fppSchedules[address] ?? []);
  }

  /** Like the engine's plan_fpp_setup. */
  async fppSetupPlan(address: string): Promise<FppSetupPlan> {
    const { device, config, plan } = await this.inspectDevice(address);
    const show = this.show;
    const taken = new Set(show.controllers.map((c) => c.name));
    const unique = (base: string) => {
      let name = base;
      for (let n = 2; taken.has(name); n++) name = `${base} ${n}`;
      taken.add(name);
      return name;
    };
    const own = config.ports.length > 0 && plan.canImport && !show.controllers.some((c) => c.address === device.address) ? plan : null;
    if (own) taken.add(own.controller.name);
    const controllers: Controller[] = [];
    const result: FppSetupPlan = { own, controllers, skipped: [], notes: [] };
    for (const d of config.destinations) {
      const name = d.description.trim() || d.address;
      const skip = (reason: string) => result.skipped.push({ name, address: d.address, reason });
      const existing = show.controllers.find((c) => c.address === d.address);
      if (existing) {
        skip(`Already in your show as ${existing.name}.`);
      } else if (controllers.some((c) => c.address === d.address) || own?.controller.address === d.address) {
        skip(`The FPP lists ${d.address} more than once; it's added once.`);
      } else if (d.protocol !== "DDP" && d.protocol !== "sACN unicast" && d.protocol !== "sACN multicast") {
        skip(`PixelFlow can't send ${d.protocol} yet.`);
      } else {
        controllers.push({
          ...newController(unique(name), d.address, d.protocol === "DDP" ? "ddp" : "sacn", 0),
          sequenceChannels:
            d.channels > 0 ? { start: Math.max(1, d.startChannel), count: d.channels, ...(d.ddpRaw && d.protocol === "DDP" ? { rawDdpOffsets: true } : {}) } : null,
        });
      }
    }
    return result;
  }

  async fppSetUpShow(address: string, expected: string[]) {
    this.calls.push(`fppSetUpShow:${address}:${expected.join(",")}`);
    const plan = await this.fppSetupPlan(address);
    const addresses = [...(plan.own ? [plan.own.controller.address] : []), ...plan.controllers.map((c) => c.address)];
    if (addresses.join("\n") !== expected.join("\n")) {
      throw new Error("What this FPP sends to, or your show, changed since you looked. Check the list again before adding.");
    }
    if (addresses.length === 0) throw new Error("Your show already has everything this FPP sends to.");
    return this.applyEdits([
      ...(plan.own ? [...plan.own.props.map((prop) => ({ type: "addProp" as const, prop })), { type: "addController" as const, controller: plan.own.controller }] : []),
      ...plan.controllers.map((controller) => ({ type: "addController" as const, controller })),
    ]);
  }

  /** How the next send of a setup goes wrong, if it does (for tests and screenshots). */
  setupSendFailure: "fail" | "mismatch" | null = null;
  /** Put back fails (the controller doesn't answer) while set. */
  restoreFailure = false;
  private compared = new Map<string, DeviceConfig>();
  private sends = new Map<string, { shown: string; target: Setup; ids: string[]; kind: DeviceKind; deviceName: string }>();
  /** The copy of each controller's setup from before its last send, kept until dismissed (like
   * the engine's files in the app's data folder). */
  setupCopies = new Map<string, { config: DeviceConfig; deviceName: string; address: string; takenAtMs: number }>();
  private restores = new Map<string, { key: string; shown: string; ids: string[] }>();

  /** Which device this is, whatever its address (the engine uses an FPP's uuid or a WLED's MAC;
   * the pretend devices have their kind and name). */
  private identityKey(address: string) {
    const { device } = this.deviceAt(address);
    return `${device.kind}-${device.name}`;
  }

  private controllerAt(address: string) {
    const here = this.show.controllers.filter((c) => c.address === address);
    const controller = here.find((c) => !isPlaceholder(c)) ?? here[0];
    if (!controller) throw new Error(`No controller at ${address} is in your show. Add it from the Controllers screen first.`);
    return controller;
  }

  private deviceAt(address: string) {
    const found = this.deviceNetwork.details.find((d) => d.device.address === address);
    if (!found) throw new Error(`Could not reach ${address}: no response`);
    return found;
  }

  /** Like the engine's compare_device. */
  async compareDevice(address: string): Promise<DeviceComparison> {
    this.calls.push(`compareDevice:${address}`);
    const controller = this.controllerAt(address);
    const { device, config } = structuredClone(this.deviceAt(address));
    this.compared.set(address, config);
    return { device, controllerName: controller.name, ...compareSetup(this.show, controller, device.kind, config) };
  }

  /** Like the engine's take_from_device_setup: one undo step. */
  async takeFromDevice(address: string, picks: string[], useProps: UseProps = {}) {
    this.calls.push(`takeFromDevice:${address}:${picks.join(",")}`);
    const config = this.compared.get(address);
    if (!config) throw new Error("Compare with the controller first.");
    if (picks.length === 0) throw new Error("Pick at least one difference to take into your show.");
    const { device } = this.deviceAt(address);
    const taken = takeFromDevice(this.show, this.controllerAt(address), device.kind, config, picks, useProps);
    return this.applyEdits([
      ...taken.newProps.map((prop) => ({ type: "addProp" as const, prop })),
      ...taken.changedProps.map((prop) => ({ type: "updateProp" as const, prop })),
      { type: "updateController" as const, controller: taken.controller },
    ]);
  }

  /** Like the engine's plan_device_setup (outputs only: the pretend devices have no receive settings). */
  async planDeviceSetup(address: string): Promise<SendPlan> {
    this.calls.push(`planDeviceSetup:${address}`);
    const controller = this.controllerAt(address);
    const { device, config } = structuredClone(this.deviceAt(address));
    if (device.kind === "falcon") {
      this.sends.delete(address);
      return {
        device,
        controllerName: controller.name,
        changes: [],
        notes: [],
        problems: [],
        busy: null,
        canSend: false,
        reason: "PixelFlow can't send a setup to Falcon controllers yet. Use Compare to bring the Falcon's setup into your show, or set it on the Falcon's own page.",
        restorePoint: this.restorePoint(address),
      };
    }
    const target = showSetup(this.show, controller, oneStringPerPort(device.kind));
    const shown = JSON.stringify(target);
    const current = deviceSetup(config);
    if (device.kind === "wled") {
      // Like the engine: a WLED output the show doesn't wire is left as it is.
      for (const port of current.ports) {
        const wanted = target.ports.find((p) => p.number === port.number);
        if (port.strings.length === 0 || (wanted && wanted.strings.length > 0)) continue;
        target.notes.push(`Output ${port.number} isn't wired in your show; PixelFlow leaves it as it is.`);
        if (wanted) wanted.strings = port.strings;
        else target.ports.push(port);
      }
      target.ports.sort((a, b) => a.number - b.number);
    }
    const changes = diffPorts(current, target, "toDevice");
    this.sends.set(address, { shown, target, ids: changes.map((c) => c.id), kind: device.kind, deviceName: device.name });
    // Like the engine: fppd won't load a string over 1,600 pixels.
    const problems =
      device.kind === "fpp"
        ? target.ports.flatMap((port) =>
            port.strings
              .filter((s) => s.pixels > 1600)
              .map((s) => `Port ${port.number} string ${port.strings.indexOf(s) + 1} (${s.name}): an FPP string drives at most 1,600 pixels, and this one would have ${thousands(s.pixels)}. Split it across strings or ports.`),
          )
        : [];
    const player = this.fppPlayers[address]?.status;
    const busy =
      player && (player.state === "playing" || player.state === "paused")
        ? `This FPP is playing ${player.sequence ?? player.playlist ?? "a show"}. Its lights may flicker or go dark while the new setup is saved.`
        : null;
    const notes = device.kind === "fpp" && changes.length > 0 ? ["If the lights don't change after sending, restart FPP's player (fppd) from the FPP's own page."] : [];
    return {
      device,
      controllerName: controller.name,
      changes,
      notes: [...target.notes, ...notes],
      problems,
      busy,
      canSend: changes.length > 0 && problems.length === 0,
      reason: problems.length > 0 ? "PixelFlow won't send this until the problems below are fixed." : changes.length > 0 ? null : "The controller already matches your show.",
      restorePoint: this.restorePoint(address),
    };
  }

  private restorePoint(address: string) {
    const key = this.identityKey(address);
    const copy = this.setupCopies.get(key);
    return copy ? { key, deviceName: copy.deviceName, address: copy.address, takenAtMs: copy.takenAtMs } : null;
  }

  /** Like the engine's send_device_setup: keeps a copy of the device's setup first (until it's
   * dismissed), then sends and checks. */
  async sendDeviceSetup(address: string, expected: string[]): Promise<SendReport> {
    this.calls.push(`sendDeviceSetup:${address}:${expected.join(",")}`);
    const session = this.sends.get(address);
    if (!session) throw new Error("Review what will change before sending.");
    if (session.ids.join("\n") !== expected.join("\n")) throw new Error("What will change isn't what was shown. Review the changes again.");
    session.ids = ["(sending)"];
    const found = this.deviceAt(address);
    const now = showSetup(this.show, this.controllerAt(address), oneStringPerPort(found.device.kind));
    if (JSON.stringify(now) !== session.shown) throw new Error("Your show changed since you looked. Review the changes again.");
    // Like the engine: the oldest copy not yet put back is kept (the last setup known to work).
    const key = this.identityKey(address);
    if (!this.setupCopies.has(key)) {
      this.setupCopies.set(key, { config: structuredClone(found.config), deviceName: session.deviceName, address, takenAtMs: Date.now() });
    }
    const failure = this.setupSendFailure;
    this.setupSendFailure = null;
    if (failure === "fail") {
      // Part of it landed before the controller stopped answering.
      found.config = applySetup(found.config, { ...session.target, ports: session.target.ports.slice(0, 1) });
      return {
        status: "failed",
        message: "Saving the new setup failed: Could not reach the controller: it didn't answer in time. It may have been only partly saved.",
        mismatches: [],
        canRestore: true,
        notes: [],
      };
    }
    if (failure === "mismatch") {
      return {
        status: "mismatch",
        message: "Sent, but reading it back, the controller's setup doesn't match your show.",
        mismatches: diffPorts(deviceSetup(found.config), session.target, "toDevice"),
        canRestore: true,
        notes: [],
      };
    }
    found.config = applySetup(found.config, session.target);
    const message =
      session.kind === "fpp"
        ? "Saved, and reading it back, the FPP's pixel outputs match your show. The lights use them once FPP's player (fppd) restarts: restart it from the FPP's own page, then check its warnings."
        : "Sent. Reading it back, the controller matches your show.";
    return { status: "sent", message, mismatches: [], canRestore: true, notes: [] };
  }

  /** Like the engine's plan_device_restore: what putting the kept copy back would change. */
  async planDeviceRestore(address: string): Promise<RestorePlan> {
    this.calls.push(`planDeviceRestore:${address}`);
    const key = this.identityKey(address);
    const copy = this.setupCopies.get(key);
    if (!copy) throw new Error("There's no earlier setup of this controller to put back.");
    const { device, config } = structuredClone(this.deviceAt(address));
    const changes = diffPorts(deviceSetup(config), deviceSetup(copy.config), "toDevice");
    this.restores.set(address, { key, shown: JSON.stringify(config), ids: changes.map((c) => c.id) });
    return {
      device,
      copy: this.restorePoint(address)!,
      changes,
      canRestore: changes.length > 0,
      reason: changes.length > 0 ? null : "The controller already holds the kept setup.",
    };
  }

  /** Like the engine's restore_device_setup: only on the device the copy came from, only as shown;
   * once it's back, the copy is let go. */
  async restoreDeviceSetup(address: string, expected: string[]): Promise<RestoreReport> {
    this.calls.push(`restoreDeviceSetup:${address}:${expected.join(",")}`);
    const session = this.restores.get(address);
    if (!session) throw new Error("Look at what Put back will change first.");
    if (session.ids.join("\n") !== expected.join("\n")) throw new Error("What Put back will change isn't what was shown. Look again.");
    const found = this.deviceAt(address);
    if (this.identityKey(address) !== session.key) {
      throw new Error(`The controller at ${address} is now ${found.device.name}, not the one this was planned for. Nothing was changed.`);
    }
    if (JSON.stringify(found.config) !== session.shown) throw new Error("The controller's setup changed since you looked, so nothing was put back. Look again.");
    const copy = this.setupCopies.get(session.key);
    if (!copy) throw new Error("There's no earlier setup of this controller to put back.");
    this.restores.delete(address);
    if (this.restoreFailure) return { restored: false, message: `Putting the previous setup back failed: Could not reach ${address}: it didn't answer in time` };
    found.config = structuredClone(copy.config);
    this.setupCopies.delete(session.key);
    return { restored: true, message: "The previous setup is back on the controller." };
  }

  async forgetDeviceSetupCopy(key: string) {
    this.calls.push(`forgetDeviceSetupCopy:${key}`);
    this.setupCopies.delete(key);
  }

  /** What the fake FPPs run, by address (an unlisted one has nothing newer and an unread list). */
  fppSoftwares: Record<string, FppSoftware> = {};

  async fppSoftware(address: string): Promise<FppSoftware> {
    this.player(address);
    return structuredClone(
      this.fppSoftwares[address] ?? { version: "", osBuild: "", osRelease: "", platform: "", bits: null, imagePrefix: null, update: null, checked: false },
    );
  }

  async openDevicePage(address: string, page?: "about.php") {
    this.calls.push(page ? `openDevicePage:${address}:${page}` : `openDevicePage:${address}`);
    this.openUrl?.(`http://${address}/${page ?? ""}`);
  }

  /** The fake FPP's music, playlists (sequence files on each), free space (null when it doesn't
   * say), and channel-layout warnings, by address. */
  fppFiles: Record<string, { media: string[]; playlists: Record<string, string[]>; freeBytes: number | null; layoutWarnings?: string[] }> = {};
  /** How long each step of a fake send takes (ms): 0 in tests, a little in the demo. */
  fppSendStepMs = 0;
  /** Makes the next send fail with this message. */
  fppSendError: string | null = null;
  private sendCancels = 0;

  private fppFilesOf(address: string) {
    this.player(address);
    return (this.fppFiles[address] ??= { media: [], playlists: {}, freeBytes: 8e9 });
  }

  async fppSequenceNames(address: string) {
    return this.player(address).sequences.map((s) => `${s.name}.fseq`);
  }

  async fppSendPlan(address: string, source: SendSource, music: string | null): Promise<FppSendPlan> {
    const player = this.player(address);
    const files = this.fppFilesOf(address);
    const sequence = fppFileName(source.kind === "file" ? fileName(source.path).replace(/\.fseq$/i, "") : source.name, "fseq");
    const check = (list: string[], name: string): NameCheck => {
      const taken = (n: string) => list.some((f) => f.toLowerCase() === n.toLowerCase());
      const dot = name.lastIndexOf(".");
      const [stem, ext] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
      let n = 2;
      while (taken(`${stem} (${n})${ext}`)) n++;
      const fppName = list.find((f) => f === name) ?? list.find((f) => f.toLowerCase() === name.toLowerCase()) ?? null;
      return { name, exists: fppName !== null, fppName, keepBothName: `${stem} (${n})${ext}` };
    };
    let musicCheck: NameCheck | null = null;
    if (music) {
      const file = fileName(music);
      const ext = /\.(mp3|ogg|m4a|wav|au|m4p|wma|flac|aac)$/i.exec(file)?.[1];
      if (!ext) throw new Error(`The FPP can't play ${file} with a sequence. Choose an mp3, ogg, m4a, wav, or flac file.`);
      musicCheck = check(files.media, fppFileName(file.slice(0, -ext.length - 1), ext));
    }
    return {
      sequence: check(
        player.sequences.map((s) => `${s.name}.fseq`),
        sequence,
      ),
      music: musicCheck,
      playlists: Object.keys(files.playlists).sort(),
      newPlaylistName: sequence.replace(/\.fseq$/, "").replace(/[^-a-zA-Z0-9_ ]/g, "").trim() || "PixelFlow",
      freeBytes: files.freeBytes,
      layoutWarnings: files.layoutWarnings ?? [],
    };
  }

  async fppSend(address: string, request: FppSendRequest, onProgress?: (progress: FppSendProgress) => void): Promise<FppSendResult> {
    this.calls.push(`fppSend:${address}:${request.sequenceName}:${request.playlist.kind}`);
    const player = this.player(address);
    const files = this.fppFilesOf(address);
    const started = this.sendCancels;
    const cancelled = "The upload was cancelled. Nothing on the FPP was changed.";
    const step = async (name: FppSendStep, total: number) => {
      for (const percent of [0, 25, 50, 75, 100]) {
        if (this.sendCancels !== started && name !== "commit") throw new Error(cancelled);
        onProgress?.({ sendId: request.sendId, step: name, percent, done: (total * percent) / 100, total });
        if (this.fppSendStepMs) await new Promise((r) => setTimeout(r, this.fppSendStepMs));
      }
    };
    // Like the shell: nothing is replaced unless the user chose to replace that very file.
    const sequences = player.sequences.map((s) => `${s.name}.fseq`);
    const refuse = (list: string[], name: string, replace: boolean) => {
      const found = list.find((f) => f.toLowerCase() === name.toLowerCase());
      if (found !== undefined && !(replace && found === name)) {
        throw new Error(
          `The FPP now has a file called ${found} that wasn't there when you chose what to send, so nothing was replaced. Check again and choose what to do.`,
        );
      }
    };
    if (request.source.kind === "openSequence") await step("export", 1200);
    refuse(sequences, request.sequenceName, request.replaceSequence);
    if (request.musicName) {
      if (request.uploadMusic) refuse(files.media, request.musicName, request.replaceMusic);
      else if (!files.media.includes(request.musicName)) throw new Error(`The FPP no longer has ${request.musicName}. Check again and choose what to do.`);
    }
    if (this.fppSendError) {
      const error = this.fppSendError;
      this.fppSendError = null;
      throw new Error(error);
    }
    await step("sequence", 24_000_000);
    if (request.uploadMusic && request.musicName) await step("music", 4_000_000);
    if (this.sendCancels !== started) throw new Error(cancelled);
    await step("commit", 1);
    const stem = request.sequenceName.replace(/\.fseq$/i, "");
    if (!player.sequences.some((s) => s.name === stem)) {
      player.sequences.push({ name: stem, frames: this.sequenceDurationMs / 50, stepMs: 50, channels: 4800 });
    }
    if (request.uploadMusic && request.musicName && !files.media.includes(request.musicName)) files.media.push(request.musicName);
    let playlist: string | null = null;
    const notes: string[] = [];
    const choice = request.playlist;
    if (choice.kind === "new" && Object.keys(files.playlists).some((p) => p.toLowerCase() === choice.name.toLowerCase())) {
      notes.push(`The FPP already has a playlist called "${choice.name}". The sequence is on the FPP; add it to a playlist on FPP's Playlists page.`);
    } else if (choice.kind !== "none") {
      playlist = choice.name;
      const items = (files.playlists[playlist] ??= []);
      if (!items.includes(request.sequenceName)) items.push(request.sequenceName);
    }
    onProgress?.({ sendId: request.sendId, step: "playlist", percent: 100, done: 1, total: 1 });
    return {
      sequenceName: request.sequenceName,
      musicName: request.musicName,
      playlist,
      playName: request.playlist.kind === "new" && playlist ? playlist : request.sequenceName,
      notes,
    };
  }

  async cancelFppSend() {
    this.calls.push("cancelFppSend");
    this.sendCancels++;
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
    const played = positionMs + (since === null ? 0 : Date.now() - since);
    const position = authored?.looping && duration > 0 ? played % duration : Math.min(duration, played);
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
      looping: authored?.looping ?? false,
    };
  }

  async startPlayback(path: string, positionMs: number) {
    this.calls.push(`startPlayback:${path}`);
    if (!this.show.controllers.some((c) => c.sequenceChannels)) {
      throw new Error(
        "None of your controllers knows which sequence channels are theirs yet. Add them from your FPP's output list on the Controllers screen.",
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

  /** Loops a playing authored sequence, or stops looping it, from where it is now; null when
   * none is playing. */
  setAuthoredLooping(looping: boolean): PlaybackStatus | null {
    const now = this.playbackNow();
    if (!this.playing?.authored || !now) return null;
    const { since } = this.playing;
    this.playing = { ...this.playing, positionMs: now.positionMs, since: since === null ? null : Date.now(), authored: { ...this.playing.authored, looping } };
    return this.playbackNow();
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
    const image = this.missingPaths.has(path) ? undefined : this.images.get(path);
    if (!image) throw new Error("This photo was moved or deleted. Choose it again with Replace…");
    return image.slice();
  }

  async pickImagePath() {
    return this.nextImagePath;
  }

  async readHouseModel(path: string) {
    const model = this.missingPaths.has(path) ? undefined : this.models.get(path);
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

  async pickSavePath(_defaultName?: string) {
    return this.nextSavePath;
  }

  private replace(show: Show, path: string | null) {
    this.show = show;
    this.path = path;
    this.generation++;
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
      missingFiles: this.filesChecked ? this.missingFiles() : [],
      filesChecked: this.filesChecked,
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
  /** Goes round again from the top at the end instead of ending. */
  looping: boolean;
  /** The show frame at a moment. */
  frame(positionMs: number): Uint8Array;
}

/** A small SVG picture of the show's pixels seen from the front (like the shell's), or null. */
export function layoutThumbnail(show: Show): string | null {
  const points: number[] = [];
  for (const prop of show.props) points.push(...frontView(prop));
  if (points.length === 0) return null;
  const [w, h, pad] = [320, 200, 12];
  let [minX, maxX, minY, maxY] = [Infinity, -Infinity, Infinity, -Infinity];
  for (let i = 0; i < points.length; i += 2) {
    minX = Math.min(minX, points[i]);
    maxX = Math.max(maxX, points[i]);
    minY = Math.min(minY, points[i + 1]);
    maxY = Math.max(maxY, points[i + 1]);
  }
  const [spanX, spanY] = [Math.max(maxX - minX, 1e-3), Math.max(maxY - minY, 1e-3)];
  const scale = Math.min((w - 2 * pad) / spanX, (h - 2 * pad) / spanY);
  const [offX, offY] = [(w - spanX * scale) / 2, (h - spanY * scale) / 2];
  const seen = new Set<string>();
  let dots = "";
  for (let i = 0; i < points.length && seen.size < 1500; i += 2) {
    const x = Math.round((points[i] - minX) * scale + offX);
    const y = Math.round(h - ((points[i + 1] - minY) * scale + offY));
    if (!seen.has(`${x},${y}`)) {
      seen.add(`${x},${y}`);
      dots += `M${x} ${y}h0`;
    }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}"><path d="${dots}" fill="none" stroke="#fcd34d" stroke-width="4" stroke-linecap="round"/></svg>`;
}

export function emptyShow(name: string): Show {
  return {
    schemaVersion: 11,
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

/** Refuses a universe size the engine can't read, in its words. */
function checkUniverseSize(controller: Controller): void {
  const size = controller.protocol.type === "sacn" ? controller.protocol.universeSize : 510;
  if (!isUniverseSize(size)) throw new Error(`A universe carries 1 to ${MAX_UNIVERSE_SIZE} channels, so ${size} channels per universe won't work.`);
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
      for (const g of show.groups) g.members = g.members.filter((m) => typeof m === "string" || m.prop !== edit.prop.id || regions.has(m.region));
      break;
    }
    case "removeProp":
      removeById(show.props, edit.id, "prop");
      for (const c of show.controllers) for (const p of c.ports) p.slots = p.slots.filter((s) => s.prop !== edit.id);
      for (const g of show.groups) g.members = g.members.filter((m) => memberProp(m) !== edit.id);
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
      checkUniverseSize(edit.controller);
      addUnique(show.controllers, edit.controller, "controller");
      break;
    case "updateController":
      checkUniverseSize(edit.controller);
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
