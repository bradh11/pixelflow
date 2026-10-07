// A controller's setup side by side with the show's, for the in-memory backend: the same rows
// crates/pf-devices/src/setup.rs gives the desktop app ("Compare with this device" and "Send
// setup to this device…"), and taking a device's differences into the show.

import type { Change, ChangeKind, ColorOrder, Controller, DeviceConfig, DeviceKind, PortSlot, Prop, Show, UseProps } from "../api/types";
import { thousands } from "./format";
import { nodeCount } from "./shows";

export interface SetupString {
  name: string;
  pixels: number;
  colorOrder: ColorOrder | null;
  start: number | null;
  channelsPerPixel: number;
  /** Show side only: which of the port's slots it carries. */
  slots: number[];
}

export interface SetupPort {
  number: number;
  strings: SetupString[];
}

export type SetupInput =
  | { type: "ddp" }
  | { type: "sacn"; startUniverse: number | null; universeSize: number }
  | { type: "other"; description: string };

export interface Setup {
  input: SetupInput;
  ports: SetupPort[];
  notes: string[];
}

export type Direction = "toDevice" | "intoShow";

const RECEIVER_NOTE = "Strings on smart receivers aren't compared yet; check them on the controller itself.";

export const stringKey = (port: number, index: number) => `port${port}/string${index + 1}`;

export const oneStringPerPort = (kind: DeviceKind) => kind === "wled";

const cpp = (order: ColorOrder) => (order === "RGBW" || order === "GRBW" ? 4 : 3);

const pushOnce = (notes: string[], note: string) => {
  if (!notes.includes(note)) notes.push(note);
};

/** The setup the show wants `controller` to have: strings back to back from channel 1. */
export function showSetup(show: Show, controller: Controller, onePerPort: boolean): Setup {
  const notes: string[] = [];
  let channel = 1;
  const ports: SetupPort[] = [];
  for (const port of controller.ports) {
    let strings: SetupString[] = [];
    port.slots.forEach((slot, i) => {
      const prop = show.props.find((p) => p.id === slot.prop);
      if (!prop) return;
      const nodes = nodeCount(prop.shape);
      const range = slot.segment ?? { start: 0, end: nodes };
      if (range.start > range.end || range.end > nodes) return;
      const perPixel = cpp(prop.colorOrder);
      const pixels = slot.nullPixels + (range.end - range.start);
      if (pixels === 0) return;
      const start = channel;
      channel += pixels * perPixel;
      if (slot.smartReceiver !== null) {
        pushOnce(notes, RECEIVER_NOTE);
        return;
      }
      strings.push({ name: prop.name, pixels, colorOrder: slot.controllerColorOrder ?? null, start, channelsPerPixel: perPixel, slots: [i] });
    });
    if (onePerPort && strings.length > 1) {
      const orders = new Set(strings.map((s) => s.colorOrder));
      if (orders.size > 1) notes.push(`Port ${port.number}: its props ask for different color orders, so the controller's own is left as it is.`);
      strings = [
        {
          name: strings.map((s) => s.name).join(" + "),
          pixels: strings.reduce((sum, s) => sum + s.pixels, 0),
          colorOrder: orders.size === 1 ? strings[0].colorOrder : null,
          start: strings[0].start,
          channelsPerPixel: Math.max(...strings.map((s) => s.channelsPerPixel)),
          slots: strings.flatMap((s) => s.slots),
        },
      ];
    }
    const existing = ports.find((p) => p.number === port.number);
    if (existing) existing.strings.push(...strings);
    else ports.push({ number: port.number, strings });
  }
  ports.sort((a, b) => a.number - b.number);
  const p = controller.protocol;
  const input: SetupInput = p.type === "ddp" ? { type: "ddp" } : { type: "sacn", startUniverse: p.startUniverse, universeSize: p.universeSize };
  return { input, ports, notes };
}

/** The setup a device reports. */
export function deviceSetup(config: DeviceConfig): Setup {
  const notes: string[] = [];
  const ports = config.ports
    .map((port) => ({
      number: port.number,
      strings: port.strings
        .filter((s) => {
          if (s.smartReceiver !== null) pushOnce(notes, RECEIVER_NOTE);
          return s.smartReceiver === null;
        })
        .map((s, i) => ({ name: s.name ?? `String ${i + 1}`, pixels: s.pixels, colorOrder: s.colorOrder, start: null, channelsPerPixel: cpp(s.colorOrder), slots: [] })),
    }))
    .sort((a, b) => a.number - b.number);
  const i = config.input;
  const input: SetupInput =
    i.type === "ddp" ? { type: "ddp" } : i.type === "sacn" ? { type: "sacn", startUniverse: i.startUniverse, universeSize: i.channelsPerUniverse } : { type: "other", description: i.description };
  return { input, ports, notes };
}

