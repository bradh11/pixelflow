// TypeScript mirrors of authored sequences (see crates/pf-sequence, crates/pf-render's export,
// crates/pf-analysis, and crates/pf-engine's SequenceEdit), plus helpers that build valid values.

import type { Show, Uuid } from "./types";

/** A color as `#rrggbb`. */
export type Rgb = string;

export type EffectKind =
  | "on"
  | "off"
  | "colorWash"
  | "fade"
  | "chase"
  | "bars"
  | "wave"
  | "twinkle"
  | "shimmer"
  | "strobe"
  | "spiral"
  | "fire"
  | "meteors"
  | "ripple"
  | "shape"
  | "fan"
  | "morph"
  | "circles"
  | "pinwheel"
  | "snowflakes"
  | "plasma"
  | "butterfly"
  | "garlands"
  | "lines"
  | "life"
  | "tendril"
  | "text"
  | "faces"
  | "vuMeter"
  | "impact"
  | "wipe"
  | "lightning"
  | "pulse"
  | "sing"
  | "colorShift"
  | "dancer";

export type Gradient = "none" | "horizontal" | "vertical";
/** What the Shape effect draws. */
export type ShapeObject =
  | "circle"
  | "ellipse"
  | "triangle"
  | "square"
  | "pentagon"
  | "hexagon"
  | "octagon"
  | "star"
  | "heart"
  | "tree"
  | "snowflake"
  | "candyCane"
  | "crucifix"
  | "present"
  | "random";
export type Direction = "forward" | "reverse";
export type Axis = "horizontal" | "vertical";
/** How an effect mixes with the layers below it (see `Blend` in crates/pf-sequence/src/effect.rs). */
export type Blend =
  | "normal"
  | "add"
  | "subtract"
  | "max"
  | "min"
  | "multiply"
  | "average"
  | "over"
  | "behind"
  | "mask"
  | "reveal"
  | "revealBrightness"
  | "cutOut"
  | "clip"
  | "clipBrightness"
  | "shadow"
  | "shadowBelow"
  | "highlight"
  | "highlightAdd"
  | "bottomHalf"
  | "leftHalf";

