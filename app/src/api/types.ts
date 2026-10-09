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

/** One stretch of a poly line, from one of its points to the next. */
export interface PolySegment {
  /** Pixels on this stretch (unused while the line spreads its pixels evenly). */
  nodes: number;
  /** The two control points of a curved stretch (a cubic Bézier), prop-local; absent when straight. */
  curve?: [Vec3, Vec3] | null;
}

/** Where a star's pixels start: its top tip, the inner corner at its bottom, or a bottom leg's tip. */
export type StarStart = "top" | "bottom" | "leftLeg" | "rightLeg";

export type Generator =
  | { type: "line"; nodes: number; length: number }
  /** Arches in a row (`nodes` pixels each), or with `layers` one arch of nested layers, laid out as xLights does. Settings left out read as one half-ellipse arch. */
  | {
      type: "arch";
      nodes: number;
      /** Between each arch's feet. */
      width: number;
      /** From the feet to the top. */
      height: number;
      arches?: number;
      /** Degrees of the ellipse each arch goes round, 1–180 (default 180). */
      arc?: number;
      /** Between one arch's right foot and the next one's left foot. */
      gap?: number;
      /** Lean in degrees, positive to the left. */
      skewDeg?: number;
      startRight?: boolean;
      /** Pixels on each layer, innermost first; empty or absent for plain arches. */
      layers?: number[];
      /** The innermost layer's size, percent of the outermost (default 70). */
      hollow?: number;
      zigZag?: boolean;
      startInside?: boolean;
    }
  /** Rings of pixels from the top, clockwise; with `layers` (pixels per ring, innermost first) several rings evenly spaced in to `innerPercent` of the radius. */
  | {
      type: "circle";
      nodes: number;
      radius: number;
      layers?: number[];
      innerPercent?: number;
      startInside?: boolean;
      startAtBottom?: boolean;
      counterClockwise?: boolean;
    }
  | { type: "matrix"; columns: number; rows: number; width: number; height: number; wiring?: MatrixWiring }
  | {
      type: "tree";
      strings: number;
      nodesPerString: number;
      height: number;
      baseRadius: number;
      topRadius: number;
      serpentine?: boolean;
      /** Round (a cone, the default), or fanned out flat, or a ribbon. */
      style?: "round" | "flat" | "ribbon";
      /** How far round a round tree goes (default 360). */
      degrees?: number;
      /** Where the first string of a round tree stands, degrees round from the front (default 0). */
      startAngle?: number;
      /** The corner the first string starts at: a top corner runs it down, a right one goes round the other way. */
      start?: MatrixWiring["start"];
      /** Zig-zag starts afresh every this many strings (each string folded into that many); 0 never. */
      strandsPerString?: number;
      /** Each string goes up every other spot and comes back down the ones between. */
      alternateNodes?: boolean;
      /** Turns a round tree's strings wind round from base to top, as xLights winds them. */
      spiralRotations?: number;
    }
  /** Star outlines, pixels evenly along each from the `start` corner, clockwise; with `layers` (pixels per outline, innermost first) nested outlines in to `innerPercent` of the size. */
  | {
      type: "star";
      points: number;
      nodes: number;
      outerRadius: number;
      innerRadius: number;
      start?: StarStart;
      counterClockwise?: boolean;
      layers?: number[];
      innerPercent?: number;
      startInside?: boolean;
    }
  | { type: "customGrid"; columns: number; rows: number; cells: number[] }
  /** Points the line runs through, first to last; `segments` has one fewer. `spreadNodes` spreads that many pixels evenly instead. */
  | { type: "polyLine"; vertices: Vec3[]; segments: PolySegment[]; spreadNodes?: number | null }
  /** A row of candy canes between two ends, `width` apart, laid out as xLights does. `height` scales the canes and hooks, `caneHeight` stretches them taller. */
  | {
      type: "candyCanes";
      canes: number;
      nodesPerCane: number;
      width: number;
      height: number;
      caneHeight: number;
      reverse: boolean;
      sticks: boolean;
      alternateNodes: boolean;
      skewDeg: number;
      /** The first cane is the rightmost (the data comes in there). */
      startRight?: boolean;
    }
  /** Icicles hanging from a line `width` long: each string fills drops of the `drops` pattern in turn; the longest hangs `dropHeight` below the line. */
  | { type: "icicles"; strings: number; lightsPerString: number; drops: number[]; width: number; dropHeight: number; alternateNodes: boolean }
  /** A window frame `width` by `height`: one string once round it from the `start` corner, spaced as xLights does. */
  | {
      type: "windowFrame";
      top: number;
      sides: number;
      bottom: number;
      width: number;
      height: number;
      start: MatrixWiring["start"];
      counterClockwise: boolean;
    }
  /** A ring of pixels rounded to a square grid, as xLights places wreath lights. */
  | { type: "wreath"; nodes: number; radius: number; startAtBottom: boolean; counterClockwise: boolean }
  /** Arms radiating from a hollow middle (`hollow` percent), the first pointing down turned by `startAngle`, spread over `arc` degrees; `radius` reaches the outermost pixel. */
  | {
      type: "spinner";
      arms: number;
      nodesPerArm: number;
      hollow: number;
      startAngle: number;
      arc: number;
      zigZag: boolean;
      alternate: boolean;
      fromCenter: boolean;
      clockwise: boolean;
      radius: number;
    }
  /** A globe of `columns` strands of `rows` pixels between two latitudes, round `degrees` of it. */
  | {
      type: "sphere";
      columns: number;
      rows: number;
      radius: number;
      startLatitude?: number;
      endLatitude?: number;
      degrees?: number;
      start?: MatrixWiring["start"];
      strandStyle?: StrandStyle;
    }
  /** A cube of `width` × `height` × `depth` pixels `spacing` apart, wired from a corner as xLights does. */
  | {
      type: "cube";
      width: number;
      height: number;
      depth: number;
      spacing: number;
      start?: CubeStart;
      style?: CubeStyle;
      strandStyle?: StrandStyle;
      strandPerLayer?: boolean;
    };