const pixelsText = (n: number) => `${thousands(n)} ${n === 1 ? "pixel" : "pixels"}`;
const inputText = (i: SetupInput) => (i.type === "ddp" ? "DDP" : i.type === "sacn" ? "sACN (E1.31)" : i.description);

/** The differences between two setups' ports and strings, port by port. */
export function diffPorts(before: Setup, after: Setup, direction: Direction): Change[] {
  const numbers = [...new Set([...before.ports, ...after.ports].map((p) => p.number))].sort((a, b) => a - b);
  const changes: Change[] = [];
  for (const number of numbers) {
    const b = before.ports.find((p) => p.number === number)?.strings ?? [];
    const a = after.ports.find((p) => p.number === number)?.strings ?? [];
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      const key = stringKey(number, i);
      const old = b[i];
      const now = a[i];
      const showSide = direction === "toDevice" ? now : old;
      const name = (showSide ?? old ?? now)?.name ?? "";
      const subject = name ? `String ${i + 1} · ${name}` : `String ${i + 1}`;
      const row = (kind: ChangeKind, id: string, what: string, before: string, after: string): Change => ({
        id,
        port: number,
        kind,
        subject,
        what,
        before,
        after,
        warning: null,
        canTake: true,
        whyNot: null,
      });
      if (old && now) {
        if (old.pixels !== now.pixels) {
          const change = row("pixels", `${key}/pixels`, "Pixels", thousands(old.pixels), thousands(now.pixels));
          if (now.pixels < old.pixels) {
            const lost = old.pixels - now.pixels;
            change.warning =
              direction === "toDevice"
                ? `${pixelsText(lost)} fewer: the last ${thousands(lost)} on this string go dark.`
                : `${name} gets ${pixelsText(lost)} shorter; effects on its last pixels are lost.`;
          }
          changes.push(change);
        }
        const plain = now.colorOrder === "RGB" || now.colorOrder === "RGBW";
        if (now.colorOrder && old.colorOrder !== now.colorOrder && !(old.colorOrder === null && plain)) {
          changes.push(row("colorOrder", `${key}/colorOrder`, "Color order", old.colorOrder ?? "Not set", now.colorOrder));
        }
        if (old.start !== null && now.start !== null && old.start !== now.start) {
          changes.push(row("start", `${key}/start`, "Starts at channel", thousands(old.start), thousands(now.start)));
        }
      } else if (now) {
        const order = now.colorOrder ? `, ${now.colorOrder}` : "";
        changes.push(row("stringAdded", key, "New string", "None", `${pixelsText(now.pixels)}${order}`));
      } else if (old) {
        const change = row("stringRemoved", key, "Removed string", pixelsText(old.pixels), "None");
        change.warning = direction === "toDevice" ? `Its ${pixelsText(old.pixels)} go dark.` : `${name} stays in your show, no longer wired here.`;
        changes.push(change);
      }
    }
  }
  return changes;
}

function diffInput(show: SetupInput, device: SetupInput): Change[] {
  const row = (kind: ChangeKind, id: string, what: string, before: string, after: string): Change => ({
    id,
    port: null,
    kind,
    subject: "",
    what,
    before,
    after,
    warning: null,
    canTake: true,
    whyNot: null,
  });
  if (show.type === "ddp" && device.type === "ddp") return [];
  if (show.type === "sacn" && device.type === "sacn") {
    const changes: Change[] = [];
    if (device.startUniverse !== null && show.startUniverse !== device.startUniverse) {
      changes.push(row("startUniverse", "input/startUniverse", "First universe", show.startUniverse === null ? "Chosen by PixelFlow" : String(show.startUniverse), String(device.startUniverse)));
    }
    if (show.universeSize !== device.universeSize) {
      const change = row("universeSize", "input/universeSize", "Channels per universe", String(show.universeSize), String(device.universeSize));
      if (device.universeSize < 1 || device.universeSize > 512) {
        change.canTake = false;
        change.whyNot = "A universe carries 1 to 512 channels.";
      }
      changes.push(change);
    }
    return changes;
  }
  const change = row("receives", "input/receives", "Receives", inputText(show), inputText(device));
  if (device.type === "other") {
    change.canTake = false;
    change.whyNot = `PixelFlow can't send ${device.description} yet.`;
  }
  return [change];
}

