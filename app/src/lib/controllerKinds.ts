// Controllers PixelFlow knows the shape of: how many ports they have and how many pixels each
// port drives. Matches what the engine's device import knows (crates/pf-devices, falcon.rs), which
// follows xLights' Falcon definitions (resources/controllers/falcon.xcontroller, Falcon.cpp).

import type { Controller } from "../api/types";
import { newController } from "./shows";

export interface ControllerKind {
  id: string;
  label: string;
  adapter: Controller["adapter"];
  /** Ports on the board, when it has a fixed number. */
  ports: number | null;
  /** Most pixels one port drives, when known. */
  maxPixels: number | null;
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
];

export function kindById(id: string): ControllerKind {
  return CONTROLLER_KINDS.find((k) => k.id === id) ?? CONTROLLER_KINDS[0];
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
  controller.adapter = kind.adapter;
  const limit = kindPixelLimit(kind, portCount);
  for (const port of controller.ports) port.maxPixels = limit;
  return controller;
}
