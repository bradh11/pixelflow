// Click-to-wire: the user clicks props on the layout in the order the wire runs, and each click
// adds that prop to the end of one port's chain. The session keeps the gestures (not a copy of
// the show), so the draft is the show as it is now with the gestures played over it, and Done
// sends them all as one batch of edits: one undo step. Nothing here touches the DOM.

import type { Controller, Edit, Port, PortSlot, Show } from "../api/types";
import { type NodeCounts, type PortRef, findPort, propWiring, wireAction, wirePropEdits } from "./wiringMath";

/** One gesture of the session. `add` wires the prop at the end of the chain (of its receiver's
 * run, on a port with smart receivers), moving it from another port if it's wired there. */
export type WireOp = { kind: "add"; prop: string; receiver: number | null } | { kind: "remove"; prop: string } | { kind: "toEnd"; prop: string };

/** A question the session is waiting on: what to do with a prop already in this chain, or with
 * one wired on another port. */
export type WirePrompt = { kind: "inChain"; prop: string; last: boolean } | { kind: "elsewhere"; prop: string; controller: string; port: number };

export interface WireSession {
  /** The port being wired; `at` is always set. */
  port: Required<PortRef>;
  /** The smart receiver props are added to (null: the port itself). */
  receiver: number | null;
  ops: WireOp[];
  prompt: WirePrompt | null;
  /** A prop that couldn't be added (it's wired in several pieces elsewhere). */
  refused: string | null;
}

/** A session on the port, adding to the receiver its last prop is on. */
export function startSession(show: Show, ref: Required<PortRef>): WireSession {
  const last = findPort(show, ref)?.slots.at(-1);
  return { port: ref, receiver: last?.smartReceiver ?? null, ops: [], prompt: null, refused: null };
}

/** The show with these controllers' edits made. */
function withEdits(show: Show, edits: Edit[]): Show {
  if (edits.length === 0) return show;
  const changed = new Map<string, Controller>();
  for (const e of edits) if (e.type === "updateController") changed.set(e.controller.id, e.controller);
  return { ...show, controllers: show.controllers.map((c) => changed.get(c.id) ?? c) };
}

function changeSlots(show: Show, ref: PortRef, change: (slots: PortSlot[]) => PortSlot[]): Show {
  const controller = show.controllers.find((c) => c.id === ref.controller);
  const port = findPort(show, ref);
  if (!controller || !port) return show;
  return { ...show, controllers: show.controllers.map((c) => (c === controller ? { ...c, ports: c.ports.map((p) => (p === port ? { ...p, slots: change(p.slots) } : p)) } : c)) };
}

/** Where a prop added to `receiver` goes: after that receiver's last prop, else before the first
 * prop on a later receiver (receivers stay in order along the port), else at the end. */
export function insertIndex(port: Port, receiver: number | null): number {
  if (receiver === null) return port.slots.length;
  for (let i = port.slots.length - 1; i >= 0; i--) if (port.slots[i].smartReceiver === receiver) return i + 1;
  const later = port.slots.findIndex((s) => s.smartReceiver !== null && s.smartReceiver > receiver);
  return later < 0 ? port.slots.length : later;
}

const inserted = (slots: PortSlot[], index: number, slot: PortSlot) => [...slots.slice(0, index), slot, ...slots.slice(index)];

/** The show with the session's gestures played over it, in order. A gesture that no longer
 * applies (its prop is gone, say) does nothing. */