function showString(ours: Setup, change: Change): SetupString | undefined {
  const index = Number(change.id.split("/")[1]?.replace("string", "")) - 1;
  return ours.ports.find((p) => p.number === change.port)?.strings[index];
}

function cannotTake(show: Show, controller: Controller, ours: Setup, change: Change): string | null {
  if (!change.canTake) return change.whyNot;
  if (change.kind === "pixels") {
    const string = showString(ours, change);
    const port = controller.ports.find((p) => p.number === change.port);
    if (!string || !port) return null;
    if (string.slots.length !== 1) return "Several props share this output; change their sizes on the Layout screen.";
    const slot = port.slots[string.slots[0]];
    const prop = show.props.find((p) => p.id === slot.prop);
    if (!prop) return null;
    if (prop.shape.source !== "generator" || prop.shape.type !== "line" || slot.segment) return `${prop.name}'s shape sets its size; change it on the Layout screen.`;
    const devicePixels = Number(change.after.replace(/,/g, ""));
    return devicePixels <= slot.nullPixels ? `${prop.name} starts with ${slot.nullPixels} null pixels on this port.` : null;
  }
  if (change.kind === "start") return "PixelFlow places strings back to back. Use Send setup to set the controller to match.";
  return null;
}

/** "Compare with this device": show (before) → device (after). */
export function compareSetup(show: Show, controller: Controller, kind: DeviceKind, config: DeviceConfig): { changes: Change[]; notes: string[] } {
  const ours = showSetup(show, controller, oneStringPerPort(kind));
  const theirs = deviceSetup(config);
  const changes = [...diffInput(ours.input, theirs.input), ...diffPorts(ours, theirs, "intoShow")];
  for (const change of changes) {
    const reason = cannotTake(show, controller, ours, change);
    if (reason) {
      change.canTake = false;
      change.whyNot = reason;
    }
  }
  const notes = [...ours.notes];
  for (const note of theirs.notes) pushOnce(notes, note);
  return { changes, notes };
}

function uniqueProp(base: string, taken: Set<string>): string {
  let name = base;
  for (let n = 2; taken.has(name); n++) name = `${base} ${n}`;
  taken.add(name);
  return name;
}

/** Takes the picked differences into the show's `controller` (throws, changing nothing, on a
 * pick that isn't a difference that can be taken). */
