import { ArrowDownToLine, Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { PreviewProp } from "../../api/types";
import { thousands } from "../../lib/format";
import { type WireSession, answer, clickProp, doOp, draftShow, sessionEdits, setReceiver } from "../../lib/wireSession";
import { capacityOptions, findPort, nodeCounts, portCapacity, propWiring, receiverName, slotLabel } from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { portKey, useWiring } from "../../state/wiring";
import { Button } from "../ui";
import type { WiringData } from "./ControllerCard";
import { CapacityBar } from "./PortRow";
import { WiringCanvas } from "./WiringPreview";

/** Most props listed to add before the search has to narrow them. */
const LIST_MAX = 60;

/** The session as it is now (a handler may run after newer clicks than its render saw). */
const current = () => useWiring.getState().session;
const update = (change: (s: WireSession) => WireSession) => {
  const s = current();
  if (s) useWiring.setState({ session: change(s) });
};

/** Leaves the mode, giving focus back to the port's Wire button. */
function leave(session: WireSession) {
  useWiring.setState({ session: null });
  setTimeout(() => document.querySelector<HTMLElement>(`[data-wire-button="${portKey(session.port.controller, session.port.at)}"]`)?.focus());
}

/**
 * Wiring one port by clicking props on the layout, in the order the wire runs: each click adds
 * the prop to the end of the chain, numbered on the picture, with the wire drawn through it.
 * Nothing changes in the show until Done, which makes the whole session one undo step; Cancel
 * leaves it as it was, and Escape asks first when there are changes to lose. The list beside it
 * does the same from the keyboard.
 */
export function WireMode({ session, data, preview }: { session: WireSession; data: WiringData; preview: PreviewProp[] }) {
  const apply = useApp((s) => s.apply);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const [query, setQuery] = useState("");
  const [lit, setLit] = useState<string | null>(null);
  const draft = useMemo(() => draftShow(data.show, session, data.nodes), [data, session]);
  const wiring = useMemo(() => propWiring(draft, data.nodes), [draft, data.nodes]);
  const controller = draft.controllers.find((c) => c.id === session.port.controller);
  const port = findPort(draft, session.port);
  const name = (id: string) => data.propById.get(id)?.name ?? "Missing prop";

  useEffect(() => headingRef.current?.focus(), []);
  // The port went away underneath (an undo, say): nothing left to wire.
  useEffect(() => {
    if (!port) useWiring.setState({ session: null });
  }, [port]);

  const finish = async () => {
    const s = current();
    if (!s) return;
    leave(s);
    if (s.ops.length === 0) return;
    // Built from the show as it is when the edit's turn comes.
    await apply((show) => sessionEdits(show, s, nodeCounts(useApp.getState().snapshot?.channelMap ?? { frameLen: 0, props: [], controllers: [] })));
  };
  const cancel = () => {
    const s = current();
    if (s) leave(s);
  };
  const pick = (prop: string) => update((s) => clickProp(data.show, s, data.nodes, prop));

  // Escape closes the open question; then, with changes made, asks whether to keep them (Escape
  // again goes back to wiring); with none, it leaves.
  const [asking, setAsking] = useState(false);
  const askingRef = useRef(asking);
  askingRef.current = asking;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      const s = current();
      if (s?.prompt) update((x) => answer(x, "cancel"));
      else if (askingRef.current) setAsking(false);
      else if (s && s.ops.length > 0) setAsking(true);
      else cancel();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const choices = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const onPort = new Set(port?.slots.map((s) => s.prop));
    const all = draft.props.filter((p) => !onPort.has(p.id) && (!needle || p.name.toLowerCase().includes(needle)));
    // Unwired first, left to right as in the layout; then the rest in show order.
    const order = new Map(data.unwired.map((id, i) => [id, i]));
    const rank = (id: string) => (wiring.get(id)?.status === "unwired" ? (order.get(id) ?? 0) : Number.MAX_SAFE_INTEGER);
    return all.map((p, i) => ({ p, i })).sort((a, b) => rank(a.p.id) - rank(b.p.id) || a.i - b.i).map((x) => x.p);
  }, [draft, port, query, wiring, data.unwired]);

  if (!controller || !port) return null;
  const capacity = portCapacity(port, data.nodes, capacityOptions(draft, controller, data.cpp));
  // A–D, and any other receiver the port uses; offered when the port feeds receivers.
  const inUse = [...(findPort(data.show, session.port)?.slots ?? []), ...port.slots].flatMap((s) => (s.smartReceiver === null ? [] : [s.smartReceiver]));
  const receivers = [...new Set([1, 2, 3, 4, ...inUse])].sort((a, b) => a - b);
  const usesReceivers = inUse.length > 0;
  const where = (id: string) => {
    const place = wiring.get(id)?.places[0];
    return place ? `${place.controllerName} · Port ${place.port}` : "";
  };
  const describe = (id: string) => {
    const at = port.slots.findIndex((s) => s.prop === id);
    if (at >= 0) return `${name(id)}: ${at + 1} on this port. Click to remove it or move it to the end.`;
    const w = wiring.get(id);
    if (!w) return name(id);
    if (w.status === "unwired") return `${name(id)} · ${thousands(w.nodes)} px. Click to add it next.`;
    if (w.status === "partial") return `${name(id)}: ${thousands(w.wiredPixels)} of ${thousands(w.nodes)} px wired. Click to add the rest here.`;
    return `${name(id)} is on ${where(id)}. Click to move it here.`;
  };
  const prompt = asking ? null : session.prompt;
  const changes = session.ops.length === 1 ? "1 change" : `${session.ops.length} changes`;

  return (
    <section aria-labelledby="wire-mode-title" className="@container">
      <div className="flex flex-col gap-3 @min-[860px]:flex-row @min-[860px]:items-start">
        <div className="relative min-w-0 flex-1 overflow-hidden rounded-lg border border-neutral-200 bg-neutral-950 dark:border-neutral-800">
          <WiringCanvas
            show={draft}
            props={preview}
            port={port}
            highlight={lit}
            badges
            maxHeight="calc(100vh - 11rem)"
            onPick={pick}
            pickLabel={describe}
          />
          {prompt && (
            <div
              role="group"
              aria-label="What to do with this prop"
              className="absolute inset-x-0 top-3 mx-auto flex w-fit max-w-[95%] flex-wrap items-center gap-2 rounded-lg border border-neutral-300 bg-white px-3 py-2 text-sm shadow-lg dark:border-neutral-700 dark:bg-neutral-900"
            >
              {prompt.kind === "elsewhere" ? (
                <>
                  <span>
                    {name(prompt.prop)} is on {draft.controllers.find((c) => c.id === prompt.controller)?.name ?? "another controller"} · Port {prompt.port}. Move it here?
                  </span>
                  <Button variant="primary" autoFocus onClick={() => update((s) => answer(s, "move"))}>
                    Move it here
                  </Button>
                </>
              ) : (
                <>
                  <span>{name(prompt.prop)} is already on this port.</span>
                  <Button variant="danger" autoFocus onClick={() => update((s) => answer(s, "remove"))}>
                    Remove
                  </Button>
                  {!prompt.last && <Button onClick={() => update((s) => answer(s, "toEnd"))}>Move to end</Button>}
                </>
              )}
              <Button variant="ghost" onClick={() => update((s) => answer(s, "cancel"))}>
                Cancel
              </Button>
            </div>
          )}
          {asking && (
            <div
              role="alertdialog"
              aria-label={`Keep the ${changes} to Port ${port.number}?`}
              className="absolute inset-x-0 top-3 mx-auto flex w-fit max-w-[95%] flex-wrap items-center gap-2 rounded-lg border border-neutral-300 bg-white px-3 py-2 text-sm shadow-lg dark:border-neutral-700 dark:bg-neutral-900"
            >
              <span>
                Keep the {changes} to Port {port.number}?
              </span>
              <Button variant="primary" autoFocus onClick={() => void finish()}>
                Keep
              </Button>
              <Button variant="danger" onClick={cancel}>
                Discard
              </Button>
              <Button variant="ghost" onClick={() => setAsking(false)}>
                Back to wiring
              </Button>
            </div>
          )}
          <p className="pointer-events-none absolute bottom-2 left-3 text-xs text-white/70">
            Click props in the order the wire runs. Done keeps them; Esc asks before throwing changes away.
          </p>
        </div>

        <aside aria-label="Wiring this port" className="flex w-full shrink-0 flex-col gap-3 @min-[860px]:w-80">
          <div className="rounded-lg border border-accent-500/50 bg-white p-3 dark:bg-neutral-900">
            <h2 id="wire-mode-title" ref={headingRef} tabIndex={-1} className="text-sm font-semibold focus:outline-none">
              Wiring {controller.name} · Port {port.number}
            </h2>
            <p className="mt-0.5 text-xs text-neutral-500">Click props on the picture in the order the wire reaches them.</p>
            <div className="mt-2">
              <CapacityBar port={port} controller={controller} c={capacity} />
              {capacity.message && (
                <p className={`mt-1 text-xs ${capacity.level === "over" ? "text-red-600 dark:text-red-400" : "text-amber-700 dark:text-amber-400"}`}>{capacity.message}</p>
              )}
            </div>
            {usesReceivers && (
              <div role="radiogroup" aria-label="Smart receiver being wired" className="mt-2 flex flex-wrap items-center gap-1 text-xs">
                <span className="mr-1 text-neutral-500">Receiver</span>
                {[null, ...receivers].map((r) => (
                  <button
                    key={r ?? "port"}
                    type="button"
                    role="radio"
                    aria-checked={session.receiver === r}
                    aria-label={r === null ? "None: wired to the port itself" : `Receiver ${receiverName(r)}`}
                    title={r === null ? "Wired straight to the port" : `Smart receiver ${receiverName(r)}`}
                    onClick={() => update((s) => setReceiver(s, r))}
                    className={`min-w-7 rounded border px-1.5 py-0.5 ${session.receiver === r ? "border-accent-500 bg-accent-50 text-accent-800 dark:bg-accent-600/20 dark:text-accent-200" : "border-neutral-300 dark:border-neutral-700"}`}
                  >
                    {r === null ? "None" : receiverName(r)}
                  </button>
                ))}
              </div>
            )}
            {session.refused && (
              <p role="status" className="mt-2 text-xs text-amber-700 dark:text-amber-400">
                {name(session.refused)} is wired in pieces on other ports. Move its pieces from their own tables.
              </p>
            )}
            <div className="mt-3 flex justify-end gap-2">
              <Button variant="ghost" onClick={cancel}>
                Cancel
              </Button>
              <Button variant="primary" onClick={() => void finish()}>
                Done
              </Button>
            </div>
          </div>

          <div className="rounded-lg border border-neutral-200 bg-white p-2 dark:border-neutral-800 dark:bg-neutral-900">
            <h3 className="px-1 text-xs font-medium text-neutral-500">On this port, in order</h3>
            {port.slots.length === 0 ? (
              <p className="px-1 py-1.5 text-xs text-neutral-500">Nothing yet.</p>
            ) : (
              <ol className="mt-1 flex max-h-56 flex-col overflow-auto" aria-label={`Port ${port.number}, in wiring order`}>
                {port.slots.map((slot, i) => {
                  const label = slotLabel(name(slot.prop), slot);
                  return (
                    <li
                      key={`${slot.prop}-${i}`}
                      className="flex items-center gap-1.5 rounded px-1 py-0.5 text-sm hover:bg-neutral-50 dark:hover:bg-neutral-800/60"
                      onPointerEnter={() => setLit(slot.prop)}
                      onPointerLeave={() => setLit(null)}
                    >
                      <span className="w-5 text-right text-xs text-neutral-400 tabular-nums">{i + 1}</span>
                      {slot.smartReceiver !== null && <span className="rounded bg-neutral-200 px-1 text-[10px] font-semibold dark:bg-neutral-700">{receiverName(slot.smartReceiver)}</span>}
                      <span className="min-w-0 flex-1 truncate">{label}</span>
                      {i < port.slots.length - 1 && (
                        <button
                          type="button"
                          aria-label={`Move ${label} to the end`}
                          title="Move to the end"
                          onClick={() => update((s) => doOp(s, { kind: "toEnd", prop: slot.prop }))}
                          className="rounded p-0.5 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
                        >
                          <ArrowDownToLine size={13} aria-hidden />
                        </button>
                      )}
                      <button
                        type="button"
                        aria-label={`Remove ${label} from this port`}
                        title="Remove from this port"
                        onClick={() => update((s) => doOp(s, { kind: "remove", prop: slot.prop }))}
                        className="rounded p-0.5 text-neutral-500 hover:bg-red-50 hover:text-red-700 dark:hover:bg-red-950/40"
                      >
                        <X size={13} aria-hidden />
                      </button>
                    </li>
                  );
                })}
              </ol>
            )}
          </div>

          <div className="rounded-lg border border-neutral-200 bg-white p-2 dark:border-neutral-800 dark:bg-neutral-900">
            <h3 className="px-1 text-xs font-medium text-neutral-500">Add next</h3>
            <label className="relative mt-1 block">
              <span className="sr-only">Find a prop to wire next</span>
              <Search size={14} className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-neutral-400" aria-hidden />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Find a prop"
                className="w-full rounded-md border border-neutral-300 bg-white py-1 pr-2 pl-7 text-sm dark:border-neutral-700 dark:bg-neutral-950"
              />
            </label>
            <ul className="mt-1 flex max-h-64 flex-col overflow-auto" aria-label="Props to add, in the order you pick them">
              {choices.slice(0, LIST_MAX).map((p) => {
                const w = wiring.get(p.id);
                const detail = w?.status === "unwired" ? `${thousands(w.nodes)} px` : w?.status === "partial" ? "the rest" : where(p.id);
                return (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => pick(p.id)}
                      onPointerEnter={() => setLit(p.id)}
                      onPointerLeave={() => setLit(null)}
                      className="flex w-full items-baseline gap-2 rounded px-1.5 py-1 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800"
                    >
                      <span className="min-w-0 flex-1 truncate">{p.name}</span>
                      <span className={`shrink-0 text-xs ${w?.status === "unwired" ? "text-neutral-500" : "text-amber-700 dark:text-amber-400"}`}>{detail}</span>
                    </button>
                  </li>
                );
              })}
              {choices.length > LIST_MAX && <li className="px-1.5 py-1 text-xs text-neutral-500">{choices.length - LIST_MAX} more: type to narrow the list.</li>}
              {choices.length === 0 && <li className="px-1.5 py-1 text-xs text-neutral-500">{query.trim() ? `No prop matches “${query.trim()}”.` : "Every prop is on this port."}</li>}
            </ul>
          </div>
        </aside>
      </div>
    </section>
  );
}
