import type { Effect, EffectKind, Mark, Row, Sequence } from "./sequence";
import type { Show } from "./types";

export const DEMO_SEQUENCE_PATH = "/Shows/Christmas Medley 2017.pfseq.json";
export const DEMO_MUSIC = "/Shows/Christmas Medley 2017.mp3";

const BEAT_MS = 500;

function effect(kind: EffectKind, startMs: number, endMs: number, colors: string[], params: Record<string, unknown> = {}, extra: Partial<Effect> = {}): Effect {
  return {
    id: crypto.randomUUID(),
    startMs,
    endMs,
    params: { kind, ...params } as Effect["params"],
    palette: { colors },
    blend: "normal",
    fadeInMs: 0,
    fadeOutMs: 0,
    ...extra,
  };
}

/** A minute of sample show for the browser demo (`?demo`): the demo show's props timed to 120 BPM. */
export function demoSequence(show: Show, durationMs = 60_000): Sequence {
  const beats: Mark[] = [];
  for (let t = 0, i = 0; t < durationMs; t += BEAT_MS, i++) beats.push({ startMs: t, endMs: Math.min(durationMs, t + BEAT_MS), label: String((i % 4) + 1) });
  const bars: Mark[] = beats.filter((_, i) => i % 4 === 0).map((b, i) => ({ startMs: b.startMs, endMs: Math.min(durationMs, b.startMs + 4 * BEAT_MS), label: String(i + 1) }));
  const bar = (n: number) => n * 4 * BEAT_MS;
  const [arch, tree, matrix, star] = show.props;
  const rows: Row[] = [];
  const add = (propId: string | undefined, layers: Effect[][]) => {
    if (propId) rows.push({ id: crypto.randomUUID(), target: { prop: propId }, layers: layers.map((effects) => ({ effects })) });
  };
  const red = "#ff1a1a";
  const green = "#16c60c";
  const white = "#ffffff";
  const gold = "#ffb300";
  const blue = "#2a6bff";
  add(tree?.id, [
    [
      effect("colorWash", 0, bar(2), [red, green, white], { cycles: 2 }, { fadeInMs: 1500 }),
      effect("spiral", bar(2), bar(6), [red, white], { count: 4, speed: 0.6 }),
      effect("twinkle", bar(6), bar(8), [white, gold], { density: 0.4 }),
      effect("bars", bar(8), bar(12), [red, green], { count: 6, speed: 1 }),
      effect("meteors", bar(12), bar(16), [white, blue], { count: 8 }),
      effect("ripple", bar(16), bar(20), [gold, red, green]),
      effect("spiral", bar(20), bar(26), [green, white, red], { count: 6, speed: 1 }),
      effect("fire", bar(26), bar(30), []),
    ],
    [effect("strobe", bar(7), bar(8), [white], { rate: 12 }, { blend: "add" }), effect("shimmer", bar(15), bar(16), [white], {}, { blend: "add" })],
  ]);
  add(arch?.id, [
    Array.from({ length: 14 }, (_, i) =>
      effect(i % 2 ? "chase" : "wave", bar(i * 2), bar(i * 2 + 2), i % 2 ? [red, white, green] : [blue, white], i % 2 ? { bands: 3, speed: 1.5 } : {}),
    ),
  ]);
  add(matrix?.id, [
    [
      effect("bars", 0, bar(4), [red, green], { axis: "horizontal" }),
      effect("fire", bar(4), bar(8), []),
      effect("wave", bar(8), bar(12), [gold, red], { cycles: 2 }),
      effect("ripple", bar(12), bar(18), [blue, white]),
      effect("colorWash", bar(18), bar(24), [red, gold, green], { gradient: "horizontal", cycles: 3 }),
      effect("meteors", bar(24), bar(30), [white], { direction: "down", count: 10 }),
    ],
  ]);
  add(star?.id, [
    beats
      .filter((_, i) => i % 2 === 0 && i < 112)
      .map((b, i) => effect("on", b.startMs, b.startMs + BEAT_MS, [i % 2 ? gold : white], { startLevel: 1, endLevel: 0.1 })),
  ]);
  return {
    schemaVersion: 2,
    name: "Christmas Medley 2017",
    audio: DEMO_MUSIC,
    durationMs,
    frameMs: 25,
    timingTracks: [
      { id: crypto.randomUUID(), name: "Beats", kind: "beats", marks: beats },
      { id: crypto.randomUUID(), name: "Bars", kind: "bars", marks: bars },
    ],
    rows,
  };
}
