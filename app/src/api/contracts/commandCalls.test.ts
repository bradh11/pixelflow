// What the window sends the shell. Every method of the API objects that talk to the desktop shell
// is called here with arguments of the window's own types, and what each passes to `invoke` is
// recorded in commandCalls.json. The shell's tests read that file and hold each call against the
// command it names (app/src-tauri/src/tests/contracts/commands.rs): a command or argument renamed
// on one side alone fails there, and a call that changes here without the file fails here.

import { beforeAll, describe, expect, it, vi } from "vitest";

const recorder = vi.hoisted(() => {
  // The window's own API object is chosen when its module loads: as in the desktop app.
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  return {
    /** The API method being called ("backend.applyEdits"). */
    current: "",
    calls: [] as { api: string; command: string; args?: unknown; headers?: Record<string, string> }[],
    listens: [] as { api: string; event: string }[],
  };
});

/** The bytes an empty set of preview positions comes as (see previewBytes.ts). */
function emptyPreview(format: number): ArrayBuffer {
  const view = new DataView(new ArrayBuffer(16));
  view.setUint32(0, format, true);
  return view.buffer;
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (command: string, args?: unknown, options?: { headers?: Record<string, string> }) => {
    const raw = args instanceof Uint8Array;
    recorder.calls.push({ api: recorder.current, command, ...(raw ? { headers: options?.headers ?? {} } : { args: args ?? {} }) });
    if (command === "preview_props" || command === "ai_preview") return emptyPreview(1);
    if (command === "preview_props_3d") return emptyPreview(2);
    return null;
  },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (event: string) => {
    recorder.listens.push({ api: recorder.current, event });
    return () => {};
  },
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "main", onCloseRequested: async () => () => {}, destroy: async () => {} }),
}));

import type { AppWindow } from "../appWindow";
import type { AssistantApi } from "../assistant";
import type { Backend } from "../backend";
import type { Effect, Row, SequenceEdit, TimingTrack } from "../sequence";
import type { SequencerApi } from "../sequencer";
import type {
  CodeBase,
  Controller,
  DecodedCapture,
  DownloadClash,
  Edit,
  FileRole,
  FppDownloadRequest,
  FppFolder,
  FppSendRequest,
  Group,
  PatternKind,
  ProjectLink,
  PickKind,
  PlaylistChoice,
  Prop,
  SendSource,
  SequenceEntry,
  TargetSpec,
} from "../types";
import { staleFile } from "./support";

/** The folder every recorded path is in: the shell's tests put their own temp folder in its place. */
const DIR = "/fixture";
/** The recorded FPP's address: the shell's tests put a fake FPP's in its place. */
const FPP = "192.0.2.10";

const id = (kind: number, n: number) => `${String(kind).repeat(8)}-0000-4000-8000-${String(n).padStart(12, "0")}`;
const [PROP, GROUP, CONTROLLER, ROW, EFFECT, TRACK, SEQUENCE, REGION] = [1, 2, 3, 4, 5, 6, 7, 8].map((kind) => id(kind, 1));

/** `api` with each method call noted, so every `invoke` and `listen` it makes is filed under it. */
function noting<T extends object>(name: string, api: T, used: Set<string>): T {
  return new Proxy(api, {
    get(target, key) {
      const member = Reflect.get(target, key);
      if (typeof member !== "function") return member;
      return (...args: unknown[]) => {
        recorder.current = `${name}.${String(key)}`;
        used.add(String(key));
        return member.apply(target, args);
      };
    },
  });
}

/** One value for each member of a union told apart by `tag`: a member left out doesn't compile. */
function each<U, T extends keyof U>(_tag: T, by: { [K in U[T] & string]: Extract<U, Record<T, K>> }): U[] {
  return Object.values(by);
}

/** Every member of a union of string literals: a member left out doesn't compile. */
function all<U extends string>(by: Record<U, true>): U[] {
  return Object.keys(by) as U[];
}