/** Settings for each kind of effect; missing settings take the engine's defaults. */
export type EffectParams =
  | { kind: "on"; gradient?: Gradient; startLevel?: number; endLevel?: number }
  | { kind: "off" }
  | { kind: "colorWash"; cycles?: number; gradient?: Gradient }
  | { kind: "fade"; direction?: "in" | "out" }
  | {
      kind: "chase";
      speed?: number;
      width?: number;
      bands?: number;
      direction?: Direction;
      bounce?: boolean;
      /** Along the wiring, left to right, or prop by prop on a group. */
      order?: "wiring" | "across" | "props" | "propsAcross";
      /** Steps on this track's marks instead of moving at the speed. */
      timingTrack?: Uuid | null;
    }
  | { kind: "bars"; count?: number; speed?: number; axis?: Axis; direction?: Direction }
  | { kind: "wave"; cycles?: number; speed?: number; height?: number; thickness?: number; direction?: Direction }
  | { kind: "twinkle"; density?: number; rate?: number }
  | { kind: "shimmer"; rate?: number; duty?: number }
  | { kind: "strobe"; rate?: number; density?: number }
  | { kind: "spiral"; count?: number; speed?: number; thickness?: number; twist?: number; direction?: Direction }
  | { kind: "fire"; height?: number; sparks?: number }
  | { kind: "meteors"; count?: number; speed?: number; length?: number; direction?: "down" | "up" | "left" | "right" }
  | { kind: "ripple"; speed?: number; spacing?: number; thickness?: number }
  | {
      kind: "shape";
      shape?: ShapeObject;
      count?: number;
      lifetime?: number;
      startSize?: number;
      growth?: number;
      thickness?: number;
      fade?: boolean;
      randomLocation?: boolean;
      rotation?: number;
      points?: number;
      centerX?: number;
      centerY?: number;
      speed?: number;
      direction?: number;
      randomMovement?: boolean;
      randomStart?: boolean;
      /** Shapes appear at this track's marks instead of `count` at once. */
      timingTrack?: Uuid | null;
      /** Shapes appear when the music gets louder than `triggerLevel` (0–100). */
      fireOnMusic?: boolean;
      triggerLevel?: number;
    }
  | {
      kind: "fan";
      centerX?: number;
      centerY?: number;
      startRadius?: number;
      endRadius?: number;
      blades?: number;
      bladeWidth?: number;
      revolutions?: number;
      bladeAngle?: number;
      duration?: number;
      startAngle?: number;
      elements?: number;
      elementWidth?: number;
      acceleration?: number;
      direction?: Direction;
      blendEdges?: boolean;
      scale?: boolean;
    }
  | {
      kind: "morph";
      startX1?: number;
      startY1?: number;
      startX2?: number;
      startY2?: number;
      endX1?: number;
      endY1?: number;
      endX2?: number;
      endY2?: number;
      headDuration?: number;
      startLength?: number;
      endLength?: number;
      acceleration?: number;
      repeats?: number;
      repeatSpacing?: number;
      stagger?: number;
      headAtStart?: boolean;
      autoRepeat?: boolean;
    }
  | {
      kind: "circles";
      count?: number;
      size?: number;
      speed?: number;
      look?: "solid" | "fading" | "bubbles" | "plasma" | "radial" | "rainbowRadial";
      bounce?: boolean;
      centerX?: number;
      centerY?: number;
    }
  | {
      kind: "pinwheel";
      arms?: number;
      armSize?: number;
      twist?: number;
      thickness?: number;
      speed?: number;
      counterclockwise?: boolean;
      shading?: "flat" | "raised" | "sunken" | "sweep";
      offset?: number;
      centerX?: number;
      centerY?: number;
      style?: "smooth" | "spokes";
    }
  | {
      kind: "snowflakes";
      count?: number;
      flake?: "random" | "dot" | "cross" | "bar" | "bigCross" | "star" | "square" | "plus" | "diamond" | "x";
      speed?: number;
      motion?: "blowing" | "falling" | "pilingUp";
      warmup?: number;
    }
  | { kind: "plasma"; colors?: "palette" | "redGreen" | "blueGreen" | "rainbow" | "white"; twist?: number; density?: number; speed?: number }
  | {
      kind: "butterfly";
      pattern?: number;
      colors?: "rainbow" | "palette";
      speed?: number;
      direction?: Direction;
      chunks?: number;
      skip?: number;
    }
  | {
      kind: "garlands";
      shape?: "straight" | "smallSwags" | "swags" | "deepSwags" | "doubleDips";
      spacing?: number;
      cycles?: number;
      direction?: "up" | "down" | "left" | "right" | "upThenDown" | "downThenUp" | "leftThenRight" | "rightThenLeft";
    }
  | { kind: "lines"; count?: number; points?: number; thickness?: number; speed?: number; trails?: number; fadeTrails?: boolean }
  | {
      kind: "life";
      density?: number;
      rules?: "classic" | "b35S236" | "amoeba" | "coagulations" | "b25678S5678";
      speed?: number;
    }
  | {
      kind: "tendril";
      movement?:
        | "random"
        | "square"
        | "circle"
        | "horizontalZigZag"
        | "horizontalZigZagReturn"
        | "verticalZigZag"
        | "verticalZigZagReturn"
        | "musicLine"
        | "musicCircle"
        | "manual";
      movementSize?: number;
      thickness?: number;
      tendrils?: number;
      length?: number;
      speed?: number;
      friction?: number;
      dampening?: number;
      tension?: number;
      offsetX?: number;
      offsetY?: number;
      manualX?: number;
      manualY?: number;
    }
  | {
      kind: "text";
      /** What it says; `\n` starts a new line. */
      text?: string;
      movement?:
        | "none"
        | "left"
        | "right"
        | "up"
        | "down"
        | "upLeft"
        | "downLeft"
        | "upRight"
        | "downRight"
        | "vector"
        | "wavy"
        | "leftRight"
        | "upDown";
      speed?: number;
      size?: number;
      orientation?: "across" | "stackedDown" | "stackedUp";
      toCenter?: boolean;
      noRepeat?: boolean;
      startX?: number;
      startY?: number;
      endX?: number;
      endY?: number;
      pixelOffsets?: boolean;
      colorPerWord?: boolean;
      countdown?: "none" | "seconds" | "minutesSeconds";
    }
  | {
      kind: "faces";
      /** One of the prop's faces by name; blank: its first face. */
      face?: string;
      timingTrack?: Uuid | null;
      eyes?: "open" | "auto" | "closed";
      colors?: "face" | "palette";
      outline?: boolean;
    }
  | {
      kind: "vuMeter";
      meter?: VuMeterType;
      bars?: number;
      sensitivity?: number;
      gain?: number;
      timingTrack?: Uuid | null;
      shape?: VuMeterShape;
      slowFalls?: boolean;
      startNote?: number;
      endNote?: number;
      logX?: boolean;
      xOffset?: number;
      yOffset?: number;
      filter?: string;
    }
  | {
      kind: "impact";
      decay?: "linear" | "exponential" | "punch";
      color?: "white" | "palette";
      colorShift?: boolean;
      bloom?: number;
      hold?: number;
      centerX?: number;
      centerY?: number;
    }
  | { kind: "wipe"; direction?: Sweep; mode?: "on" | "onOff" | "off"; duration?: number; softness?: number; band?: number }
  | {
      kind: "lightning";
      density?: number;
      branches?: number;
      flashOnly?: boolean;
      glow?: number;
      thickness?: number;
      segments?: number;
    }
  | {
      kind: "pulse";
      source?: "marks" | "level" | "bass" | "onsets";
      timingTrack?: Uuid | null;
      shape?: "sine" | "saw" | "square" | "heartbeat";
      min?: number;
      max?: number;
      attack?: number;
      release?: number;
    }
  | { kind: "sing"; mode?: "mouth" | "wordPop" | "barMouth" | "karaoke"; timingTrack?: Uuid | null; min?: number }
  | { kind: "colorShift"; ease?: "instant" | "linear" | "smooth"; duration?: number; stagger?: number; direction?: Sweep }
  | {
      kind: "dancer";
      character?: DancerCharacter;
      moves?: "mix" | "bounce" | "armWave" | "kick" | "twist" | "shuffle" | "jump" | "headBob";
      /** The beats it dances to; none: the song's Beats track, else two a second. */
      timingTrack?: Uuid | null;
      speed?: "half" | "normal" | "double";
      size?: number;
      mirror?: boolean;
      count?: number;
      usePalette?: boolean;
      bassBounce?: number;
      x?: number;
      y?: number;
      stagger?: number;
      background?: "off" | "glow";
      routine?: number;
    };