/** How pixels run along each strand of a sphere or cube. */
export type StrandStyle = "zigZag" | "noZigZag" | "alternatePixel";
export type CubeStart =
  | "frontBottomLeft"
  | "frontBottomRight"
  | "frontTopLeft"
  | "frontTopRight"
  | "backBottomLeft"
  | "backBottomRight"
  | "backTopLeft"
  | "backTopRight";
export type CubeStyle = "verticalFrontBack" | "verticalLeftRight" | "horizontalFrontBack" | "horizontalLeftRight" | "stackedFrontBack" | "stackedLeftRight";

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

/** How effects lay a group's pixels out (xLights' group layouts). */
export type GroupLayout =
  | "minimalGrid"
  | "grid"
  | "horizontalPerModel"
  | "verticalPerModel"
  | "horizontalStack"
  | "verticalStack"
  | "horizontalStackScaled"
  | "verticalStackScaled"
  | "singleLine"
  | "overlayCentered"
  | "overlayScaled"
  | "singleLineModelAsPixel"
  | "defaultModelAsPixel"
  | "perModelDefault";

/** A named, ordered set of props and submodels; effects that run along the group follow this order. */
export interface Group {
  id: Uuid;
  name: string;
  members: GroupMember[];
  /** How effects lay the group out, unless an effect picks its own render style (the minimal grid when missing). */
  layout?: GroupLayout;
  /** The most cells along the longer side of the group's grid, 10 to 4000 (400 when missing). */
  gridSize?: number;
}

export interface PortSlot {
  prop: Uuid;
  segment: NodeRange | null;
  nullPixels: number;
  reverse: boolean;
  brightness: number | null;
  gamma: number | null;
  smartReceiver: number | null;
  /** The color order the controller itself applies to this string ("Send setup" sets it on the
   * controller); absent or null leaves the controller's setting alone. */
  controllerColorOrder?: ColorOrder | null;
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
      /** Channels in each universe, 1–512 (510 is exactly 170 RGB pixels). */
      universeSize: number;
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
  /** The layout's area from its origin, for groups on the whole layout's grid (from xLights). */
  layoutArea?: { width: number; height: number } | null;
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
  /** The open sequence's revision: changes when undo or redo also took back (or brought back) a
   * sequence change made together with the show change. Absent where there's no such pairing. */
  sequenceRevision?: number | null;
  /** The show's files (sequences, music, the photo, the house model) that aren't where it says. */
  missingFiles: MissingFile[];
  /** False until every file the show refers to has been looked at (ask with checkFiles):
   * snapshots never look at the disk, so only files looked at can be called missing. */
  filesChecked: boolean;
}