const prop: Prop = {
  id: PROP,
  name: "Mega Tree",
  shape: { source: "generator", type: "tree", strings: 16, nodesPerString: 50, height: 5, baseRadius: 1.5, topRadius: 0.1, serpentine: true, style: "flat", degrees: 180, startAngle: 10, start: "topRight", strandsPerString: 2, alternateNodes: true, spiralRotations: 1.5 },
  transform: { position: { x: 1, y: 2, z: 3 }, rotationDeg: { x: 10, y: 20, z: 30 }, scale: { x: 2, y: 2, z: 2 } },
  colorOrder: "GRB",
  regions: [
    { id: REGION, name: "Top", kind: "nodes", lines: [[{ first: 0, last: 9 }, null]], layout: "vertical", buffer: "stackedStrands" },
    { id: id(8, 2), name: "Left", kind: "subBuffer", x1: 0, y1: 0, x2: 50, y2: 100 },
    { id: id(8, 3), name: "Singer", kind: "face", mouths: { AI: [{ start: 0, end: 4 }] }, eyesOpen: [{ start: 4, end: 6 }], eyesClosed: [{ start: 6, end: 8 }], outline: [{ start: 8, end: 20 }], colors: { mouths: { AI: "#ff0000" }, eyesOpen: "#00ff00", eyesClosed: "#0000ff", outline: "#ffffff" } },
  ],
  tags: ["tree"],
};
const group: Group = { id: GROUP, name: "Everything", members: [PROP, { prop: PROP, region: REGION }], layout: "horizontalStack", gridSize: 200 };
const controller: Controller = {
  id: CONTROLLER,
  name: "Porch",
  address: FPP,
  adapter: "fpp",
  protocol: { type: "sacn", startUniverse: 4, universeSize: 255, allowPixelStraddle: true, multicast: true },
  ports: [{ number: 1, maxPixels: 680, brightness: 80, gamma: 2.2, slots: [{ prop: PROP, segment: { start: 0, end: 400 }, nullPixels: 1, reverse: true, brightness: 50, gamma: 1.8, smartReceiver: 2, controllerColorOrder: "RGB" }] }],
  sequenceChannels: { start: 1, count: 2400, rawDdpOffsets: true },
};
const entry: SequenceEntry = { id: SEQUENCE, name: "Carol", path: `${DIR}/sequences/Carol.fseq`, audio: `${DIR}/music/Carol.mp3`, offsetMs: -40 };

const effect: Effect = {
  id: EFFECT,
  startMs: 1000,
  endMs: 3000,
  params: { kind: "chase", speed: 4, width: 0.5, bands: 3, direction: "reverse", bounce: true, order: "across", timingTrack: TRACK },
  palette: { colors: ["#ff0000", "#00ff00"] },
  blend: "add",
  fadeInMs: 100,
  fadeOutMs: 200,
  sparkles: 40,
  sparkleColor: "#ffcc00",
  musicSparkles: true,
  blur: 2,
  renderStyle: "perPreview",
  bufferTransform: "rotateCw90",
  curves: { speed: { shape: "musicTrigger", from: 1, to: 9, cycles: 2, points: [[0, 0.5], [1, 1]], gain: 20, trigger: 40, fade: 3, timingTrack: TRACK } },
};
const row: Row = { id: ROW, target: { prop: PROP }, layers: [{ effects: [effect] }] };
const track: TimingTrack = { id: TRACK, name: "Lyrics (words)", kind: "words", marks: [{ startMs: 0, endMs: 400, label: "afraid", sung: "fraid" }, { startMs: 400, endMs: 900, label: "of the dark" }] };

const edits = each<Edit, "type">("type", {
  renameShow: { type: "renameShow", name: "House" },
  setFrameRate: { type: "setFrameRate", fps: 20 },
  addProp: { type: "addProp", prop },
  updateProp: { type: "updateProp", prop },
  removeProp: { type: "removeProp", id: PROP },
  addGroup: { type: "addGroup", group },
  updateGroup: { type: "updateGroup", group },
  removeGroup: { type: "removeGroup", id: GROUP },
  addController: { type: "addController", controller },
  updateController: { type: "updateController", controller: { ...controller, protocol: { type: "ddp" } } },
  removeController: { type: "removeController", id: CONTROLLER },
  addSequence: { type: "addSequence", sequence: entry },
  updateSequence: { type: "updateSequence", sequence: entry },
  removeSequence: { type: "removeSequence", id: SEQUENCE },
  moveSequence: { type: "moveSequence", id: SEQUENCE, index: 2 },
  setBackground: { type: "setBackground", background: { path: `${DIR}/photos/house.png`, x: -10, y: 8, width: 20, opacity: 0.6 } },
  setHouseModel: { type: "setHouseModel", houseModel: { path: `${DIR}/models/house.glb`, position: { x: 1, y: 2, z: -3 }, rotationDeg: { x: 5, y: 90, z: 15 }, scale: 1.5, opacity: 0.8 } },
});

