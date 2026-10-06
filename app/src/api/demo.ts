import type { DeviceDetails, FppSequence, NodeRange, Phoneme, PlayerStatus, PortSlot, Prop, Region, Show, SilentPeer, StringConfig } from "./types";
import { type MemoryBackend, emptyShow } from "./memory";
import { newController, newProp } from "../lib/shows";

/** A small sample show for browser-only UI development (`?demo`). */
export function demoShow(): Show {
  const show = emptyShow("Demo House");
  const at = (prop: Prop, x: number, y: number): Prop => ({
    ...prop,
    transform: { ...prop.transform, position: { x, y, z: 0 } },
  });
  const arch = at({ ...newProp("arch", show), name: "Garage Arch" }, -6, 0);
  const tree = at({ ...newProp("tree", show), name: "Mega Tree" }, 8, 0);
  const matrix = at({ ...newProp("matrix", show), name: "Window Matrix", colorOrder: "GRB" as const }, 0.5, 3.8);
  const star = at({ ...newProp("star", show), name: "Porch Star" }, -3, 8.2);
  arch.regions = [
    { id: crypto.randomUUID(), name: "Left half", kind: "nodes", lines: [[{ first: 0, last: 24 }]], layout: "horizontal", buffer: "default" },
    { id: crypto.randomUUID(), name: "Right half", kind: "nodes", lines: [[{ first: 49, last: 25 }]], layout: "horizontal", buffer: "default" },
    {
      id: crypto.randomUUID(),
      name: "Ends",
      kind: "nodes",
      lines: [[{ first: 0, last: 5 }], [{ first: 49, last: 44 }]],
      layout: "horizontal",
      buffer: "stackedStrands",
    },
  ];
  matrix.regions = [{ id: crypto.randomUUID(), name: "Top half", kind: "subBuffer", x1: 0, y1: 50, x2: 100, y2: 100 }, demoFace()];
  show.props = [arch, tree, matrix, star];
  show.background = { path: DEMO_PHOTO, x: -12, y: 12, width: 24, opacity: 0.8 };
  const fpp = newController("Main FPP", "192.168.1.50", "sacn", 4);
  fpp.ports[0].slots = [arch, matrix].map((p) => ({
    prop: p.id,
    segment: null,
    nullPixels: 0,
    reverse: false,
    brightness: null,
    gamma: null,
    smartReceiver: null,
  }));
  fpp.ports[1].slots = [
    { prop: tree.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null },
  ];
  fpp.sequenceChannels = { start: 1, count: 4800 };
  const wled = newController("Porch WLED", "192.168.1.60", "ddp", 1);
  show.controllers = [fpp, wled];
  return show;
}

/**
 * A singing face drawn on the demo's 32 × 16 window matrix (wired in rows from the bottom left,
 * back and forth): eyes, an outline, and a mouth for every sound.
 */
function demoFace(): Region {
  const node = (col: number, row: number) => row * 32 + (row % 2 === 0 ? col : 31 - col);
  /** Pixels in columns `from`–`to` on each row in `rows`, as one range per row. */
  const block = (from: number, to: number, rows: number[]): NodeRange[] =>
    rows.map((row) => {
      const [a, b] = [node(from, row), node(to, row)];
      return { start: Math.min(a, b), end: Math.max(a, b) + 1 };
    });
  const span = (lo: number, hi: number) => Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
  const mouth = (from: number, to: number, lo: number, hi: number) => block(from, to, span(lo, hi));
  const mouths: Partial<Record<Phoneme, NodeRange[]>> = {
    AI: mouth(11, 20, 2, 6),
    E: mouth(9, 22, 3, 5),
    ETC: mouth(12, 19, 3, 5),
    FV: mouth(12, 19, 4, 5),
    L: mouth(12, 19, 3, 6),
    MBP: mouth(10, 21, 4, 4),
    O: mouth(13, 18, 1, 7),
    REST: mouth(12, 19, 4, 4),
    U: mouth(14, 17, 3, 5),
    WQ: mouth(14, 17, 2, 6),
  };
  return {
    id: crypto.randomUUID(),
    name: "Singer",
    kind: "face",
    mouths,
    eyesOpen: [...block(8, 10, [10, 11, 12]), ...block(21, 23, [10, 11, 12])],
    eyesClosed: [...block(8, 10, [11]), ...block(21, 23, [11])],
    outline: [...block(0, 31, [0, 15]), ...span(1, 14).flatMap((row) => block(0, 0, [row]).concat(block(31, 31, [row])))],
    colors: { mouths: Object.fromEntries(Object.keys(mouths).map((p) => [p, "#ff2d2d"])), eyesOpen: "#3cb4ff", eyesClosed: "#3cb4ff", outline: "#1fbf4f" },
  };
}