/** Who a Dancer is (see `DancerCharacter` in crates/pf-sequence/src/effect.rs). */
export type DancerCharacter = "skeleton" | "ghost" | "witch" | "santa" | "snowman" | "elf";

/** Which way a Wipe (or a staggered Color Shift) travels (see `Sweep` in crates/pf-sequence/src/effect.rs). */
export type Sweep = "leftToRight" | "rightToLeft" | "up" | "down" | "centerOut" | "edgesIn" | "diagonal" | "radial";

/** What a VU Meter draws (see `VuMeterType` in crates/pf-sequence/src/effect.rs). */
export type VuMeterType =
  | "spectrogram"
  | "spectrogramPeak"
  | "spectrogramLine"
  | "spectrogramCircleLine"
  | "volumeBars"
  | "waveform"
  | "on"
  | "colorOn"
  | "dominantFrequencyColor"
  | "dominantFrequencyColorGradient"
  | "intensityWave"
  | "pulse"
  | "levelBar"
  | "levelRandomBar"
  | "levelColor"
  | "levelPulse"
  | "levelPulseColor"
  | "levelJump"
  | "levelJump100"
  | "levelShape"
  | "timingEventBar"
  | "timingEventBarBounce"
  | "timingEventRandomBar"
  | "timingEventBars"
  | "timingEventSpike"
  | "timingEventSweep"
  | "timingEventSweep2"
  | "timingEventTimedSweep"
  | "timingEventTimedSweep2"
  | "timingEventAlternateTimedSweep"
  | "timingEventAlternateTimedSweep2"
  | "timingEventChaseFromMiddle"
  | "timingEventChaseToMiddle"
  | "timingEventColor"
  | "timingEventJump"
  | "timingEventJump100"
  | "timingEventPulse"
  | "timingEventPulseColor"
  | "noteOn"
  | "noteLevelPulse"
  | "noteLevelJump"
  | "noteLevelJump100"
  | "noteLevelBar"
  | "noteLevelRandomBar";

