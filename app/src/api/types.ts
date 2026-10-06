// TypeScript mirrors of the engine's JSON (see crates/pf-model and crates/pf-engine).

import type { SequenceSnapshot } from "./sequence";

export type Uuid = string;

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export interface Transform {
  position: Vec3;
  rotationDeg: Vec3;
  scale: Vec3;
}

export type ColorOrder = "RGB" | "RBG" | "GRB" | "GBR" | "BRG" | "BGR" | "RGBW" | "GRBW";

export interface MatrixWiring {
  start: "bottomLeft" | "bottomRight" | "topLeft" | "topRight";
  orientation: "horizontal" | "vertical";
  serpentine: boolean;
}

export type Generator =
  | { type: "line"; nodes: number; length: number }
  | { type: "arch"; nodes: number; width: number; height: number }
  | { type: "circle"; nodes: number; radius: number }
  | { type: "matrix"; columns: number; rows: number; width: number; height: number; wiring?: MatrixWiring }
  | {
      type: "tree";
      strings: number;
      nodesPerString: number;
      height: number;
      baseRadius: number;
      topRadius: number;
      serpentine?: boolean;
    }
  | { type: "star"; points: number; nodes: number; outerRadius: number; innerRadius: number }
  | { type: "customGrid"; columns: number; rows: number; cells: number[] };

export type ShapeSource =
  | ({ source: "generator" } & Generator)
  | { source: "measured"; points: Vec3[]; provenance: "cameraMap" | "import" | "manual" };

export interface NodeRange {
  start: number;
  end: number;
}

/** A run of pixels on a submodel line: nodes `first` to `last` (0-based, both included); `first > last` runs backwards. */
export interface NodeRun {
  first: number;
  last: number;
}

/** One line of a submodel: runs of pixels, with `null` for an empty spot. */
export type SubmodelLine = (NodeRun | null)[];

/** Each line is a row (first at the bottom) or a column (first on the left). */
export type LineLayout = "horizontal" | "vertical";

/** How effects see a submodel's pixels: lines side by side, where they really are, or all on top of each other. */
export type BufferStyle = "default" | "keepXY" | "stackedStrands";

/** Mouth shapes (Preston Blair phonemes), as written in show files. */
export type Phoneme = "AI" | "E" | "ETC" | "FV" | "L" | "MBP" | "O" | "REST" | "U" | "WQ";

/** Colors a face was made with; a part without one is white. */
export interface FaceColors {
  mouths?: Partial<Record<Phoneme, string>>;
  eyesOpen?: string;
  eyesClosed?: string;
  outline?: string;
}

/** Which pixels make each part of a singing face. */
export interface FaceDefinition {
  mouths: Partial<Record<Phoneme, NodeRange[]>>;
  eyesOpen: NodeRange[];
  eyesClosed: NodeRange[];
  outline: NodeRange[];
  colors?: FaceColors;
}

/** A named part of a prop: a submodel (lines of pixels, or a rectangle of the prop) or a singing face. */
export type Region = { id: Uuid; name: string } & (
  | { kind: "nodes"; lines: SubmodelLine[]; layout: LineLayout; buffer: BufferStyle }
  | { kind: "subBuffer"; x1: number; y1: number; x2: number; y2: number }
  | ({ kind: "face" } & FaceDefinition)
);

/** A submodel as a group member. */
export interface RegionRef {
  prop: Uuid;
  region: Uuid;
}

export interface Prop {
  id: Uuid;
  name: string;
  shape: ShapeSource;
  transform: Transform;
  colorOrder: ColorOrder;
  regions: Region[];
  tags: string[];
}

/** A group member: a whole prop (its id) or one of a prop's submodels. */
export type GroupMember = Uuid | RegionRef;

/** A named, ordered set of props and submodels; effects that run along the group follow this order. */
export interface Group {
  id: Uuid;
  name: string;
  members: GroupMember[];
}

export interface PortSlot {
  prop: Uuid;
  segment: NodeRange | null;
  nullPixels: number;
  reverse: boolean;
  brightness: number | null;
  gamma: number | null;
  smartReceiver: number | null;
}

export interface Port {
  number: number;
  maxPixels: number | null;
  brightness: number;
  gamma: number;
  slots: PortSlot[];
}

export type Protocol =
  | {
      type: "sacn";
      startUniverse: number | null;
      universeSize: 510 | 512;
      allowPixelStraddle: boolean;
      multicast: boolean;
    }
  | { type: "ddp" };

export interface Controller {
  id: Uuid;
  name: string;
  address: string;
  adapter: "fpp" | "falcon" | "wled" | "generic";
  protocol: Protocol;
  ports: Port[];
  /** Where this controller's data sits in a rendered sequence (channels from 1), when known. */
  sequenceChannels: { start: number; count: number; rawDdpOffsets?: boolean } | null;
}

/** A rendered sequence in the show, with its music. */
export interface SequenceEntry {
  id: Uuid;
  name: string;
  /** The .fseq file. */
  path: string;
  /** The music file, when the sequence has one. */
  audio: string | null;
  /** How far the lights run ahead of the music, in ms (negative: behind). */
  offsetMs: number;
}