/** Where the demo show's house photo "is". */
export const DEMO_PHOTO = "/Photos/Demo House.svg";

/**
 * The demo show as if it had been opened from a folder that moved (`?demo&missing`): two songs
 * and the photo aren't where they were. A search finds the medley's music and the photo; the
 * other sequence file has to be located.
 */
export function demoMissingFiles(backend: MemoryBackend) {
  const medley = "/Shows/Christmas Medley 2017.mp3";
  backend.show.sequences = [
    { id: crypto.randomUUID(), name: "Christmas Medley 2017", path: "/Shows/Christmas Medley 2017.fseq", audio: medley, offsetMs: 0 },
    { id: crypto.randomUUID(), name: "Wizards in Winter", path: "/Shows/Wizards in Winter.fseq", audio: "/Shows/Wizards in Winter.mp3", offsetMs: 0 },
  ];
  backend.path = "/Shows/Demo House.pixelflow.json";
  backend.files.set(backend.path, structuredClone(backend.show));
  backend.missingPaths = new Set([medley, "/Shows/Wizards in Winter.fseq", DEMO_PHOTO]);
  backend.findable.set(medley, "/Shows/Music/Christmas Medley 2017.mp3");
  backend.findable.set(DEMO_PHOTO, "/Shows/Photos/Demo House.svg");
  backend.images.set("/Shows/Photos/Demo House.svg", demoHousePhoto());
  backend.nextLocatePath = "/Volumes/USB/Wizards in Winter.fseq";
}

/**
 * A drawing of a house at night (1600 × 1000), standing in for a photo in the demo. Layout
 * units map to it as x: -12…12 and y: 12…-3, so the ground (y = 0) is 800 px down.
 */