const sequenceEdits = each<SequenceEdit, "type">("type", {
  updateInfo: { type: "updateInfo", name: "Carol", audio: `${DIR}/music/Carol.mp3`, durationMs: 60_000, frameMs: 50 },
  setRowBlending: { type: "setRowBlending", rowBlending: "blend" },
  setMusicLevelSource: { type: "setMusicLevelSource", musicLevelSource: "leftChannel" },
  addRow: { type: "addRow", row, index: 1 },
  removeRow: { type: "removeRow", id: ROW },
  moveRow: { type: "moveRow", id: ROW, index: 2 },
  addLayer: { type: "addLayer", row: ROW, index: 1 },
  removeLayer: { type: "removeLayer", row: ROW, layer: 1 },
  addEffect: { type: "addEffect", row: ROW, layer: 1, effect },
  updateEffect: { type: "updateEffect", effect },
  setEffectTiming: { type: "setEffectTiming", id: EFFECT, startMs: 500, endMs: 1500 },
  setEffectParams: { type: "setEffectParams", id: EFFECT, params: { kind: "picture", file: "images/star.gif", fit: "stretch", scale: 50 } },
  moveEffect: { type: "moveEffect", id: EFFECT, row: ROW, layer: 1, startMs: 500, endMs: 1500 },
  removeEffect: { type: "removeEffect", id: EFFECT },
  addTimingTrack: { type: "addTimingTrack", track },
  updateTimingTrack: { type: "updateTimingTrack", track },
  removeTimingTrack: { type: "removeTimingTrack", id: TRACK },
  renameTimingTrack: { type: "renameTimingTrack", id: TRACK, name: "Words" },
  moveTimingTrack: { type: "moveTimingTrack", id: TRACK, index: 2 },
  addMarks: { type: "addMarks", track: TRACK, marks: track.marks },
  setMark: { type: "setMark", track: TRACK, index: 1, mark: track.marks[0] },
  removeMarks: { type: "removeMarks", track: TRACK, indices: [1, 2] },
  splitMark: { type: "splitMark", track: TRACK, index: 1, atMs: 600 },
  mergeMarks: { type: "mergeMarks", track: TRACK, index: 1 },
  generateMarks: { type: "generateMarks", track: TRACK, everyMs: 500, fromMs: 1000, toMs: 5000 },
  copyMarks: { type: "copyMarks", from: TRACK, to: id(6, 2), every: 4 },
  spreadLyrics: { type: "spreadLyrics", track: TRACK, lines: ["Silent night", "Holy night"], fromMs: 1000, toMs: 9000 },
  labelMarks: { type: "labelMarks", track: TRACK, indices: [1, 2], labels: ["Silent", "night"] },
  breakIntoWords: { type: "breakIntoWords", track: TRACK, indices: [1, 2], words: id(6, 2) },
});