/** The shape a VU Meter's Level Shape draws. */
export type VuMeterShape =
  | "circle"
  | "filledCircle"
  | "square"
  | "filledSquare"
  | "diamond"
  | "filledDiamond"
  | "star"
  | "filledStar"
  | "tree"
  | "filledTree"
  | "crucifix"
  | "filledCrucifix"
  | "present"
  | "filledPresent"
  | "candyCane"
  | "snowflake"
  | "heart"
  | "filledHeart";

export interface Palette {
  colors: Rgb[];
}

export type CurveShape =
  | "ramp"
  | "sine"
  | "square"
  | "saw"
  | "custom"
  | "music"
  | "invertedMusic"
  | "musicTrigger"
  | "timingToggle"
  | "timingFade"
  | "timingFadeSpan";

/**
 * A setting that changes over its effect (see `Curve` in crates/pf-sequence/src/curve.rs): its
 * value goes from `from` to `to` in the shape; sine, square, and saw repeat `cycles` times (1 when
 * missing), and a custom curve goes through `points` (`[time 0–1, level 0–1]`, level 0 being
 * `from` and 1 `to`; two points at one time make a step).
 */
export interface Curve {
  shape: CurveShape;
  from: number;
  to: number;
  cycles?: number;
  points?: [number, number][];
  /** music, invertedMusic: boosts the music's level, -100 to 100 (%). */
  gain?: number;
  /** musicTrigger: how loud (0–100) the music must get. */
  trigger?: number;
  /** musicTrigger, timingFade: frames to fade over; timingFadeSpan: % of the gap to the next mark. */
  fade?: number;
  /** The timing shapes: the track whose marks drive the curve. */
  timingTrack?: Uuid | null;
}

/** How an effect lays out its target's pixels (xLights' render styles). */
export type RenderStyle =
  | "default"
  | "perPreview"
  | "singleLine"
  | "asPixel"
  | "horizontalPerModel"
  | "verticalPerModel"
  | "horizontalStack"
  | "verticalStack"
  | "horizontalStackScaled"
  | "verticalStackScaled"
  | "overlayCentered"
  | "overlayScaled"
  | "singleLineModelAsPixel"
  | "defaultModelAsPixel"
  | "perModelDefault"
  | "perModelPerPreview"
  | "perModelSingleLine";

/** Turns or flips the layout an effect draws on (xLights' buffer transformations). */
export type BufferTransform =
  | "none"
  | "rotateCw90"
  | "rotateCcw90"
  | "rotate180"
  | "flipVertical"
  | "flipHorizontal"
  | "rotateCw90FlipHorizontal"
  | "rotateCcw90FlipHorizontal";

export interface Effect {
  id: Uuid;
  startMs: number;
  /** Exclusive. */
  endMs: number;
  params: EffectParams;
  palette: Palette;
  blend: Blend;
  fadeInMs: number;
  fadeOutMs: number;
  /** Sparkles on the lit pixels, 0 (none, when missing) to 200 (most). */
  sparkles?: number;
  /** The sparkles' color (white when missing). */
  sparkleColor?: Rgb;
  /** Sparkles follow the music: as many as it's loud, up to `sparkles` (off when missing). */
  musicSparkles?: boolean;
  /** Softening, 0 (none, when missing) to 14. */
  blur?: number;
  /** How the target's pixels are laid out for the effect (its own layout when missing). */
  renderStyle?: RenderStyle;
  /** Turns or flips that layout (as it is when missing). */
  bufferTransform?: BufferTransform;
  /** Settings that change over the effect, by key (a `params` number setting, `sparkles`, or
   * `blur`); none when missing. */
  curves?: Record<string, Curve>;
}

/**
 * Layers draw bottom (first, index 0) to top (last): the opposite of xLights, where layer 1 is on
 * top. An effect's `blend` mixes only with the layers below it on the same row (the lowest effect
 * drawn covers, whatever its blend); rows don't blend with each other — a later row covers an earlier one where it's lit.
 */
export interface Layer {
  effects: Effect[];
}

export type SequenceTarget = { prop: Uuid } | { group: Uuid } | { region: { prop: Uuid; region: Uuid } };

export interface Row {
  id: Uuid;
  target: SequenceTarget;
  layers: Layer[];
}

export type TimingKind = "beats" | "bars" | "sections" | "lyrics" | "words" | "phonemes" | "custom";