export function takeFromDevice(
  show: Show,
  controller: Controller,
  kind: DeviceKind,
  config: DeviceConfig,
  picks: string[],
  useProps: UseProps = {},
): { controller: Controller; newProps: Prop[]; changedProps: Prop[] } {
  const { changes } = compareSetup(show, controller, kind, config);
  const ours = showSetup(show, controller, oneStringPerPort(kind));
  const theirs = deviceSetup(config);
  const picked = picks.map((id) => {
    const change = changes.find((c) => c.id === id);
    if (!change) throw new Error("The controller or your show changed since you compared them. Compare again.");
    if (!change.canTake) throw new Error(change.whyNot ?? "That difference can't be taken into your show.");
    return change;
  });
  const out: Controller = structuredClone(controller);
  const changedProps: Prop[] = [];
  const newProps: Prop[] = [];
  const names = new Set(show.props.map((p) => p.name));
  const removals: [number, number[]][] = [];
  const deviceString = (change: Change) => {
    const index = Number(change.id.split("/")[1]?.replace("string", "")) - 1;
    return theirs.ports.find((p) => p.number === change.port)?.strings[index];
  };
  for (const change of picked) {
    const port = out.ports.find((p) => p.number === change.port);
    const device = deviceString(change);
    switch (change.kind) {
      case "pixels": {
        const string = showString(ours, change);
        const slot = string && port?.slots[string.slots[0]];
        const original = slot && show.props.find((p) => p.id === slot.prop);
        if (!slot || !original || !device || original.shape.source !== "generator" || original.shape.type !== "line") break;
        const nodes = device.pixels - slot.nullPixels;
        const prop = structuredClone(changedProps.find((p) => p.id === original.id) ?? original);
        if (prop.shape.source === "generator" && prop.shape.type === "line") {
          prop.shape.length = Math.max(0.01, (prop.shape.length * nodes) / Math.max(1, prop.shape.nodes));
          prop.shape.nodes = nodes;
        }
        changedProps.splice(0, changedProps.length, ...changedProps.filter((p) => p.id !== prop.id), prop);
        break;
      }
      case "colorOrder": {
        const string = showString(ours, change);
        if (!string || !port || !device) break;
        for (const i of string.slots) if (port.slots[i]) port.slots[i].controllerColorOrder = device.colorOrder;
        break;
      }
      case "stringAdded": {
        if (!device || change.port === null) break;
        let propId = useProps[change.id];
        if (propId !== undefined) {
          if (!show.props.some((p) => p.id === propId)) throw new Error("A prop you picked is no longer in your show. Compare again.");
        } else {
          const fallback = `${out.name} Port ${change.port} ${change.id.split("/")[1].replace("string", "String ")}`;
          const base = device.name && !device.name.startsWith("String ") ? device.name : fallback;
          const prop: Prop = {
            id: crypto.randomUUID(),
            name: uniqueProp(base, names),
            shape: { source: "generator", type: "line", nodes: device.pixels, length: Math.max(1, device.pixels * 0.05) },
            transform: { position: { x: 0, y: -(show.props.length + newProps.length) * 0.5, z: 0 }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } },
            colorOrder: device.channelsPerPixel === 4 ? "RGBW" : "RGB",
            regions: [],
            tags: [],
          };
          newProps.push(prop);
          propId = prop.id;
        }
        let target = port;
        if (!target) {
          target = { number: change.port, maxPixels: null, brightness: 100, gamma: 1, slots: [] };
          const at = out.ports.findIndex((p) => p.number > change.port!);
          out.ports.splice(at < 0 ? out.ports.length : at, 0, target);
        }
        const slot: PortSlot = {
          prop: propId,
          segment: null,
          nullPixels: 0,
          reverse: false,
          brightness: null,
          gamma: null,
          smartReceiver: null,
          controllerColorOrder: device.colorOrder,
        };
        target.slots.push(slot);
        break;
      }
      case "stringRemoved": {
        const string = showString(ours, change);
        if (string && change.port !== null) removals.push([change.port, string.slots]);
        break;
      }
      case "receives":
      case "startUniverse":
      case "universeSize": {
        const input = theirs.input;
        if (input.type === "ddp" && change.kind === "receives") out.protocol = { type: "ddp" };
        if (input.type === "sacn") {
          const sacn = out.protocol.type === "sacn" ? { ...out.protocol } : { type: "sacn" as const, startUniverse: null, universeSize: 510, allowPixelStraddle: false, multicast: false };
          const size = input.universeSize >= 1 && input.universeSize <= 512 ? input.universeSize : null;
          if (change.kind === "startUniverse") sacn.startUniverse = input.startUniverse;
          else if (change.kind === "universeSize") sacn.universeSize = size ?? sacn.universeSize;
          else {
            sacn.startUniverse = input.startUniverse;
            sacn.universeSize = size ?? 510;
          }
          out.protocol = sacn;
        }
        break;
      }
    }
  }
  for (const [number, slots] of removals) {
    const port = out.ports.find((p) => p.number === number);
    if (!port) continue;
    for (const i of [...slots].sort((a, b) => b - a)) if (i < port.slots.length) port.slots.splice(i, 1);
  }
  return { controller: out, newProps, changedProps };
}

/** A device's configuration after a send of the show's setup: each port's strings take the
 * show's pixel counts and color orders (the device keeps its own other settings). For the
 * in-memory backend's pretend devices. */
export function applySetup(config: DeviceConfig, target: Setup): DeviceConfig {
  const next = structuredClone(config);
  next.ports = target.ports
    .filter((p) => p.strings.length > 0)
    .map((port) => {
      const before = config.ports.find((p) => p.number === port.number);
      return {
        number: port.number,
        maxPixels: before?.maxPixels ?? null,
        strings: port.strings.map((s, i) => {
          const old = before?.strings[i];
          return {
            name: old?.name ?? s.name,
            pixels: s.pixels,
            colorOrder: s.colorOrder ?? old?.colorOrder ?? (s.channelsPerPixel === 4 ? "RGBW" : "RGB"),
            nullPixels: old?.nullPixels ?? 0,
            reverse: old?.reverse ?? false,
            brightness: old?.brightness ?? 100,
            gamma: old?.gamma ?? 1,
            smartReceiver: null,
          };
        }),
      };
    });
  return next;
}
