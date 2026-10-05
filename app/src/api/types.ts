// TypeScript mirrors of the engine's JSON (see crates/pf-model and crates/pf-engine).

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

export interface Region {
  name: string;
  kind: "nodes" | "face";
  [key: string]: unknown;
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

export interface Group {
  id: Uuid;
  name: string;
  members: Uuid[];
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
  sequenceChannels: { start: number; count: number } | null;
}

export interface Show {
  schemaVersion: number;
  name: string;
  settings: { frameRate: number };
  props: Prop[];
  groups: Group[];
  controllers: Controller[];
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
  | { type: "removeController"; id: Uuid };

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
}

export interface Destination {
  address: string;
  description: string;
  protocol: string;
  channels: number;
  /** The first sequence channel (1-based) sent to this destination. */
  startChannel: number;
  startUniverse: number | null;
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
}

/** Where a prop's pixels are drawn in the preview (x, y pairs) and where their colors sit in a live frame. */
export interface PreviewProp {
  prop: Uuid;
  frameOffset: number;
  channelsPerPixel: number;
  points: number[];
}
