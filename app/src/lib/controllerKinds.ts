// Controllers PixelFlow knows the shape of: how many ports they have and how many pixels each
// port drives. Matches what the engine's device import knows (crates/pf-devices, falcon.rs), which
// follows xLights' Falcon definitions (resources/controllers/falcon.xcontroller, Falcon.cpp).

import type { Controller, Device } from "../api/types";
import { newController } from "./shows";

export interface ControllerKind {
  id: string;
  label: string;
  adapter: Controller["adapter"];
  /** Ports on the board, when it has a fixed number. */
  ports: number | null;
  /** Most pixels one port drives, when known. */
  maxPixels: number | null;
  /** A DMX node or interface (each port one DMX universe), and how it's sent. */
  dmx?: "artNet" | "sacn" | "usb";
}

/**
 * About how many pixels a Falcon V4/V5 port refreshes in time at 40 frames a second (xLights'
 * `FPS40Pixels` for these boards). The board's own limit (1,024, or 704 with more than 32 ports
 * in use) is what it reaches at about 20 fps.
 */
export const FALCON_PIXELS_AT_40FPS = 704;

/** A Falcon V4/V5 board in the mode with this many ports: 1,024 pixels a port up to 32 ports, 704 with more. */
const falcon = (id: string, label: string, ports: number): ControllerKind => ({ id, label, adapter: "falcon", ports, maxPixels: ports > 32 ? 704 : 1024 });

export const CONTROLLER_KINDS: ControllerKind[] = [
  { id: "other", label: "Other / not listed", adapter: "generic", ports: null, maxPixels: null },
  falcon("f16v5", "Falcon F16V5", 16),
  falcon("f32v5", "Falcon F32V5", 32),
  falcon("f48v5", "Falcon F48V5", 48),
  falcon("f16v4", "Falcon F16V4", 16),
  falcon("f48v4", "Falcon F48V4", 48),
  { id: "fpp", label: "FPP (Pi or BeagleBone cape)", adapter: "fpp", ports: null, maxPixels: null },
  { id: "wled", label: "WLED", adapter: "wled", ports: 1, maxPixels: null },
  { id: "dmx-artnet", label: "DMX node (Art-Net)", adapter: "generic", ports: 1, maxPixels: null, dmx: "artNet" },
  { id: "dmx-sacn", label: "DMX node (sACN)", adapter: "generic", ports: 1, maxPixels: null, dmx: "sacn" },
  { id: "dmx-usb", label: "USB DMX interface", adapter: "generic", ports: 1, maxPixels: null, dmx: "usb" },
];

export function kindById(id: string): ControllerKind {
  return CONTROLLER_KINDS.find((k) => k.id === id) ?? CONTROLLER_KINDS[0];
}

/** A controller type worth offering for one being added, with its short name and why. */
export interface KindGuess {
  id: string;
  short: string;
  why: string;
}

/**
 * What a controller being added by hand most likely is: what a scan found at its address, else an
 * FPP when its name or address says so ("Show FPP", "fpp.local"). Null with nothing to go on. Only
 * ever offered: the form's type changes when the user takes it.
 */
export function guessKind(name: string, address: string, devices: Pick<Device, "address" | "kind" | "model">[] = []): KindGuess | null {
  const found = devices.find((d) => d.address === address.trim());
  if (found?.kind === "fpp") return { id: "fpp", short: "FPP", why: "A scan found an FPP at this address." };
  if (found?.kind === "wled") return { id: "wled", short: "WLED", why: "A scan found a WLED at this address." };
  const falcon = found?.kind === "falcon" ? CONTROLLER_KINDS.find((k) => k.adapter === "falcon" && k.label.toLowerCase().endsWith(` ${found.model.toLowerCase()}`)) : undefined;
  if (found && falcon) return { id: falcon.id, short: falcon.label, why: `A scan found a ${falcon.label} at this address.` };
  if (!found && (/\bfpp\b/i.test(name) || /\bfpp\b/i.test(address))) return { id: "fpp", short: "FPP", why: "Is it an FPP?" };
  return null;
}

/** Most pixels a port drives on this kind of board with `ports` ports in use. */
export function kindPixelLimit(kind: ControllerKind, ports: number): number | null {
  return kind.adapter === "falcon" ? (ports > 32 ? 704 : 1024) : kind.maxPixels;
}

/** A new controller of the kind with `portCount` ports (the form starts from the board's own
 * count, but an expansion board can add more), each with the board's limit when known. */
export function controllerOfKind(id: string, name: string, address: string, protocol: "ddp" | "sacn", portCount: number): Controller {
  const kind = kindById(id);
  const controller = newController(name, address, protocol, portCount);
  if (kind.dmx) {
    // Each port one whole universe, sent in full.
    controller.kind = "dmx";
    controller.protocol =
      kind.dmx === "usb"
        ? { type: "dmxUsb" }
        : kind.dmx === "artNet"
          ? { type: "artNet", startUniverse: null, universeSize: 512, allowPixelStraddle: false }
          : { type: "sacn", startUniverse: null, universeSize: 512, allowPixelStraddle: false, multicast: false };
    if (kind.dmx === "usb") controller.ports = controller.ports.slice(0, 1);
    return controller;
  }
  controller.adapter = kind.adapter;
  const limit = kindPixelLimit(kind, portCount);
  for (const port of controller.ports) port.maxPixels = limit;
  return controller;
}