const targets = each<TargetSpec, "type">("type", {
  show: { type: "show" },
  prop: { type: "prop", id: PROP },
  group: { type: "group", id: GROUP },
  controller: { type: "controller", id: CONTROLLER },
  port: { type: "port", controller: CONTROLLER, port: 2 },
});
const patterns = all<PatternKind>({ solid: true, cycle: true, chase: true, ramp: true, alternate: true, identify: true, walk: true, cameraMap: true, cameraMapBinary: true });
const roles = each<FileRole, "kind">("kind", {
  sequence: { kind: "sequence", id: SEQUENCE },
  music: { kind: "music", id: SEQUENCE },
  photo: { kind: "photo" },
  houseModel: { kind: "houseModel" },
  sequenceDocMusic: { kind: "sequenceDocMusic" },
  picture: { kind: "picture" },
});
const sources = each<SendSource, "kind">("kind", {
  openSequence: { kind: "openSequence", name: "Carol" },
  file: { kind: "file", path: `${DIR}/sequences/Carol.fseq` },
});
const playlists = each<PlaylistChoice, "kind">("kind", {
  none: { kind: "none" },
  existing: { kind: "existing", name: "Main" },
  new: { kind: "new", name: "Carol" },
});
const pickKinds = all<PickKind>({
  show: true,
  showSave: true,
  xlightsFolder: true,
  xlightsSequence: true,
  fseq: true,
  fseqExport: true,
  videoExport: true,
  music: true,
  sequenceDoc: true,
  sequenceDocSave: true,
  timingFile: true,
  timingExport: true,
  downloadFolder: true,
  xlightsPackageFolder: true,
  xmap: true,
  xmapSave: true,
});
const decoded: DecodedCapture = {
  width: 48,
  height: 16,
  pixels: [
    { index: 0, x: 9, y: 8, confidence: 0.9, brightness: 220, seen: [0, 1, 2] },
    { index: 1, x: 19, y: 8, confidence: 0.4, brightness: 90, seen: null },
  ],
  duplicates: [{ index: 0, x: 9, y: 13, confidence: 0.5, brightness: 80, seen: null }],
  unreadable: [{ x: 40, y: 3, brightness: 60 }],
};