export interface Mark {
  startMs: number;
  endMs: number;
  label: string;
  /** How a sung word's label is said, when not as it's spelled ("fraid" for "afraid"): its
   * syllables and mouth shapes come from this. Dropped when the label changes. */
  sung?: string;
}

export interface TimingTrack {
  id: Uuid;
  name: string;
  kind: TimingKind;
  marks: Mark[];
}

/** An authored sequence (`*.pfseq.json`). Later rows draw over earlier ones. */
export interface Sequence {
  schemaVersion: number;
  name: string;
  audio: string | null;
  durationMs: number;
  frameMs: number;
  timingTracks: TimingTrack[];
  rows: Row[];
}

export interface SequenceIssue {
  severity: "warning" | "error";
  message: string;
  row?: Uuid;
  effect?: Uuid;
}

/** A sequence that wasn't saved when PixelFlow last closed, kept so it can be recovered. */
export interface SequenceRecovery {
  /** Pass back to recover or discard it. */
  id: string;
  name: string;
  /** The file it was opened from or last saved to; null when it was never saved. */
  path: string | null;
  /** When it was last kept (milliseconds since 1970). */
  savedAtMs: number;
}

/** The whole open sequence: when one is opened, created, or saved, and from getSequenceDoc (resync). */
export interface SequenceSnapshot {
  revision: number;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  sequence: Sequence;
  issues: SequenceIssue[];
}

/** The sequence's name, music, length, and frame time. */
export interface SequenceInfo {
  name: string;
  audio: string | null;
  durationMs: number;
  frameMs: number;
}

/** An effect and where it now sits: `index` within `layer` of row `row`. */
export interface PlacedEffect {
  row: Uuid;
  layer: number;
  index: number;
  effect: Effect;
}

/**
 * What an edit, undo, or redo changed; bring a copy of the document up to date with
 * `applySequenceChanges`. Edits list single effects where they can; rows whose layers changed (and
 * every row an undo or redo touched) come whole in `rows`.
 */
export interface SequenceChanges {
  info: SequenceInfo | null;
  rows: Row[];
  removedRows: Uuid[];
  rowOrder: Uuid[] | null;
  effects: PlacedEffect[];
  removedEffects: Uuid[];
  timingTracks: TimingTrack[];
  removedTimingTracks: Uuid[];
  trackOrder: Uuid[] | null;
}

/** The reply to an edit, undo, or redo: what changed, not the whole document. */
export interface SequenceEditResult {
  /** Increases on every change (also when an edit merges into its gesture's undo step). */
  revision: number;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  /** False when nothing changed. */
  changed: boolean;
  changes: SequenceChanges;
  /** Problems in the whole sequence now, errors first. */
  issues: SequenceIssue[];
  /** The show's revision now: it changes when undo or redo also took back (or brought back) a show
   * change made together with this one. Absent where there's no such pairing. */
  showRevision?: number;
}

/** No changes (for building replies). */
export function noChanges(): SequenceChanges {
  return {
    info: null,
    rows: [],
    removedRows: [],
    rowOrder: null,
    effects: [],
    removedEffects: [],
    timingTracks: [],
    removedTimingTracks: [],
    trackOrder: null,
  };
}

function byOrder<T extends { id: Uuid }>(items: T[], order: Uuid[]): T[] {
  const rank = new Map(order.map((id, i) => [id, i]));
  return [...items].sort((a, b) => (rank.get(a.id) ?? Infinity) - (rank.get(b.id) ?? Infinity));
}

function upsert<T extends { id: Uuid }>(items: T[], updates: T[]): T[] {
  const out = [...items];
  for (const item of updates) {
    const at = out.findIndex((x) => x.id === item.id);
    if (at >= 0) out[at] = item;
    else out.push(item);
  }
  return out;
}

