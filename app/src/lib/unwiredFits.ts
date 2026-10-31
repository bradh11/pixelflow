// When a found controller's string matches no prop by name: the props already drawn in the show
// that could be it. Someone who draws "Mega Tree 1" (800 pixels) and then adds a Falcon with an
// 800-pixel string most likely means that tree, not a second one.

import type { DeviceConfig, Prop, PropMatch, Show } from "../api/types";
import { mappingProblem, stringKey } from "./deviceSetup";
import { nodeCount } from "./shows";

/** The show's unwired props that fit one string, likeliest first. */
export interface UnwiredFit {
  props: string[];
  /** The kind of prop the string's name says it is ("tree"), when it says one. */
  kind: string | null;
}

/** Words in a string's or prop's name that say what kind of prop it is, by the shape it would have. */
const KIND_WORDS: { kind: string; shapes: string[]; words: RegExp }[] = [
  { kind: "tree", shapes: ["tree"], words: /\b(mega ?)?trees?\b|megatrees?/ },
  { kind: "arch", shapes: ["arch"], words: /\barch(es)?\b/ },
  { kind: "matrix", shapes: ["matrix"], words: /\bmatrix|matrices|\bpanels?\b/ },
  { kind: "star", shapes: ["star"], words: /\bstars?\b/ },
  { kind: "wreath", shapes: ["wreath"], words: /\bwreaths?\b/ },
  { kind: "circle", shapes: ["circle"], words: /\bcircles?\b|\brings?\b/ },
  { kind: "candy cane", shapes: ["candyCanes"], words: /\b(candy ?)?canes?\b/ },
  { kind: "icicles", shapes: ["icicles"], words: /\bicicles?\b/ },
  { kind: "window frame", shapes: ["windowFrame"], words: /\bwindows?\b/ },
  { kind: "spinner", shapes: ["spinner"], words: /\bspinners?\b/ },
  { kind: "sphere", shapes: ["sphere"], words: /\bspheres?\b/ },
  { kind: "cube", shapes: ["cube"], words: /\bcubes?\b/ },
  { kind: "line", shapes: ["line", "polyLine"], words: /\b(lines?|roof\w*|gutters?|eaves?|strips?|outlines?)\b/ },
];

/** The kinds a name says, as indexes into KIND_WORDS ("Window Matrix" says two). */
function kindsNamed(name: string): number[] {
  const text = name.toLowerCase().replace(/[_-]+/g, " ");
  return KIND_WORDS.flatMap((k, i) => (k.words.test(text) ? [i] : []));
}

/** The kinds a prop is: its shape's, or for a shape that doesn't say (one imported as points), its name's. */
function kindsOf(prop: Prop): number[] {
  if (prop.shape.source === "generator") {
    const type = prop.shape.type;
    return KIND_WORDS.flatMap((k, i) => (k.shapes.includes(type) ? [i] : []));
  }
  return kindsNamed(prop.name);
}

const words = (name: string) => new Set(name.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean));

/** The props on any controller's port in the show. */
function wiredProps(show: Show): Set<string> {
  return new Set(show.controllers.flatMap((c) => c.ports.flatMap((p) => p.slots.map((s) => s.prop))));
}

/**
 * For each of the device's strings that has no suggestion already (by port or by name), the
 * show's props that fit it: not wired to any controller, not a DMX fixture, not suggested for
 * another string, with the string's pixel count and channels per pixel, and of the kind the
 * string's name says where it says one. Likeliest first: the most words in common with the
 * string's name, then by name. Strings nothing fits are left out.
 */
export function unwiredFits(show: Show, config: DeviceConfig, suggested: Record<string, PropMatch>): Record<string, UnwiredFit> {
  const wired = wiredProps(show);
  const taken = new Set(Object.values(suggested).map((m) => m.prop));
  const free = show.props.filter((p) => !wired.has(p.id) && !taken.has(p.id) && !p.dmx);
  const fits: Record<string, UnwiredFit> = {};
  for (const port of config.ports) {
    port.strings.forEach((string, i) => {
      const key = stringKey(port.number, i);
      if (suggested[key]) return;
      const name = string.name ?? "";
      const named = kindsNamed(name);
      const said = words(name);
      const props = free
        .filter((p) => nodeCount(p.shape) === string.pixels && !mappingProblem(p, string.colorOrder))
        .filter((p) => named.length === 0 || kindsOf(p).some((k) => named.includes(k)))
        .map((p) => ({ p, shared: [...words(p.name)].filter((w) => said.has(w)).length }))
        .sort((a, b) => b.shared - a.shared || a.p.name.localeCompare(b.p.name, undefined, { numeric: true }))
        .map(({ p }) => p.id);
      if (props.length === 0) return;
      const kind = named.length === 1 ? KIND_WORDS[named[0]].kind : null;
      fits[key] = { props, kind };
    });
  }
  return fits;
}

/**
 * The fits clear enough to start on: a string's only fit, when it fits no other string. Two
 * strings that could both be one prop start on neither, and no prop starts on two strings.
 */
export function clearFits(fits: Record<string, UnwiredFit>): Record<string, string> {
  const strings = new Map<string, number>();
  for (const fit of Object.values(fits)) for (const id of fit.props) strings.set(id, (strings.get(id) ?? 0) + 1);
  return Object.fromEntries(
    Object.entries(fits)
      .filter(([, fit]) => fit.props.length === 1 && strings.get(fit.props[0]) === 1)
      .map(([key, fit]) => [key, fit.props[0]]),
  );
}