async function callTheBackend(backend: Backend) {
  await backend.getSnapshot();
  await backend.applyEdits(edits);
  await backend.applyEdits([
    { type: "setBackground", background: null },
    { type: "setHouseModel", houseModel: null },
  ]);
  await backend.undo();
  await backend.redo();
  await backend.newShow("House");
  await backend.openSampleShow();
  await backend.openShow(`${DIR}/House.pixelflow.json`);
  await backend.saveShow();
  await backend.saveShowAs(`${DIR}/House.pixelflow.json`);
  await backend.checkFiles(true);
  await backend.findMissingFiles();
  for (const role of roles) {
    await backend.findMissingFiles(role);
    await backend.locateFile(role);
  }
  await backend.setPlaybackSpeed(0.5);
  await backend.listHistory();
  await backend.restoreHistory("1700000000000.pixelflow.json");
  for (const [i, kind] of patterns.entries()) await backend.startOutput({ kind, color: "ff8000" }, targets[i % targets.length]);
  await backend.stopOutput();
  await backend.outputStatus();
  for (const base of all<CodeBase>({ four: true, two: true })) {
    await backend.cameraMapTarget(targets[1], base);
    await backend.cameraMapSync([{ t: 0, v: 10 }, { t: 0.033, v: 50 }], 4, base);
    await backend.cameraMapDecode(new Uint8Array(48 * 16 * 3), { width: 48, height: 16, pixels: 4, base });
  }
  await backend.cameraMapPlan(targets[1], 4, decoded, [0, 3]);
  await backend.discoverDevices([FPP, "192.0.2.20"], true);
  await backend.inspectDevice(FPP);
  await backend.importDevice(FPP);
  await backend.importDevice(FPP, { "port1/string2": PROP });
  await backend.compareDevice(FPP);
  await backend.takeFromDevice(FPP, ["port1/string2/pixels"]);
  await backend.takeFromDevice(FPP, ["port1/string3"], { "port1/string3": PROP });
  await backend.planDeviceSetup(FPP);
  await backend.sendDeviceSetup(FPP, ["port1/string2/pixels"]);
  await backend.planDeviceRestore(FPP);
  await backend.restoreDeviceSetup(FPP, ["port1/string2/pixels"]);
  await backend.forgetDeviceSetupCopy("fpp-4d312d46414b45");
  await backend.importFppDestination(FPP, "192.0.2.20", "DDP");
  await backend.checkControllers([FPP, "192.0.2.20"]);
  await backend.fppStatus(FPP);
  await backend.fppSequences(FPP);
  await backend.fppStart(FPP, "Carol.fseq");
  await backend.fppStop(FPP, true);
  for (const folder of all<FppFolder>({ sequences: true, music: true, playlists: true })) await backend.fppFolder(FPP, folder);
  await backend.fppSchedule(FPP);
  await backend.fppSetupPlan(FPP);
  await backend.fppSetUpShow(FPP, ["192.0.2.20"]);
  await backend.fppSoftware(FPP);
  await backend.openDevicePage(FPP);
  await backend.openDevicePage(FPP, "about.php");
  for (const link of all<ProjectLink>({ source: true, xlights: true, licence: true, credits: true, notices: true, githubSponsors: true, koFi: true, buyMeACoffee: true, payPal: true })) {
    await backend.openProjectLink(link);
  }
  await backend.fppSequenceNames(FPP);
  await backend.fppSendPlan(FPP, sources[0], `${DIR}/music/Carol.mp3`);
  await backend.fppSendPlan(FPP, sources[1], null);
  for (const [i, playlist] of playlists.entries()) {
    const request: FppSendRequest = {
      source: sources[i % sources.length],
      music: `${DIR}/music/Carol.mp3`,
      sequenceName: "Carol.fseq",
      musicName: "Carol.mp3",
      uploadMusic: true,
      replaceSequence: true,
      replaceMusic: true,
      playlist,
      sendId: 7,
    };
    await backend.fppSend(FPP, request, () => {});
  }
  await backend.cancelFppSend();
  await backend.pickDownloadFolder();
  await backend.fppDownloadPlan(FPP, "Carol.fseq", `${DIR}/Downloads`);
  await backend.fppDownloadPlan(FPP, "Carol.fseq", null);
  for (const clash of all<DownloadClash>({ replace: true, keepBoth: true })) {
    const request: FppDownloadRequest = { sequence: "Carol.fseq", music: "Carol.mp3", folder: `${DIR}/Downloads`, sequenceClash: clash, musicClash: clash, downloadId: 9 };
    await backend.fppDownload(FPP, request, () => {});
  }
  await backend.cancelFppDownload();
  await backend.startPlayback(`${DIR}/sequences/Carol.fseq`, 500);
  await backend.pausePlayback(true);
  await backend.seekPlayback(1500);
  await backend.stopPlayback();
  await backend.playbackStatus();
  await backend.playbackStopReason();
  await backend.liveFrame();
  await backend.sequenceFrame();
  await backend.previewProps();
  await backend.previewProps3d();
  await backend.readImage(`${DIR}/photos/house.png`);
  await backend.pickImagePath();
  await backend.readHouseModel(`${DIR}/models/house.glb`);
  await backend.pickHouseModelPath();
  await backend.importXlights(`${DIR}/xLights`);
  await backend.pickShowFolder();
  await backend.addSequence(`${DIR}/sequences/Carol.fseq`);
  await backend.addSequence(`${DIR}/sequences/Carol.fseq`, `${DIR}/music/Carol.mp3`);
  await backend.playSequence(SEQUENCE, 500);
  await backend.setPlaybackVolume(0.5);
  await backend.audioWaveform(`${DIR}/music/Carol.mp3`, 400);
  await backend.vocalLane(`${DIR}/music/Carol.mp3`);
  await backend.syncClickStart(500);
  await backend.syncClickPosition();
  await backend.syncClickStop();
  await backend.probeAudio(`${DIR}/music/Carol.mp3`, () => {});
  await backend.onAudioProgress(() => {});
  await backend.pickAudioPath();
  await backend.pickSequencePath();
  await backend.pickOpenPath();
  await backend.pickSavePath("House.pixelflow.json");
  await backend.onCloseRequested(() => true);
  await backend.closeWindow();
  await backend.openPreviewWindow();
  await backend.closePreviewWindow();
  await backend.previewLink("main");
  await backend.previewLink("main", { positionMs: 500 });
  await backend.listRecentShows();
  await backend.forgetRecentShow(`${DIR}/House.pixelflow.json`);
  await backend.clearRecentShows();
  await backend.locateRecentShow(`${DIR}/House.pixelflow.json`);
  await backend.onMenu(() => {});
}

