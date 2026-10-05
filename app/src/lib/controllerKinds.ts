// Controllers PixelFlow knows the shape of: how many ports they have and how many pixels each
// port drives. Matches what the engine's device import knows (crates/pf-devices, falcon.rs).

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

const falcon = (id: string, label: string, ports: number): ControllerKind => ({ id, label, adapter: "falcon", ports, maxPixels: 1024 });

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

/** A new controller of the kind: its own port count and limit when known, else `portCount`. */
export function controllerOfKind(id: string, name: string, address: string, protocol: "ddp" | "sacn", portCount: number): Controller {
  const kind = kindById(id);
  const controller = newController(name, address, protocol, kind.ports ?? portCount);
  controller.adapter = kind.adapter;
  for (const port of controller.ports) port.maxPixels = kind.maxPixels;
  return controller;
}
