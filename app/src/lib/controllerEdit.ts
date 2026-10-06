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

/** What's wrong with an address, or null when it looks usable (an IPv4 address or a host name). */
export function addressProblem(raw: string): string | null {
  const address = raw.trim();
  if (!address) return "Enter the controller's IP address, like 192.168.1.50.";
  if (/^[a-z]+:\/\/|\//i.test(address)) return "Enter just the address (like 10.0.0.5), without http:// or a slash.";
  if (/\s/.test(address)) return "An address can't have spaces. Enter an IP address like 192.168.1.50, or a name like fpp.local.";
  if (/^[\d.]+$/.test(address)) {
    const parts = address.split(".");
    if (parts.length !== 4 || parts.some((p) => p === "")) return `${address} isn't a complete IP address: it needs four numbers, like 192.168.1.50.`;
    if (parts.some((p) => Number(p) > 255)) return `${address} isn't a valid IP address: each of the four numbers must be 0 to 255.`;
    return null;
  }
  if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*\.?$/i.test(address)) {
    return `${address} isn't an address PixelFlow can use. Enter an IP address like 192.168.1.50, or a name like fpp.local.`;
  }
  return null;
}

/** The pinned start universe, null for "let PixelFlow choose", or undefined when it isn't valid. */
function startUniverse(draft: ControllerDraft): number | null | undefined {
  const text = draft.startUniverse.trim();
  if (!text) return null;
  const n = Number(text);
  return Number.isInteger(n) && n >= 1 && n <= MAX_UNIVERSE ? n : undefined;
}

/** Everything wrong with the form, by field; empty when it can be saved. */
export function draftProblems(draft: ControllerDraft, show: Show, id: string): DraftProblems {
  const problems: DraftProblems = {};
  const others = show.controllers.filter((c) => c.id !== id);
  const name = draft.name.trim();
  if (!name) problems.name = "Give the controller a name.";
  else if (others.some((c) => c.name.trim().toLowerCase() === name.toLowerCase())) problems.name = `Another controller is already called ${name}.`;
  const address = addressProblem(draft.address);
  const clash = others.find((c) => c.address.trim().toLowerCase() === draft.address.trim().toLowerCase());
  if (address) problems.address = address;
  else if (clash) problems.address = `${clash.name} already uses ${draft.address.trim()}.`;
  if (draft.protocol === "sacn" && startUniverse(draft) === undefined) {
    problems.startUniverse = `The start universe must be a whole number from 1 to ${MAX_UNIVERSE}, or empty to let PixelFlow choose.`;
  }
  return problems;
}

function protocolOf(draft: ControllerDraft, before: Protocol): Protocol {
  if (draft.protocol === "ddp") return { type: "ddp" };
  const kept = before.type === "sacn" ? before : { allowPixelStraddle: false };
  return {
    type: "sacn",
    startUniverse: startUniverse(draft) ?? null,
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