/**
 * Which file the show (or the open sequence) refers to. Paths in the show are "path text": a
 * path as written, where a byte that isn't UTF-8 (Linux) is a NUL followed by two hex digits.
 * Show them with fileName; pass them back to the backend unchanged.
 */
export type FileRole =
  | { kind: "sequence"; id: Uuid }
  | { kind: "music"; id: Uuid }
  | { kind: "photo" }
  | { kind: "houseModel" }
  | { kind: "sequenceDocMusic" };

/** A file that isn't where the show (or the open sequence) says it is. */
export interface MissingFile {
  file: FileRole;
  /** The file's name ("Christmas Medley 2017.mp3"). */
  name: string;
  /** Where the show looks for it now. */
  path: string;
  /** Where it was when the show was saved (`path`, unless the show file moved without it). */
  wasAt: string;
  /** What it belongs to ("Music for Medley", "Background photo"). */
  owner: string;
  /** "Christmas Medley 2017.mp3 isn't where it was." */
  message: string;
}

/** A missing file found again, and now used. */
export interface FoundFile {
  file: FileRole;
  name: string;
  /** Where it was. */
  from: string;
  /** Where it is now. */
  to: string;
  /** Other files that fit just as well, for the user to choose with Locate… if `to` is wrong. */
  also: string[];
}