export function demoHousePhoto(): Uint8Array {
  const stars = Array.from({ length: 40 }, (_, i) => {
    const x = (i * 397) % 1600;
    const y = (i * 151) % 320;
    return `<circle cx="${x}" cy="${y}" r="${1 + (i % 3) * 0.6}" fill="#fff" opacity="${0.3 + (i % 4) * 0.15}"/>`;
  }).join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1000" viewBox="0 0 1600 1000">
<defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0b1026"/><stop offset="1" stop-color="#26304f"/></linearGradient></defs>
<rect width="1600" height="1000" fill="url(#sky)"/>${stars}
<circle cx="1380" cy="140" r="48" fill="#f3efd8" opacity="0.85"/>
<rect y="790" width="1600" height="210" fill="#1d2b22"/>
<rect x="133" y="400" width="934" height="400" fill="#5b4a3f"/>
<polygon points="67,410 600,100 1133,410" fill="#3a2f2a"/>
<rect x="250" y="590" width="300" height="210" fill="#8c8478"/>
<g stroke="#6d665c" stroke-width="4">${[630, 670, 710, 750].map((y) => `<line x1="250" y1="${y}" x2="550" y2="${y}"/>`).join("")}</g>
<rect x="700" y="480" width="267" height="133" fill="#e9c46a" opacity="0.75"/>
<path d="M833 480v133M700 547h267" stroke="#5b4a3f" stroke-width="8"/>
<rect x="600" y="640" width="70" height="160" fill="#3b2b22"/>
<rect x="540" y="200" width="120" height="90" fill="#e9c46a" opacity="0.6"/>
<rect x="1313" y="785" width="40" height="25" fill="#3b2b22"/>
</svg>`;
  return new TextEncoder().encode(svg);
}

function slotFor(prop: Prop, overrides: Partial<PortSlot> = {}): PortSlot {
  return { prop: prop.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null, ...overrides };
}

function stringConfig(prop: Prop, pixels: number, overrides: Partial<StringConfig> = {}): StringConfig {
  return { name: prop.name, pixels, colorOrder: prop.colorOrder, nullPixels: 0, reverse: false, brightness: 100, gamma: 1, smartReceiver: null, ...overrides };
}

/** A small sample network for browser development and tests: an FPP player that sends to a
 * Falcon, the Falcon itself, a WLED, and one Falcon the FPP lists that isn't answering. */
export function demoDevices(): { details: DeviceDetails[]; silent: SilentPeer[] } {
  const empty = emptyShow("Devices");
  const tree = { ...newProp("tree", empty), name: "Falcon Mega Tree" };
  const arch = { ...newProp("arch", empty), name: "Falcon Arch" };
  const falcon = newController("Falcon_F16V5_B9F5", "192.0.2.20", "ddp", 2);
  falcon.adapter = "falcon";
  // Like the engine's import of an F16V5 in its 16-port board mode: 1,024 pixels a port.
  for (const port of falcon.ports) port.maxPixels = 1024;
  falcon.ports[0].slots = [slotFor(tree)];
  falcon.ports[1].slots = [slotFor(arch)];
  const strip = { ...newProp("line", empty), name: "Porch Strip" };
  const wled = newController("Porch WLED", "192.0.2.40", "ddp", 1);
  wled.adapter = "wled";
  wled.ports[0].slots = [slotFor(strip)];
  const fppController = newController("FPP", "192.0.2.10", "ddp", 0);
  fppController.adapter = "fpp";
  return {
    details: [
      {
        device: { address: "192.0.2.10", kind: "fpp", name: "FPP", model: "Pi 3 Model B+", firmware: "FPP 9.3", mode: "player", foundBy: ["webSweep"] },
        config: {
          input: { type: "ddp" },
          ports: [],
          destinations: [
            { address: "192.0.2.20", description: "Falcon_F16V5_B9F5", protocol: "DDP", channels: 6147, startChannel: 1, startUniverse: null, universeSize: null, ddpRaw: false, unevenUniverses: false },
          ],
          notes: ["This FPP has no pixel outputs of its own; it sends to the controllers listed above. Add each one with its Add to show button."],
        },
        plan: { controller: fppController, props: [], notes: ["This FPP has no pixel outputs of its own; it sends to the controllers listed above. Add each one with its Add to show button."], alreadyInShow: false, canImport: false },
      },
      {
        device: { address: "192.0.2.20", kind: "falcon", name: "Falcon_F16V5_B9F5", model: "F16v5", firmware: "F16V5 v2.00", mode: null, foundBy: ["fppPeer"] },
        config: {
          input: { type: "ddp" },
          ports: [
            { number: 1, strings: [stringConfig(tree, 800, { colorOrder: "GRB" })], maxPixels: 1024 },
            { number: 2, strings: [stringConfig(arch, 50, { reverse: true, nullPixels: 1 })], maxPixels: 1024 },
          ],
          destinations: [],
          notes: [],
        },
        plan: {
          controller: falcon,
          props: [tree, arch],
          notes: [
            'The controller skips its own null pixels (Port 2 "Falcon Arch": 1), so PixelFlow won\'t send data for them.',
            'The controller applies its own settings (Port 2 "Falcon Arch": reversed), so PixelFlow sends unadjusted data.',
          ],
          alreadyInShow: false,
          canImport: true,
        },
      },
      {
        device: { address: "192.0.2.40", kind: "wled", name: "Porch WLED", model: "WLED (esp32)", firmware: "WLED 0.15.0", mode: null, foundBy: ["mdns"] },
        config: { input: { type: "ddp" }, ports: [{ number: 1, strings: [stringConfig(strip, 50, { colorOrder: "GRB" })], maxPixels: null }], destinations: [], notes: [] },
        plan: { controller: wled, props: [strip], notes: [], alreadyInShow: false, canImport: true },
      },
    ],
    silent: [{ address: "192.0.2.21", description: "Falcon_F16V5_Garage", listedBy: "FPP" }],
  };
}

/** The demo FPP is playing its one sequence, and can't reach one of its controllers. */
export function demoPlayers(): Record<string, { status: PlayerStatus; sequences: FppSequence[] }> {
  return {
    "192.0.2.10": {
      status: {
        state: "playing",
        playlist: "Christmas Medley 2017.fseq",
        sequence: "Christmas Medley 2017.fseq",
        secondsElapsed: 109,
        secondsRemaining: 456,
        nextPlaylist: "Christmas Medley 2017.fseq",
        nextStart: "Mon Oct  5 @ 06:48 PM - (Everyday)",
        warnings: ["Cannot Ping DDP Channel Data Target 192.0.2.21 Falcon_F16V5_Garage"],
      },
      sequences: [{ name: "Christmas Medley 2017", frames: 11332, stepMs: 50, channels: 6148 }],
    },
  };
}