/** `doc` with `changes` applied (a new object; `doc` is left as it was). */
export function applySequenceChanges(doc: Sequence, changes: SequenceChanges): Sequence {
  const next: Sequence = { ...doc };
  if (changes.info) Object.assign(next, changes.info);
  let rows = upsert(
    doc.rows.filter((r) => !changes.removedRows.includes(r.id)),
    changes.rows,
  );
  if (changes.rowOrder) rows = byOrder(rows, changes.rowOrder);
  if (changes.effects.length > 0 || changes.removedEffects.length > 0) {
    const gone = new Set([...changes.removedEffects, ...changes.effects.map((p) => p.effect.id)]);
    const whole = new Set(changes.rows.map((r) => r.id));
    const placed = [...changes.effects].sort((a, b) => a.index - b.index);
    rows = rows.map((row) => {
      const mine = placed.filter((p) => p.row === row.id);
      if (whole.has(row.id) || (mine.length === 0 && !row.layers.some((l) => l.effects.some((e) => gone.has(e.id))))) {
        return row;
      }
      const layers = row.layers.map((layer) => ({ effects: layer.effects.filter((e) => !gone.has(e.id)) }));
      for (const p of mine) layers[p.layer].effects.splice(p.index, 0, p.effect);
      return { ...row, layers };
    });
  }
  let tracks = upsert(
    doc.timingTracks.filter((t) => !changes.removedTimingTracks.includes(t.id)),
    changes.timingTracks,
  );
  if (changes.trackOrder) tracks = byOrder(tracks, changes.trackOrder);
  next.rows = rows;
  next.timingTracks = tracks;
  return next;
}

/** One choice in a list setting. */
export interface ChoiceOption {
  value: string;
  label: string;
}

interface SettingBase {
  /** The key in the effect's `params`. */
  key: string;
  label: string;
  description: string;
  /** A setting most people leave alone: the panel keeps it under "More". */
  more?: boolean;
}

/** One effect setting, with the control that edits it (`type`), its range, and its default. */
export type EffectSetting = SettingBase &
  (
    | { type: "number"; min: number; max: number; step: number; default: number; unit?: string }
    | { type: "int"; min: number; max: number; step: number; default: number; unit?: string }
    | { type: "bool"; default: boolean }
    | { type: "choice"; default: string; options: ChoiceOption[] }
    /** One of the row's prop's faces, by name ("" = its first face). */
    | { type: "face"; default: string }
    /** Free text. */
    | { type: "text"; default: string }
    /** One of the sequence's timing tracks, by id. */
    | { type: "timingTrack"; default: Uuid | null }
  );

/**
 * An effect kind for the settings panel, from the engine's effect catalog: the same table the
 * engine clamps files to and checks edits against, so a value inside these ranges is accepted.
 */
export interface EffectInfo {
  kind: EffectKind;
  label: string;
  description: string;
  settings: EffectSetting[];
}

/** Every setting of `info` at its default, as effect params. */
export function defaultParams(info: EffectInfo): EffectParams {
  const params: Record<string, unknown> = { kind: info.kind };
  for (const setting of info.settings) params[setting.key] = setting.default;
  return params as EffectParams;
}

/** How far an export has got (sent about once per percent). */
export interface ExportProgress {
  /** The file being written. */
  path: string;
  framesDone: number;
  frames: number;
  /** 0–100. */
  percent: number;
}

/** One change to the open sequence; a batch is one undo step. */
export type SequenceEdit =
  | { type: "updateInfo"; name: string; audio: string | null; durationMs: number; frameMs: number }
  | { type: "addRow"; row: Row; index?: number }
  | { type: "removeRow"; id: Uuid }
  | { type: "moveRow"; id: Uuid; index: number }
  | { type: "addLayer"; row: Uuid; index?: number }
  | { type: "removeLayer"; row: Uuid; layer: number }
  /** `layer` may be one past the top to start a new layer. */
  | { type: "addEffect"; row: Uuid; layer: number; effect: Effect }
  | { type: "updateEffect"; effect: Effect }
  | { type: "setEffectTiming"; id: Uuid; startMs: number; endMs: number }
  | { type: "setEffectParams"; id: Uuid; params: EffectParams }
  | { type: "moveEffect"; id: Uuid; row: Uuid; layer: number; startMs: number; endMs: number }
  | { type: "removeEffect"; id: Uuid }
  | { type: "addTimingTrack"; track: TimingTrack }
  | { type: "updateTimingTrack"; track: TimingTrack }
  | { type: "removeTimingTrack"; id: Uuid }
  | { type: "renameTimingTrack"; id: Uuid; name: string }
  /** Moves a timing track to `index` (clamped to the end). */
  | { type: "moveTimingTrack"; id: Uuid; index: number }
  /** Adds marks, each where it belongs in time; marks on a track never overlap. */
  | { type: "addMarks"; track: Uuid; marks: Mark[] }
  /** Replaces the mark at `index`: moves, resizes, or relabels it. */
  | { type: "setMark"; track: Uuid; index: number; mark: Mark }
  | { type: "removeMarks"; track: Uuid; indices: number[] }
  /** Splits the mark at `index` in two at `atMs`; the first part keeps the label. */
  | { type: "splitMark"; track: Uuid; index: number; atMs: number }
  /** Joins the mark at `index` with the next one (labels joined with a space). */
  | { type: "mergeMarks"; track: Uuid; index: number }
  /** A mark every `everyMs` from `fromMs` to `toMs`, replacing the marks there. */
  | { type: "generateMarks"; track: Uuid; everyMs: number; fromMs: number; toMs: number }
  /** Replaces `to`'s marks with every `every`th mark of `from` (1 = a copy). */
  | { type: "copyMarks"; from: Uuid; to: Uuid; every: number }
  /** One mark per line, spread over `fromMs..toMs` by letter count, replacing the marks there. */
  | { type: "spreadLyrics"; track: Uuid; lines: string[]; fromMs: number; toMs: number }
  /** Labels the marks at `indices` with `labels`, in order (as many of each). */
  | { type: "labelMarks"; track: Uuid; indices: number[]; labels: string[] }
  /** Breaks each phrase mark at `indices` of `track` into word marks on `words`, replacing the marks there. */
  | { type: "breakIntoWords"; track: Uuid; indices: number[]; words: Uuid };