/** What looking for the show's missing files did (all of it is one undo step). */
export interface FilesFound {
  snapshot: ShowSnapshot;
  found: FoundFile[];
  stillMissing: MissingFile[];
  /** True when the search stopped before looking everywhere (it took too long). */
  gaveUp: boolean;
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

export type PatternKind = "solid" | "cycle" | "chase" | "ramp" | "alternate" | "identify" | "walk" | "cameraMap" | "cameraMapBinary";

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

// Camera mapping (see crates/pf-camera-map).

/** How the camera-mapping sequence shows each digit: off/red/green/blue, or off/white. */
export type CodeBase = "four" | "two";

export interface CameraMapProp {
  prop: Uuid;
  name: string;
  nodes: number;
  /** How many of its nodes the capture lights (fewer when only part of it is on the target). */
  covered: number;
}

/** What a capture of a target covers. */
export interface CameraMapTargetInfo {
  pixels: number;
  /** One pass of the sequence, in seconds (it loops). */
  seconds: number;
  props: CameraMapProp[];
}

/** One video frame's overall brightness at `t` seconds. */
export interface BrightnessSample {
  t: number;
  v: number;
}

/** Where the sequence starts in the video, and the span of video to average for each slot. */
export interface CameraMapSync {
  start: number;
  score: number;
  windows: [number, number][];
}

/** The slot frames sent for decoding: their size and the sequence they're from. */
export interface CameraMapFrames {
  width: number;
  height: number;
  pixels: number;
  base: CodeBase;
}

export interface FoundPixel {
  /** Its place in the sequence (wiring order across the target). */
  index: number;
  x: number;
  y: number;
  confidence: number;
  brightness: number;
  /** Camera colour seen for red, green, blue (0 red, 1 green, 2 blue); null when unclear. */
  seen: [number, number, number] | null;
}

export interface DecodedCapture {
  width: number;
  height: number;
  pixels: FoundPixel[];
  duplicates: FoundPixel[];
  unreadable: { x: number; y: number; brightness: number }[];
}

export interface Similarity {
  scale: number;
  angle: number;
  tx: number;
  ty: number;
}

export interface GeneratorFit {
  scale: number;
  rotationDeg: number;
  tx: number;
  ty: number;
  error: number;
  fits: boolean;
}

export interface PropPlan {
  nodes: number;
  found: number;
  /** Every node's measured layout position (x, y); empty when none was found. */
  points: [number, number][];
  measured: boolean[];
  fit: GeneratorFit | null;
}

export type CameraMapAnomaly =
  | { kind: "missing"; prop: number; ranges: [number, number][] }
  | { kind: "duplicate"; prop: number; node: number; x: number; y: number }
  | { kind: "reversed"; prop: number }
  | { kind: "jump"; prop: number; node: number }
  | { kind: "colorOrder"; prop: number; configured: string; suggested: string }
  | { kind: "unreadable"; count: number };

export interface CameraMapPlan {
  props: CameraMapProp[];
  plan: {
    alignment: Similarity | null;
    alignmentError: number;
    props: PropPlan[];
    anomalies: CameraMapAnomaly[];
  };
}

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
  /** Addresses that answered but asked for a password (an FPP with its UI or API password on). */
  locked: string[];
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

/** Whether a controller answered a quick, read-only look. */
export interface ControllerCheck {
  address: string;
  answering: boolean;
  /** Whether it's on one of this computer's networks; null when that isn't known. */
  onLocalNetwork: boolean | null;
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
  /** Props already in the show the device's strings most likely are, by string key: what "In
   * your show" starts on. */
  suggested: Record<string, PropMatch>;
}

/** Why a prop already in the show is suggested for a device string. */
export type MatchReason = "samePort" | "sameName" | "sameNameOtherSize";

export interface PropMatch {
  prop: Uuid;
  reason: MatchReason;
}

export interface DeviceDetails {
  device: Device;
  config: DeviceConfig;
  plan: ImportPlan;
}

/** Props already in the show to wire to a device's strings, by string key ("port1/string2"). */
export type UseProps = Record<string, Uuid>;

export type ChangeKind =
  | "pixels"
  | "colorOrder"
  | "start"
  | "stringAdded"
  | "stringRemoved"
  | "receives"
  | "startUniverse"
  | "universeSize"
  | "setting";

/** One difference between a controller in the show and the device, before → after. */
export interface Change {
  /** Stable for the same difference; a new string's id is its string key. */
  id: string;
  /** Its port; null for the controller as a whole. */
  port: number | null;
  kind: ChangeKind;
  /** The string it's about ("String 2 · Gutter"), or empty. */
  subject: string;
  what: string;
  before: string;
  after: string;
  /** What this change undoes or turns off, in plain words. */
  warning: string | null;
  /** Compare only: the device's value can be taken into the show. */
  canTake: boolean;
  whyNot: string | null;
}

/** "Compare with this device": show (before) → device (after). Changes nothing. */
export interface DeviceComparison {
  device: Device;
  controllerName: string;
  changes: Change[];
  notes: string[];
}

/** "Send setup to this device…": device (before) → show (after). Changes nothing yet. */
export interface SendPlan {
  device: Device;
  controllerName: string;
  changes: Change[];
  notes: string[];
  /** Why this can't be sent as it is (the controller wouldn't load it, for instance). */
  problems: string[];
  /** What the device is busy with that sending would interrupt. */
  busy: string | null;
  canSend: boolean;
  /** Why it can't be sent, when it can't. */
  reason: string | null;
  /** The kept copy of this device's setup, which Put back sends. */
  restorePoint: RestorePointInfo | null;
}

/** A kept copy of a device's setup, from before PixelFlow first changed it. */
export interface RestorePointInfo {
  /** What forgetting it takes. */
  key: string;
  deviceName: string;
  /** The address it was read from. */
  address: string;
  takenAtMs: number;
}

/** What putting the kept copy back would change: the device now (before) → the copy (after). */
export interface RestorePlan {
  device: Device;
  copy: RestorePointInfo;
  changes: Change[];
  canRestore: boolean;
  reason: string | null;
}

export type SendStatus = "sent" | "mismatch" | "failed" | "refused";

export interface SendReport {
  status: SendStatus;
  message: string;
  /** What still differs after reading back (device → show). */
  mismatches: Change[];
  /** The setup from just before sending can be put back. */
  canRestore: boolean;
  /** Worth knowing, but not caused by this send. */
  notes: string[];
}

export interface RestoreReport {
  restored: boolean;
  message: string;
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

/** One of an FPP's folders. */
export type FppFolder = "sequences" | "music" | "playlists";

/** A sequence, music file, or playlist stored on an FPP. */
export interface FppFile {
  /** Its name on the FPP (a sequence's with ".fseq"; a playlist's without ".json"). */
  name: string;
  sizeBytes: number | null;
  /** When it last changed, "YYYY-MM-DD HH:MM" by the FPP's clock. */
  modified: string | null;
  durationMs: number | null;
  /** A sequence's channel count. */
  channels: number | null;
  /** A playlist's item count. */
  items: number | null;
}

/** A newer FPP release that fits the box (see crates/pf-devices fpp_software). */
export interface FppUpdateNotice {
  /** e.g. "10.2". */
  version: string;
  /** The exact file to choose in FPP's Upgrade OS list. */
  file: string;
  /** "Pi-", "Pi64-", "BBB-", or "BB64-". */
  prefix: string;
  /** A new major version: back up first. */
  major: boolean;
}

/** What an FPP runs, and whether a newer release fits it. */
export interface FppSoftware {
  version: string;
  /** The OS image it was installed from, e.g. "v2025-11". */
  osBuild: string;
  osRelease: string;
  platform: string;
  /** 32 or 64, when the kernel says. */
  bits: number | null;
  imagePrefix: string | null;
  update: FppUpdateNotice | null;
  /** False when FPP's release list couldn't be read. */
  checked: boolean;
}

export type ScheduleKind = "playlist" | "sequence" | "command";

/** One entry of an FPP's schedule, as FPP saved it (see crates/pf-devices fpp_info). */
export interface ScheduleEntry {
  enabled: boolean;
  kind: ScheduleKind;
  name: string;
  /** FPP's day code: 0–6 Sunday–Saturday, 7 every day, 8 weekdays, 9 weekends, 10 Mon/Wed/Fri,
   * 11 Tue/Thu, 12 Sun–Thu, 13 Fri/Sat, 14 odd days, 15 even days, or 0x10000 plus a bit per day
   * (0x4000 Sunday down to 0x100 Saturday). */
  day: number;
  /** "HH:MM:SS", or SunRise, SunSet, Dawn, Dusk (with the offset in minutes). */
  startTime: string;
  startOffset: number;
  endTime: string;
  endOffset: number;
  /** "YYYY-MM-DD" (year 0000: every year) or a holiday name; empty for no limit. */
  startDate: string;
  endDate: string;
  /** 0 plays once, 1 repeats straight away, otherwise every repeat / 100 minutes. */
  repeat: number;
  /** 0 stops gracefully, 1 at once, 2 gracefully after the loop. */
  stopType: number;
}

/** An output target that setting up the show from an FPP leaves out, and why. */
export interface SetupSkip {
  name: string;
  address: string;
  reason: string;
}

/** What "Set up my show from this FPP" adds, as one undo step. */
export interface FppSetupPlan {
  /** The FPP's own outputs, when it has pixel ports and isn't in the show yet. */
  own: ImportPlan | null;
  /** A controller per output target not in the show yet, with the channels the FPP sends it. */
  controllers: Controller[];
  skipped: SetupSkip[];
  notes: string[];
}

/** What to send to an FPP: the open sequence (exported for it, named `name` there), or one of
 * the show's `.fseq` files as it is. */
export type SendSource = { kind: "openSequence"; name: string } | { kind: "file"; path: string };

/** A file name on an FPP, whether it's taken, and the name that would keep both. */
export interface NameCheck {
  /** The name PixelFlow would give the file. */
  name: string;
  /** A file by that name is on the FPP, whatever its capitals. */
  exists: boolean;
  /** The clashing file's name on the FPP, exactly as the FPP spells it (replace or reuse that). */
  fppName: string | null;
  keepBothName: string;
}

/** What sending would do, read from the FPP before anything is sent. */
export interface FppSendPlan {
  sequence: NameCheck;
  music: NameCheck | null;
  playlists: string[];
  /** A name for a new playlist (the sequence's). */
  newPlaylistName: string;
  /** Free space on the FPP, when it says. */
  freeBytes: number | null;
  /** Where the sequence's channels don't match the FPP's outputs (sending still works). */
  layoutWarnings: string[];
}

export type PlaylistChoice = { kind: "none" } | { kind: "existing"; name: string } | { kind: "new"; name: string };

/** What the user chose in the Send to FPP dialog. */
export interface FppSendRequest {
  source: SendSource;
  /** The music on this computer, if any. */
  music: string | null;
  /** File names on the FPP: as planned, the keep-both names, or (to replace or reuse) the FPP's
   * own spelling. */
  sequenceName: string;
  musicName: string | null;
  /** False to use the copy of the music already on the FPP. */
  uploadMusic: boolean;
  /** The user chose to replace the FPP's file of that name; without it nothing is replaced. */
  replaceSequence: boolean;
  replaceMusic: boolean;
  playlist: PlaylistChoice;
  /** Tells this send's progress apart from any other's. */
  sendId: number;
}

/** "commit" and "playlist" come after the commit point: files are moving into place, and Cancel
 * no longer applies. */
export type FppSendStep = "export" | "sequence" | "music" | "commit" | "playlist";

export interface FppSendProgress {
  sendId: number;
  step: FppSendStep;
  percent: number;
  done: number;
  total: number;
}

/** What a send put on the FPP. */
export interface FppSendResult {
  sequenceName: string;
  musicName: string | null;
  playlist: string | null;
  /** What "Play it now" starts (a playlist or a sequence file). */
  playName: string;
  notes: string[];
}

/** A file to download from an FPP: where it goes, and whether a file there has its name. */
export interface FppDownloadName {
  /** Its name on the FPP. */
  name: string;
  sizeBytes: number | null;
  /** The folder it goes in (the show folder's "sequences" or "music"). */
  folder: string;
  /** That folder already has a file by this name. */
  exists: boolean;
  /** The name Keep both saves it under. */
  keepBothName: string;
}

/** What downloading a sequence from an FPP would save, read before anything is fetched. */
export interface FppDownloadPlan {
  /** The show's folder (or the one picked while the show isn't saved). */
  folder: string;
  sequence: FppDownloadName;
  /** The music its mf header names, when the FPP has it. */
  music: FppDownloadName | null;
  /** The music file the sequence names that isn't on the FPP. */
  missingMusic: string | null;
  channels: number | null;
  showChannels: number;
  /** Plain words when the sequence's channel count doesn't match the show's. */
  channelWarning: string | null;
}

/** What to do about a file of the same name already in the folder. */
export type DownloadClash = "replace" | "keepBoth";

export interface FppDownloadRequest {
  /** The sequence's name on the FPP, with ".fseq". */
  sequence: string;
  /** The music's name on the FPP, to download with it. */
  music: string | null;
  /** The folder picked while the show isn't saved (else the show's folder is used). */
  folder: string | null;
  /** Null: nothing in the folder is replaced. */
  sequenceClash: DownloadClash | null;
  musicClash: DownloadClash | null;
  downloadId: number;
}

export interface FppDownloadProgress {
  downloadId: number;
  step: "sequence" | "music";
  percent: number;
  done: number;
  total: number;
}

/** Where a download saved its files. */
export interface FppDownloadResult {
  sequencePath: string;
  musicPath: string | null;
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

/** A music file's length and format, found from what the file says about itself where it can. */
export interface AudioInfo {
  durationMs: number;
  sampleRate: number;
  channels: number;
  /** The codec's short name ("mp3", "aac", "flac", "vorbis", "pcm_s16le"). */
  codec: string;
  /** Where the length came from: the file's header, counting an MP3's frames, or decoding it all. */
  foundBy: "header" | "frames" | "decoding";
}

/** Long work on a music file: its length (when the header doesn't say), its waveform, the audio
 * track effects follow, and its beats. */
export type AudioTask = "probe" | "waveform" | "audioTrack" | "beats";

/** How far long work on a music file has got. */
export interface AudioProgress {
  task: AudioTask;
  /** The music file, as the window named it. */
  path: string;
  /** What's being done ("Reading the music"). */
  stage: string;
  /** How much is done (0–1); 1 when it's over, done or not. */
  fraction: number;
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

/** A show opened or saved lately (the shell keeps the list; the window only reads it). */
export interface RecentShow {
  /** The show file, as path text: pass it back unchanged. */
  path: string;
  name: string;
  /** When it was last opened or saved (milliseconds since 1970). */
  openedAt: number;
  props: number;
  pixels: number;
  controllers: number;
  /** A small SVG picture of the layout, or null. */
  thumbnail: string | null;
  /** Whether the file is still there ("unknown": its drive didn't answer in time). */
  status: "here" | "missing" | "unknown";
}

/** A File menu item chosen in the menu bar (macOS). */
export type MenuAction =
  | { action: "newShow" }
  | { action: "openShow" }
  | { action: "openRecent"; path: string }
  | { action: "clearRecent" }
  | { action: "closeShow" }
  | { action: "save" }
  | { action: "saveAs" }
  | { action: "undo" }
  | { action: "redo" };

/** What a file dialog is for; the shell picks its filters, title, and starting folder. */
export type PickKind =
  | "show"
  | "showSave"
  | "xlightsFolder"
  | "xlightsSequence"
  | "fseq"
  | "fseqExport"
  | "videoExport"
  | "music"
  | "sequenceDoc"
  | "sequenceDocSave"
  | "timingFile"
  | "timingExport"
  | "downloadFolder"
  | "xlightsPackageFolder"
  | "xmap"
  | "xmapSave";

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

/** Where each vendor item's effects go: the item ("Model", "Model/Submodel", "Model/Strand 1")
 * to the show's props, groups, and submodels ("Prop/Submodel") by name. An empty list skips it. */
export interface VendorMapping {
  items: Record<string, string[]>;
}

/** What a vendor item or a target is ("model" is a prop, in the show). */
export type VendorItemKind = "model" | "group" | "submodel" | "strand";

/** The kind of prop, for matching like with like and for icons. */
export type VendorPropType =
  | "tree"
  | "arch"
  | "matrix"
  | "canes"
  | "line"
  | "window"
  | "star"
  | "circle"
  | "wreath"
  | "spinner"
  | "sphere"
  | "cube"
  | "icicles"
  | "snowflake"
  | "flood"
  | "other";

/** Why a mapping was suggested. */
export type VendorMatchReason = "saved" | "exact" | "alias" | "name" | "type" | "size" | "none";

/** A vendor model, group, submodel, or strand with effects in the sequence. */
export interface VendorItem {
  name: string;
  label: string;
  parent: string | null;
  kind: VendorItemKind;
  type: VendorPropType;
  displayAs: string | null;
  /** Effects on the item itself (not its submodels or strands). */
  effects: number;
  /** Lights, when the vendor's layout says (0 when unknown). */
  pixels: number;
}

/** A prop, group, or submodel in the show effects can go to. */
export interface VendorTarget {
  name: string;
  label: string;
  parent: string | null;
  kind: Exclude<VendorItemKind, "strand">;
  type: VendorPropType;
  pixels: number;
}

/** The best idea for one vendor item; applied when confidence is at least VENDOR_AUTO_MAP. */
export interface VendorSuggestion {
  item: string;
  targets: string[];
  confidence: number;
  reason: VendorMatchReason;
}

/** The least confidence a suggestion is applied with (the engine's AUTO_MAP_CONFIDENCE). */
export const VENDOR_AUTO_MAP = 0.5;

/** What's in a vendor package, and a suggested mapping onto the open show. */
export interface VendorInspection {
  /** The sequences in the package (paths inside it), best first. */
  sequences: string[];
  sequence: string;
  song: string;
  hasLayout: boolean;
  items: VendorItem[];
  targets: VendorTarget[];
  /** One per item, in `items` order. */
  suggestions: VendorSuggestion[];
  /** The suggestions applied, and the mapping saved for this vendor last time. */
  mapping: VendorMapping;
  /** What the mapping is remembered under. */
  key: string;
  /** Every item has a prop of the same name (the user's own sequence): nothing to map. */
  allExact: boolean;
  /** A zip's music file, copied next to the show on import. */
  music: string | null;
  /** Where that music goes; null while the show isn't saved (choose a folder first). */
  musicFolder: string | null;
}

/** How to import a vendor sequence (see SequencerApi.importXlightsSequence). */
export interface VendorImportOptions {
  sequence: string;
  mapping: VendorMapping;
  key: string;
  /** A folder picked for the music while the show isn't saved. */
  musicFolder: string | null;
}

/** A mapping read from an xLights .xmap file. */
export interface XmapRead {
  mapping: VendorMapping;
  /** Lines mapping single nodes, which PixelFlow doesn't import. */
  nodesSkipped: number;
}
