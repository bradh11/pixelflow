// Editing a controller in place: what the form holds, what's wrong with it (in plain words), and
// the one edit that saves it. The controller keeps its id, ports, wiring, kind, and anything found
// on the network (its place in FPP sequences), so nothing has to be wired again.

import type { Controller, Edit, Protocol, Show } from "../api/types";

/** The highest sACN universe (the engine's limit). */
export const MAX_UNIVERSE = 63_999;

/** The edit form, as typed. */
export interface ControllerDraft {
  name: string;
  address: string;
  protocol: "ddp" | "sacn";
  /** Empty lets PixelFlow choose the universes. */
  startUniverse: string;
  universeSize: 510 | 512;
  multicast: boolean;
}

export type DraftProblems = Partial<Record<"name" | "address" | "startUniverse", string>>;

export function controllerDraft(c: Controller): ControllerDraft {
  const sacn = c.protocol.type === "sacn" ? c.protocol : null;
  return {
    name: c.name,
    address: c.address,
    protocol: c.protocol.type,
    startUniverse: sacn?.startUniverse != null ? String(sacn.startUniverse) : "",
    universeSize: sacn?.universeSize ?? 510,
    multicast: sacn?.multicast ?? false,
  };
}

const PORT_PROBLEM = "The port after the colon must be a number from 1 to 65535, or leave the colon off.";
const HOST = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*\.?$/i;

/**
 * What's wrong with an address, or null when the engine can send to it. Mirrors `resolve` in
 * crates/pf-output/src/plan.rs: an IPv4 address or a host name, each with an optional `:port`
 * (the engine looks names up and uses their IPv4 address). `emptyOk`: no address is needed
 * (multicast sACN goes to the universe's group address instead).
 */
export function addressProblem(raw: string, { emptyOk = false }: { emptyOk?: boolean } = {}): string | null {
  const address = raw.trim();
  if (!address) return emptyOk ? null : "Enter the controller's IP address, like 192.168.1.50.";
  if (/^[a-z]+:\/\/|\//i.test(address)) return "Enter just the address (like 10.0.0.5), without http:// or a slash.";
  if (/\s/.test(address)) return "An address can't have spaces. Enter an IP address like 192.168.1.50, or a name like fpp.local.";
  const colons = address.split(":").length - 1;
  if (colons > 1) return "PixelFlow sends to IPv4 addresses. Enter one like 192.168.1.50.";
  const [host, port] = colons === 1 ? address.split(":") : [address, null];
  if (port !== null && !(/^\d{1,5}$/.test(port) && Number(port) >= 1 && Number(port) <= 65535)) return PORT_PROBLEM;
  if (/^[\d.]+$/.test(host)) {
    const parts = host.split(".");
    if (parts.length !== 4 || parts.some((p) => p === "")) return `${host} isn't a complete IP address: it needs four numbers, like 192.168.1.50.`;
    if (parts.some((p) => Number(p) > 255)) return `${host} isn't a valid IP address: each of the four numbers must be 0 to 255.`;
    if (parts.some((p) => p.length > 1 && p.startsWith("0"))) {
      const fixed = parts.map((p) => String(Number(p))).join(".");
      return `${host} has a number that starts with 0, which some computers read differently. Write it as ${fixed}.`;
    }
    return null;
  }
  if (!HOST.test(host)) return `${host} isn't an address PixelFlow can use. Enter an IP address like 192.168.1.50, or a name like fpp.local.`;
  return null;
}

/** The pinned start universe, null for "let PixelFlow choose", or undefined when it isn't valid. */
function startUniverse(draft: ControllerDraft): number | null | undefined {
  const text = draft.startUniverse.trim();
  if (!text) return null;
  if (!/^\d+$/.test(text)) return undefined;
  const n = Number(text);
  return n >= 1 && n <= MAX_UNIVERSE ? n : undefined;
}

/** What stops the form saving (`problems`), and what's worth knowing but allowed (`warnings`). */
export interface DraftCheck {
  problems: DraftProblems;
  warnings: DraftProblems;
}

/**
 * Checks only what the user changed from `before`, so any controller PixelFlow made (including
 * imported ones that share an address or have none) can be edited. The engine itself allows
 * duplicate names and shared addresses (one controller can be split into several), so those are
 * warnings; what it can't use (no name, an address it can't send to, a universe out of range) stops
 * the save.
 */
export function checkDraft(draft: ControllerDraft, before: Controller, show: Show): DraftCheck {
  const problems: DraftProblems = {};
  const warnings: DraftProblems = {};
  const was = controllerDraft(before);
  const others = show.controllers.filter((c) => c.id !== before.id);
  const name = draft.name.trim();
  if (name !== was.name.trim()) {
    if (!name) problems.name = "Give the controller a name.";
    else {
      const same = others.find((c) => c.name.trim().toLowerCase() === name.toLowerCase());
      if (same) warnings.name = `${same.name} has that name too. That's allowed, but it's easy to mix them up.`;
    }
  }
  const multicast = draft.protocol === "sacn" && draft.multicast;
  const wasMulticast = was.protocol === "sacn" && was.multicast;
  const address = draft.address.trim();
  if (address !== was.address.trim() || (wasMulticast && !multicast)) {
    const problem = addressProblem(address, { emptyOk: multicast });
    const shared = address ? others.find((c) => c.address.trim().toLowerCase() === address.toLowerCase()) : undefined;
    if (problem) problems.address = problem;
    else if (shared) warnings.address = `${shared.name} also uses ${address}. That's fine if it's the same controller.`;
  }
  if (draft.protocol === "sacn" && draft.startUniverse.trim() !== was.startUniverse.trim() && startUniverse(draft) === undefined) {
    problems.startUniverse = `The start universe must be a whole number from 1 to ${MAX_UNIVERSE}, or empty to let PixelFlow choose.`;
  }
  return { problems, warnings };
}

function protocolOf(draft: ControllerDraft, before: Protocol): Protocol {
  if (draft.protocol === "ddp") return { type: "ddp" };
  const kept = before.type === "sacn" ? before : { allowPixelStraddle: false, startUniverse: null };
  const universe = startUniverse(draft);
  return {
    type: "sacn",
    // An untouched start universe the form can't read (out of range in an old file) stays as it was.
    startUniverse: universe === undefined ? kept.startUniverse : universe,
    universeSize: draft.universeSize,
    allowPixelStraddle: kept.allowPixelStraddle,
    multicast: draft.multicast,
  };
}

/**
 * The controller `id` changed to match the form, as one edit (one undo step), built from the show
 * as it is when its turn comes so wiring changed meanwhile is kept. Nothing when nothing changed.
 */
export function controllerEdits(id: string, draft: ControllerDraft): (show: Show) => Edit[] {
  return (show) => {
    const before = show.controllers.find((c) => c.id === id);
    if (!before) return [];
    const after: Controller = { ...before, name: draft.name.trim(), address: draft.address.trim(), protocol: protocolOf(draft, before.protocol) };
    const same = after.name === before.name && after.address === before.address && JSON.stringify(after.protocol) === JSON.stringify(before.protocol);
    return same ? [] : [{ type: "updateController", controller: after }];
  };
}