/** What importing a timing file did: the edit's reply, the tracks added (as named in the
 * sequence), and what didn't come across. */
export interface TimingImported {
  result: SequenceEditResult;
  tracks: string[];
  notes: string[];
}

/** Where one controller's channels sit in an exported `.fseq` file. */
export interface ExportBlock {
  controller: Uuid;
  name: string;
  /** First channel, counting from 1. */
  start: number;
  count: number;
  fromSequenceChannels: boolean;
}

export interface ExportLayout {
  channels: number;
  blocks: ExportBlock[];
  notes: string[];
}

export interface ExportSummary {
  frames: number;
  frameMs: number;
  durationMs: number;
  channels: number;
  media: string | null;
  blocks: ExportBlock[];
  notes: string[];
}

/** A section of a song, as analysis found it (crates/pf-analysis Section). */
export interface AnalysisSection {
  startMs: number;
  endMs: number;
  /** Mean energy, 0–1 (relative to the song's loud parts). */
  energy: number;
  level: "low" | "medium" | "high";
  /** "Intro", "Verse", "Pre-Chorus", "Chorus", "Bridge", "Break", "Interlude", "Outro", "Part", or "Whole song". */
  label: string;
  /** Sections of the same material share a letter (A, B, A, C …). */
  group: string;
  /** How sure the grouping and label are, 0–1. */
  confidence: number;
}

/** A moment in a song to land on. */
export interface AnalysisEvent {
  timeMs: number;
  kind: "hit" | "drop" | "break" | "build";
  /** 0–1: 1 is the strongest of its kind in the song. */
  strength: number;
  /** How long it lasts (breaks and builds). */
  durationMs?: number;
}

/** What kind of moment (crates/pf-analysis MomentKind). */
export type MomentKind =
  | "impact"
  | "stop"
  | "restart"
  | "breakdown"
  | "build"
  | "fill"
  | "peak"
  | "hold"
  | "key_change"
  | "shout"
  | "drop"
  | "crash"
  | "section_change";

/** A treatment hint for a moment (crates/pf-analysis Suggest). */
export type MomentSuggest = "hit" | "blackout" | "burst" | "minimal" | "ramp" | "chase" | "full" | "sustain" | "color-shift" | "word-pop" | "flash" | "change";

/** A moment that makes a show dramatic, ranked (crates/pf-analysis Moment). */
export interface Moment {
  timeMs: number;
  /** Where it ends, for one that lasts (a breakdown, a build, a stop's gap). */
  endMs?: number;
  kind: MomentKind;
  /** How clearly it is this kind of moment, 0–1. */
  strength: number;
  /** How much it matters in this song, 0–1. */
  importance: number;
  /** A word, a section's name, a key change ("C→D"), or a kind of stop. */
  label?: string;
  suggest: MomentSuggest;
}

/** A notable drum hit. */
export interface DrumHit {
  timeMs: number;
  drum: "kick" | "snare" | "hat" | "crash";
  /** How hard, 0–1. */
  strength: number;
}