/** A photo of the house drawn behind the layout. Its height follows the image's shape. */
export interface Background {
  /** The image file. */
  path: string;
  /** Layout position of the photo's top-left corner. */
  x: number;
  y: number;
  /** Width in layout units. */
  width: number;
  /** 0 (hidden) to 1 (full strength). */
  opacity: number;
}

/** A 3D model of the house (glTF/GLB or OBJ) for the 3D view, placed in layout units. */
export interface HouseModel {
  /** The model file. */
  path: string;
  position: Vec3;
  rotationDeg: Vec3;
  /** One factor for all three axes. */
  scale: number;
  /** 0 (hidden) to 1 (solid). */
  opacity: number;
}

export interface Show {
  schemaVersion: number;
  name: string;
  settings: { frameRate: number };
  props: Prop[];
  groups: Group[];
  controllers: Controller[];
  sequences: SequenceEntry[];
  /** Missing in shows from before the layout editor. */
  background?: Background | null;
  /** Missing unless a house model was chosen. */
  houseModel?: HouseModel | null;
}

export interface Issue {
  severity: "warning" | "error";
  code: string;
  message: string;
  fix: string | null;
}

export interface PropLayout {
  prop: Uuid;
  frameOffset: number;
  nodes: number;
  channelsPerPixel: number;
}

export interface UniverseSpan {
  universe: number;
  controllerChannel: number;
  len: number;
}

export interface OutputSpan {
  prop: Uuid;
  port: number;
  controllerChannel: number;
  frameOffset: number;
  pixels: number;
  channelsPerPixel: number;
  reverse: boolean;
  colorOrder: ColorOrder;
  brightness: number;
  gamma: number;
}

export interface ControllerOutput {
  controller: Uuid;
  channelCount: number;
  addressing: { type: "sacn"; universes: UniverseSpan[]; multicast: boolean } | { type: "ddp" };
  spans: OutputSpan[];
}

export interface ChannelMap {
  frameLen: number;
  props: PropLayout[];
  controllers: ControllerOutput[];
}

export interface Summary {
  props: number;
  pixels: number;
  controllers: number;
  universes: number;
}

export interface ShowSnapshot {
  revision: number;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  show: Show;
  issues: Issue[];
  channelMap: ChannelMap;
  summary: Summary;
}

export type Edit =
  | { type: "renameShow"; name: string }
  | { type: "setFrameRate"; fps: number }
  | { type: "addProp"; prop: Prop }
  | { type: "updateProp"; prop: Prop }
  | { type: "removeProp"; id: Uuid }
  | { type: "addGroup"; group: Group }
  | { type: "updateGroup"; group: Group }
  | { type: "removeGroup"; id: Uuid }
  | { type: "addController"; controller: Controller }
  | { type: "updateController"; controller: Controller }
  | { type: "removeController"; id: Uuid }
  | { type: "addSequence"; sequence: SequenceEntry }
  | { type: "updateSequence"; sequence: SequenceEntry }
  | { type: "removeSequence"; id: Uuid }
  | { type: "moveSequence"; id: Uuid; index: number }
  | { type: "setBackground"; background: Background | null }
  | { type: "setHouseModel"; houseModel: HouseModel | null };

export type PatternKind = "solid" | "cycle" | "chase" | "ramp" | "alternate" | "identify" | "walk";

export interface PatternSpec {
  kind: PatternKind;
  color: string;
}

export type TargetSpec =
  | { type: "show" }
  | { type: "prop"; id: Uuid }
  | { type: "group"; id: Uuid }
  | { type: "controller"; id: Uuid }
  | { type: "port"; controller: Uuid; port: number };

export interface ControllerStatus {
  id: Uuid;
  name: string;
  state: "ok" | "degraded" | "unresolved";
  packetsSent: number;
  sendErrors: number;
  lastError: string | null;
}

export interface OutputStatus {
  running: boolean;
  generation: number;
  pattern: PatternSpec | null;
  target: TargetSpec | null;
  frames: number;
  lateFrames: number;
  achievedFps: number;
  controllers: ControllerStatus[];
  /** Why output stopped on its own (show errors, empty target, restart failure); null otherwise. */
  stopReason: string | null;
}

export interface HistoryEntry {
  id: string;
  savedAtMs: number;
  sizeBytes: number;
}

// Devices (see crates/pf-devices).

export type DeviceKind = "fpp" | "falcon" | "wled";
export type FoundBy = "ping" | "webSweep" | "mdns" | "fppPeer" | "manual";

export interface Device {
  address: string;
  kind: DeviceKind;
  name: string;
  model: string;
  firmware: string;
  mode: string | null;
  foundBy: FoundBy[];
}

export interface SilentPeer {
  address: string;
  description: string;
  listedBy: string;
}

export interface Discovery {
  devices: Device[];
  /** Controllers an FPP listed that didn't answer. */
  silent: SilentPeer[];
}

export type DeviceInput =
  | { type: "ddp" }
  | { type: "sacn"; startUniverse: number; channelsPerUniverse: number; universeCount: number }
  | { type: "unsupported"; description: string };