async function callTheSequencer(sequencer: SequencerApi) {
  await sequencer.newSequenceDoc("Carol", 60_000, null);
  await sequencer.newSequenceDoc("Carol", 60_000, `${DIR}/music/Carol.mp3`, [row]);
  await sequencer.sequenceRecoveries();
  await sequencer.recoverSequence("1700000000000-1-0");
  await sequencer.discardSequenceRecovery("1700000000000-1-0");
  await sequencer.openSequenceDoc(`${DIR}/Carol.pfseq.json`);
  await sequencer.saveSequenceDoc();
  await sequencer.saveSequenceDocAs(`${DIR}/Carol.pfseq.json`);
  await sequencer.closeSequenceDoc();
  await sequencer.getSequenceDoc();
  await sequencer.editSequence(sequenceEdits);
  await sequencer.editSequence([{ type: "addRow", row }, { type: "addLayer", row: ROW }], "drag-7");
  await sequencer.undoSequence();
  await sequencer.redoSequence();
  await sequencer.effectCatalog();
  await sequencer.sequenceDocFrame(500);
  await sequencer.playSequenceDoc(500);
  await sequencer.setSequenceDocOutput(true);
  await sequencer.setSequenceDocLoop(true);
  await sequencer.addSequenceDocToShow(`${DIR}/sequences/Carol.fseq`);
  await sequencer.sequenceExportLayout();
  await sequencer.exportSequenceDoc(`${DIR}/sequences/Carol.fseq`, () => {});
  await sequencer.cancelSequenceExport();
  await sequencer.analyzeAudio(`${DIR}/music/Carol.mp3`);
  await sequencer.detectBeats();
  await sequencer.lyricsGate("anthropic");
  await sequencer.lyricsGate(null);
  await sequencer.findLyrics("openai", true, { language: "es", fresh: true, align: true }, () => {});
  await sequencer.findLyrics(null, false, { language: "en", fresh: false });
  await sequencer.chooseLyrics({ candidate: 2 }, { align: true });
  await sequencer.chooseLyrics({ pasted: "Silent night\nHoly night" });
  await sequencer.cancelLyrics();
  await sequencer.syllablesFromWords(TRACK);
  await sequencer.nudgeLyrics(TRACK, -50);
  await sequencer.retimeLyrics(TRACK);
  await sequencer.importTimingFile(`${DIR}/Vocals.xtiming`);
  await sequencer.exportTimingTrack(TRACK, `${DIR}/Vocals.xtiming`);
  await sequencer.pickTimingFilePath();
  await sequencer.pickTimingExportPath("Vocals.xtiming");
  await sequencer.importXlightsSequence(`${DIR}/Carol.xsq`);
  await sequencer.importXlightsSequence(`${DIR}/Vendor.zip`, { sequence: "Vendor/Carol.xsq", mapping: { items: { "Mega Tree": ["Mega Tree", "Everything"], Arch: [] } }, key: "vendor:abc", musicFolder: `${DIR}/Downloads` });
  await sequencer.inspectXlightsSequence(`${DIR}/Vendor.zip`);
  await sequencer.inspectXlightsSequence(`${DIR}/Vendor.zip`, "Vendor/Carol.xsq");
  await sequencer.readXmap(`${DIR}/vendor.xmap`);
  await sequencer.writeXmap(`${DIR}/vendor.xmap`, { items: { "Mega Tree": ["Mega Tree"] } });
  await sequencer.sequenceMusicMissing();
  await sequencer.findSequenceMusic();
  await sequencer.locateSequenceMusic();
  await sequencer.pickPicture();
  await sequencer.readPicture("images/star.gif");
  await sequencer.listPictures();
  await sequencer.listShaders();
  await sequencer.shaderInfo("shaders/rings.fs");
  await sequencer.pickShader();
  await sequencer.shaderStopped(EFFECT);
  await sequencer.checkSequencePictures();
  await sequencer.findSequencePictures();
  await sequencer.locateSequencePicture("C:\\xLights\\Images\\star.gif");
  sequencer.onPicturesArrived(() => {})();
  await sequencer.pickXlightsSequencePath();
  await sequencer.pickXlightsPackageFolder();
  await sequencer.pickXmapPath();
  await sequencer.pickXmapSavePath("vendor.xmap");
  await sequencer.pickSequenceDocPath();
  await sequencer.pickSequenceDocSavePath("Carol.pfseq.json");
  await sequencer.pickExportPath("Carol.fseq");
  await sequencer.videoExportChoices();
  await sequencer.exportVideo(`${DIR}/Carol.mp4`, { width: 1920, height: 1080, fps: 60, startMs: 1500, endMs: 9000, photo: true, pixelSize: 1.5, glow: 0.4, ffmpeg: true }, () => {});
  await sequencer.exportVideo(`${DIR}/Carol.mp4`, { width: 1280, height: 720, fps: 30, startMs: 0, endMs: null, photo: false, pixelSize: 1, glow: 0, ffmpeg: false });
  await sequencer.cancelVideoExport();
  await sequencer.pickVideoPath("Carol.mp4");
}