/** How many of each drum a bar has. */
export interface BarDrums {
  kick: number;
  snare: number;
  hat: number;
  crash: number;
}

/** One bar's energy, each 0–1 relative to the song. */
export interface BarEnergy {
  overall: number;
  low: number;
  mid: number;
  high: number;
}

/** What beat detection found in a song (times in ms). */
export interface Analysis {
  durationMs: number;
  tempoBpm: number | null;
  beats: number[];
  /** The first beat of each bar (the downbeats). */
  bars: number[];
  onsets: number[];
  /** The song's sections from its structure (empty when it had none to find). */
  sections: AnalysisSection[];
  /** Hits, drops, breaks, and builds, in time order. */
  events: AnalysisEvent[];
  /** One per entry in `bars`. */
  barEnergy: BarEnergy[];
  /** What makes the song dramatic, ranked by importance, in time order. */
  moments: Moment[];
  /** The notable drum hits (crashes, and kicks and snares harder than those around). */
  drums: DrumHit[];
  /** One per entry in `bars`. */
  barDrums: BarDrums[];
  /** How sure each part is, 0–1. */
  confidence: { tempo: number; downbeat: number; sections: number };
}

/** Every effect kind with the name people see, in menu order. */
export const EFFECT_KINDS: { kind: EffectKind; label: string }[] = [
  { kind: "on", label: "On" },
  { kind: "off", label: "Off" },
  { kind: "colorWash", label: "Color Wash" },
  { kind: "fade", label: "Fade" },
  { kind: "chase", label: "Chase" },
  { kind: "bars", label: "Bars" },
  { kind: "wave", label: "Wave" },
  { kind: "twinkle", label: "Twinkle" },
  { kind: "shimmer", label: "Shimmer" },
  { kind: "strobe", label: "Strobe" },
  { kind: "spiral", label: "Spiral" },
  { kind: "fire", label: "Fire" },
  { kind: "meteors", label: "Meteors" },
  { kind: "ripple", label: "Ripple" },
  { kind: "shape", label: "Shape" },
  { kind: "fan", label: "Fan" },
  { kind: "morph", label: "Morph" },
  { kind: "circles", label: "Circles" },
  { kind: "pinwheel", label: "Pinwheel" },
  { kind: "snowflakes", label: "Snowflakes" },
  { kind: "plasma", label: "Plasma" },
  { kind: "butterfly", label: "Butterfly" },
  { kind: "garlands", label: "Garlands" },
  { kind: "lines", label: "Lines" },
  { kind: "life", label: "Life" },
  { kind: "tendril", label: "Tendril" },
  { kind: "text", label: "Text" },
  { kind: "vuMeter", label: "VU Meter" },
  { kind: "impact", label: "Impact" },
  { kind: "wipe", label: "Wipe" },
  { kind: "lightning", label: "Lightning" },
  { kind: "pulse", label: "Pulse" },
  { kind: "sing", label: "Sing" },
  { kind: "colorShift", label: "Color Shift" },
  { kind: "dancer", label: "Dancer" },
];

/** A new effect of `kind` (engine-default settings, white) from `startMs` to `endMs`. */
export function newEffect(kind: EffectKind, startMs: number, endMs: number, colors: Rgb[] = ["#ffffff"]): Effect {
  return {
    id: crypto.randomUUID(),
    startMs,
    endMs,
    params: { kind } as EffectParams,
    palette: { colors },
    blend: "normal",
    fadeInMs: 0,
    fadeOutMs: 0,
  };
}

/** A new row with one empty layer. */
export function newRow(target: SequenceTarget): Row {
  return { id: crypto.randomUUID(), target, layers: [{ effects: [] }] };
}

/** The most rows a sequence can have (the engine's MAX_ROWS, crates/pf-sequence/src/limits.rs). */
export const MAX_ROWS = 10_000;

/**
 * A row for every group (that has members) and then every prop, each in the show's (layout) order:
 * what a new sequence starts with, as in xLights. At most `limit` rows (the first ones).
 */
export function rowsForShow(show: Pick<Show, "groups" | "props">, limit = MAX_ROWS): Row[] {
  const targets: SequenceTarget[] = [
    ...show.groups.filter((g) => g.members.length > 0).map((g) => ({ group: g.id })),
    ...show.props.map((p) => ({ prop: p.id })),
  ];
  return targets.slice(0, limit).map(newRow);
}
