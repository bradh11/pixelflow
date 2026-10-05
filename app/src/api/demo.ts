import type { DeviceDetails, PortSlot, Prop, Show, SilentPeer, StringConfig } from "./types";
import { emptyShow } from "./memory";
import { newController, newProp } from "../lib/shows";

/** A small sample show for browser-only UI development (`?demo`). */
export function demoShow(): Show {
  const show = emptyShow("Demo House");
  const arch = { ...newProp("arch", show), name: "Garage Arch" };
  const tree = { ...newProp("tree", show), name: "Mega Tree" };
  const matrix = { ...newProp("matrix", show), name: "Window Matrix", colorOrder: "GRB" as const };
  const star = { ...newProp("star", show), name: "Porch Star" };
  show.props = [arch, tree, matrix, star];
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
  const wled = newController("Porch WLED", "192.168.1.60", "ddp", 1);
  show.controllers = [fpp, wled];
  return show;
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
          destinations: [{ address: "192.0.2.20", description: "Falcon_F16V5_B9F5", protocol: "DDP", channels: 6147 }],
          notes: ["This FPP has no pixel outputs of its own; it sends to the controllers listed below. Import those instead."],
        },
        plan: { controller: fppController, props: [], notes: ["This FPP has no pixel outputs of its own; it sends to the controllers listed below. Import those instead."], alreadyInShow: false, canImport: false },
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
