import { wordPhonemes } from "../lib/submodels";
import type { Effect, EffectKind, Mark, Row, Sequence, TimingTrack } from "./sequence";
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

/**
 * A minute of sample show for the browser demo (`?demo`): the demo show's props timed to 120 BPM.
 * With `singing`, the window matrix's face sings a verse of lyrics first, and the arch's ends light
 * up on a row of their own.
 */
export function demoSequence(show: Show, durationMs = 60_000, { singing = false } = {}): Sequence {
  const beats: Mark[] = [];
  for (let t = 0, i = 0; t < durationMs; t += BEAT_MS, i++) beats.push({ startMs: t, endMs: Math.min(durationMs, t + BEAT_MS), label: String((i % 4) + 1) });
  const bars: Mark[] = beats.filter((_, i) => i % 4 === 0).map((b, i) => ({ startMs: b.startMs, endMs: Math.min(durationMs, b.startMs + 4 * BEAT_MS), label: String(i + 1) }));
  const bar = (n: number) => n * 4 * BEAT_MS;
  const [arch, tree, matrix, star] = show.props;
  const rows: Row[] = [];
  const add = (propId: string | undefined, layers: Effect[][]) => {
    if (propId) rows.push({ id: crypto.randomUUID(), target: { prop: propId }, layers: layers.map((effects) => ({ effects })) });
  };
  const addOnSubmodel = (propId: string | undefined, name: string, layers: Effect[][]) => {
    const region = show.props.find((p) => p.id === propId)?.regions.find((r) => r.name === name);
    if (propId && region) rows.push({ id: crypto.randomUUID(), target: { region: { prop: propId, region: region.id } }, layers: layers.map((effects) => ({ effects })) });
  };
  const lyrics = singing ? lyricTracks(durationMs) : null;
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
  if (lyrics) addOnSubmodel(arch?.id, "Ends", [[effect("on", bar(4), bar(8), [gold], { startLevel: 1, endLevel: 0.2 })]]);
  add(matrix?.id, [
    [
      ...(lyrics
        ? [effect("faces", 0, bar(8), [red, blue, green], { face: "Singer", timingTrack: lyrics.phonemes.id, eyes: "auto", colors: "face", outline: true })]
        : [effect("bars", 0, bar(4), [red, green], { axis: "horizontal" }), effect("fire", bar(4), bar(8), [])]),
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
    schemaVersion: 7,
    name: "Christmas Medley 2017",
    audio: DEMO_MUSIC,
    durationMs,
    frameMs: 25,
    timingTracks: [
      { id: crypto.randomUUID(), name: "Beats", kind: "beats", marks: beats },
      { id: crypto.randomUUID(), name: "Bars", kind: "bars", marks: bars },
      ...(lyrics ? [lyrics.phrases, lyrics.words, lyrics.phonemes] : []),
    ],
    rows,
  };
}

const SONG = [
  { at: 1000, words: ["Jingle", "bells,", "jingle", "bells,"] },
  { at: 4000, words: ["jingle", "all", "the", "way"] },
  { at: 8000, words: ["Oh", "what", "fun", "it", "is", "to", "ride"] },
  { at: 12_000, words: ["in", "a", "one", "horse", "open", "sleigh"] },
];

/** A verse of lyrics for the singing face: phrases, words, and phonemes (from the words' letters). */
function lyricTracks(durationMs: number): { phrases: TimingTrack; words: TimingTrack; phonemes: TimingTrack } {
  const WORD_MS = 450;
  const phrases: Mark[] = [];
  const words: Mark[] = [];
  const phonemes: Mark[] = [];
  for (const line of SONG) {
    if (line.at + line.words.length * WORD_MS > durationMs) break;
    phrases.push({ startMs: line.at, endMs: line.at + line.words.length * WORD_MS, label: line.words.join(" ") });
    line.words.forEach((word, i) => {
      const start = line.at + i * WORD_MS;
      words.push({ startMs: start, endMs: start + WORD_MS - 50, label: word });
      const shapes = wordPhonemes(word);
      const each = Math.floor((WORD_MS - 50) / Math.max(1, shapes.length));
      shapes.forEach((shape, k) => phonemes.push({ startMs: start + k * each, endMs: start + (k + 1) * each, label: shape === "ETC" ? "etc" : shape === "REST" ? "rest" : shape }));
    });
  }
  const track = (name: string, kind: TimingTrack["kind"], marks: Mark[]): TimingTrack => ({ id: crypto.randomUUID(), name, kind, marks });
  return { phrases: track("Lyrics", "lyrics", phrases), words: track("Lyrics (words)", "words", words), phonemes: track("Lyrics (phonemes)", "phonemes", phonemes) };
}