export function draftShow(show: Show, session: WireSession, nodes: NodeCounts): Show {
  const ref = session.port;
  // Slots this session took off the port, so adding the prop back keeps its settings.
  const removed = new Map<string, PortSlot>();
  let s = show;
  for (const op of session.ops) {
    const port = findPort(s, ref);
    if (!port) return s;
    const mine = port.slots.filter((slot) => slot.prop === op.prop);
    if (op.kind === "remove") {
      if (mine[0] && !removed.has(op.prop)) removed.set(op.prop, mine[0]);
      s = changeSlots(s, ref, (slots) => slots.filter((slot) => slot.prop !== op.prop));
    } else if (op.kind === "toEnd") {
      if (mine.length === 0) continue;
      s = changeSlots(s, ref, (slots) => {
        let rest = slots.filter((slot) => slot.prop !== op.prop);
        for (const slot of mine) rest = inserted(rest, insertIndex({ ...port, slots: rest }, slot.smartReceiver), slot);
        return rest;
      });
    } else {
      if (mine.length > 0) continue;
      const index = insertIndex(port, op.receiver);
      const back = removed.get(op.prop);
      if (back && propWiring(s, nodes).get(op.prop)?.status === "unwired") {
        s = changeSlots(s, ref, (slots) => inserted(slots, index, { ...back, smartReceiver: op.receiver }));
        continue;
      }
      const next = withEdits(s, wirePropEdits(s, op.prop, { ...ref, index }, nodes));
      if (next === s) continue;
      s = changeSlots(next, ref, (slots) => slots.map((slot, i) => (i === index && slot.prop === op.prop ? { ...slot, smartReceiver: op.receiver } : slot)));
    }
  }
  return s;
}

/** The port as the session has it now. */
export function draftPort(show: Show, session: WireSession, nodes: NodeCounts): Port | null {
  return findPort(draftShow(show, session, nodes), session.port);
}

const push = (session: WireSession, op: WireOp): WireSession => ({ ...session, ops: [...session.ops, op], prompt: null, refused: null });

/**
 * A click on a prop: a prop not wired anywhere (or the rest of one wired in part) is added to
 * the end of the chain; one already in the chain, or wired on another port, asks first.
 */
export function clickProp(show: Show, session: WireSession, nodes: NodeCounts, prop: string): WireSession {
  const draft = draftShow(show, session, nodes);
  const port = findPort(draft, session.port);
  if (!port || !draft.props.some((p) => p.id === prop)) return session;
  const last = port.slots.map((s) => s.prop).lastIndexOf(prop);
  if (last >= 0) return { ...session, prompt: { kind: "inChain", prop, last: last === port.slots.length - 1 }, refused: null };
  const action = wireAction(draft, prop, nodes);
  if (!action) return { ...session, prompt: null, refused: prop };
  if (action.kind === "move") return { ...session, prompt: { kind: "elsewhere", prop, controller: action.from.controller, port: action.from.port }, refused: null };
  return push(session, { kind: "add", prop, receiver: session.receiver });
}

/** The answer to the open question: take the prop off this port, move it to the end of the
 * chain, move it here from its other port, or leave things as they are. */
export function answer(session: WireSession, choice: "remove" | "toEnd" | "move" | "cancel"): WireSession {
  const prompt = session.prompt;
  if (!prompt || choice === "cancel") return { ...session, prompt: null };
  if (prompt.kind === "elsewhere") return choice === "move" ? push(session, { kind: "add", prop: prompt.prop, receiver: session.receiver }) : session;
  if (choice === "remove") return push(session, { kind: "remove", prop: prompt.prop });
  if (choice === "toEnd") return push(session, { kind: "toEnd", prop: prompt.prop });
  return session;
}

/** A gesture made straight from the keyboard list (no question asked). */
export function doOp(session: WireSession, op: WireOp): WireSession {
  return push(session, op);
}

export function setReceiver(session: WireSession, receiver: number | null): WireSession {
  return { ...session, receiver };
}

/** The session's edits against the show as it is now: one update for each controller it
 * changes, all in one batch (one undo step). Nothing when it changes nothing. */
export function sessionEdits(show: Show, session: WireSession, nodes: NodeCounts): Edit[] {
  const draft = draftShow(show, session, nodes);
  return draft.controllers.flatMap((c, i) => (c === show.controllers[i] || JSON.stringify(c) === JSON.stringify(show.controllers[i]) ? [] : [{ type: "updateController" as const, controller: c }]));
}