export interface StringConfig {
  name: string | null;
  pixels: number;
  colorOrder: ColorOrder;
  nullPixels: number;
  reverse: boolean;
  brightness: number;
  gamma: number;
  smartReceiver: number | null;
}

export interface PortConfig {
  number: number;
  strings: StringConfig[];
  /** Most RGB pixels the port drives as the board is set up, when known (Falcon). */
  maxPixels: number | null;
}

export interface Destination {
  address: string;
  description: string;
  protocol: string;
  channels: number;
  /** The first sequence channel (1-based) sent to this destination. */
  startChannel: number;
  startUniverse: number | null;
  /** Channels per universe, for sACN destinations; null for DDP. */
  universeSize: number | null;
  /** DDP only: the FPP sends raw channel numbers (packet offsets are absolute channels). */
  ddpRaw: boolean;
  /** Merged sACN ranges that aren't one back-to-back run of equal-sized universes. */
  unevenUniverses: boolean;
}

export interface DeviceConfig {
  input: DeviceInput;
  ports: PortConfig[];
  destinations: Destination[];
  notes: string[];
}

export interface ImportPlan {
  controller: Controller;
  props: Prop[];
  notes: string[];
  alreadyInShow: boolean;
  canImport: boolean;
}

export interface DeviceDetails {
  device: Device;
  config: DeviceConfig;
  plan: ImportPlan;
}

export type PlayerState = "idle" | "playing" | "paused" | "stopping" | "other";

/** What an FPP is playing. */
export interface PlayerStatus {
  state: PlayerState;
  playlist: string | null;
  sequence: string | null;
  secondsElapsed: number;
  secondsRemaining: number;
  nextPlaylist: string | null;
  nextStart: string | null;
  /** Problems FPP itself reports, such as an output target it can't reach. */
  warnings: string[];
}

/** A sequence stored on an FPP. */
export interface FppSequence {
  name: string;
  frames: number;
  stepMs: number;
  channels: number;
}

/** A sequence playing on the controllers. */
export interface PlaybackStatus {
  state: "playing" | "paused" | "ended";
  path: string;
  positionMs: number;
  durationMs: number;
  frameMs: number;
  controllers: ControllerStatus[];
  /** Plain-language notes, such as controllers that were left out and why. */
  notes: string[];
  /** Why playback stopped by itself (a damaged file, for example). */
  error: string | null;
  /** The show's sequence being played, if any. */
  sequence: Uuid | null;
  /** The music playing along, if any. */
  music: string | null;
  offsetMs: number;
  volume: number;
  /** True when playing an authored sequence document (see api/sequence.ts) rather than a file. */
  authored: boolean;
  /** Plays again from the top each time it reaches the end, music and lights together. */
  looping: boolean;
}

/** A song's length and peak loudness (0–1) in equal slices of time. */
export interface Waveform {
  durationMs: number;
  peaks: number[];
}

/** Where a prop's pixels are drawn in the preview (x, y pairs) and where their colors sit in a live frame. */
export interface PreviewProp {
  prop: Uuid;
  frameOffset: number;
  channelsPerPixel: number;
  /** x, y pairs, one per pixel, in wiring order. */
  points: ArrayLike<number>;
}

/** Every prop's pixel positions, and the show revision they were worked out for. */
export interface PreviewSet {
  revision: number;
  props: PreviewProp[];
}

/** Where a prop's pixels are in 3D (x right, y up, z toward the street) and where their colors sit in a live frame. */
export interface PreviewProp3d {
  prop: Uuid;
  frameOffset: number;
  channelsPerPixel: number;
  /** x, y, z triples, one per pixel, in wiring order. */
  xyz: Float32Array;
}

/** Every prop's pixel positions in 3D, and the show revision they were worked out for. */
export interface PreviewSet3d {
  revision: number;
  props: PreviewProp3d[];
}

/** Counts from an xLights import. */
export interface ImportSummary {
  props: number;
  pixels: number;
  controllers: number;
  /** Props wired onto a controller. */
  wired: number;
  groups: number;
}

/** The show an xLights import produced, with a report of anything not imported exactly. */
export interface XlightsImported {
  snapshot: ShowSnapshot;
  summary: ImportSummary;
  notes: string[];
}

/** Counts from importing an xLights sequence. */
export interface SequenceImportSummary {
  rows: number;
  /** Effects imported: exact + approximate + placeholders. */
  effects: number;
  exact: number;
  approximate: number;
  /** No PixelFlow equivalent yet: a dim fill in the effect's first color. */
  placeholders: number;
  /** xLights effects not imported (models not in the show, submodels, limits). */
  skipped: number;
  timingTracks: number;
  marks: number;
  lyricMarks: number;
  /** Timing marks not imported (outside the sequence, limits). */
  marksSkipped: number;
}

/** The sequence an xLights sequence import opened, with a report of anything not imported exactly. */
export interface XlightsSequenceImported {
  snapshot: SequenceSnapshot;
  summary: SequenceImportSummary;
  notes: string[];
}