async function callTheAssistant(assistant: AssistantApi) {
  await assistant.keyStorage();
  await assistant.setApiKey("anthropic", "sk-test-not-a-key");
  await assistant.useKeyForSession("openai", "sk-test-not-a-key");
  await assistant.hasApiKey("anthropic");
  await assistant.keyLocation("openai");
  await assistant.deleteApiKey("openai");
  await assistant.listModels("anthropic");
  await assistant.send("anthropic", "claude-opus-5-5", "Make the tree twinkle", { screen: "sequence", selectedProps: [PROP], selectedEffects: [EFFECT], playheadMs: 1500 }, () => {}, true);
  await assistant.send("openai", "gpt-5-mini", "Hello", { selectedProps: [], selectedEffects: [] }, () => {});
  await assistant.stop();
  await assistant.newChat();
  await assistant.apply("proposal-1");
  await assistant.discard("proposal-1");
  await assistant.preview("proposal-1");
  await assistant.previewFrame("proposal-1", 1500);
  await assistant.alignModels();
  await assistant.downloadAlignModels(() => {});
  await assistant.cancelAlignDownload();
  await assistant.removeAlignModels();
  await assistant.sync();
}

async function callTheWindow(shellWindow: AppWindow, pickPath: (kind: PickKind, name?: string) => Promise<string | null>) {
  await shellWindow.setFullscreen(true);
  // The window asks whether it fills its display a moment after it last changed size.
  vi.useFakeTimers();
  const stop = shellWindow.onFullscreenChange(() => {});
  window.dispatchEvent(new Event("resize"));
  await vi.advanceTimersByTimeAsync(1000);
  stop();
  vi.useRealTimers();
  recorder.current = "pickPath";
  for (const kind of pickKinds) await pickPath(kind, kind.endsWith("Save") || kind.endsWith("Export") ? "Carol" : undefined);
}

describe("the calls the window makes to the shell", () => {
  /** The methods of each API object that weren't called. */
  const uncalled: Record<string, string[]> = {};

  beforeAll(async () => {
    // Loaded here, after the mocks are in place: the test setup has already loaded some of
    // these modules with the real Tauri API.
    vi.resetModules();
    const { tauriBackend, pickPath } = await import("../tauri");
    const { tauriSequencer } = await import("../sequencer");
    const { tauriAssistant } = await import("../assistant");
    const { appWindow } = await import("../appWindow");
    const run = async <T extends object>(name: string, api: T, calls: (api: T) => Promise<void>) => {
      const used = new Set<string>();
      await calls(noting(name, api, used));
      uncalled[name] = Object.keys(api).filter((method) => !used.has(method));
    };
    await run("backend", tauriBackend, callTheBackend);
    await run("sequencer", tauriSequencer, callTheSequencer);
    await run("assistant", tauriAssistant, callTheAssistant);
    await run("appWindow", appWindow, (api) => callTheWindow(api, pickPath));
  });

  it("come from every method of every API object", () => {
    // A method added to an API object has to be called above, so its call is recorded and checked.
    expect(uncalled).toEqual({ backend: [], sequencer: [], assistant: [], appWindow: [] });
  });

  it("are the ones in commandCalls.json", () => {
    const heard = (a: { api: string; event: string }, b: { api: string; event: string }) => a.api === b.api && a.event === b.event;
    const recording = { calls: recorder.calls, listens: recorder.listens.filter((l, i, all) => all.findIndex((other) => heard(l, other)) === i) };
    expect(recording.calls.length).toBeGreaterThan(200);
    expect(staleFile("commandCalls.json", recording, "`PIXELFLOW_UPDATE_CONTRACTS=1 npx vitest run src/api/contracts` in app, then `cargo test --locked -- contracts` in app/src-tauri")).toBeNull();
  });

  it("carry only what can be written down", () => {
    // Anything `invoke` is given that JSON can't hold (undefined, a function, bytes among the
    // arguments) would be recorded as something else than what's sent.
    for (const call of recorder.calls) {
      expect(JSON.parse(JSON.stringify(call)), `${call.api} → ${call.command}`).toEqual(call);
    }
  });
});
