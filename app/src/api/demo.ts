import type { DeviceDetails, FppSequence, PlayerStatus, PortSlot, Prop, Show, SilentPeer, StringConfig } from "./types";
import { emptyShow } from "./memory";
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

/** Where the demo show's house photo "is". */
export const DEMO_PHOTO = "/Photos/Demo House.svg";

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
  // Like the engine's import: a V5 Falcon's ports drive up to 1,024 pixels each.
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
          notes: ["This FPP has no pixel outputs of its own; it sends to the controllers listed below. Add them to your show from here."],
        },
        plan: { controller: fppController, props: [], notes: ["This FPP has no pixel outputs of its own; it sends to the controllers listed below. Add them to your show from here."], alreadyInShow: false, canImport: false },
      },
      {
        device: { address: "192.0.2.20", kind: "falcon", name: "Falcon_F16V5_B9F5", model: "F16v5", firmware: "F16V5 v2.00", mode: null, foundBy: ["fppPeer"] },
        config: {
          input: { type: "ddp" },
          ports: [
            { number: 1, strings: [stringConfig(tree, 800, { colorOrder: "GRB" })] },
            { number: 2, strings: [stringConfig(arch, 50, { reverse: true, nullPixels: 1 })] },
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
        config: { input: { type: "ddp" }, ports: [{ number: 1, strings: [stringConfig(strip, 50, { colorOrder: "GRB" })] }], destinations: [], notes: [] },
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
