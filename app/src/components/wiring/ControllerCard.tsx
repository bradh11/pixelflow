import { ChevronDown, ChevronRight, Pencil, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import type { ChannelMap, Controller, Prop, Show } from "../../api/types";
import { plural, thousands } from "../../lib/format";
import { type NodeCounts, type PropWiring, addPortEdits, capacityOptions, portCapacity, portPixels } from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { toastWithUndo } from "../../state/undoToast";
import { useWiring } from "../../state/wiring";
import { Button } from "../ui";
import { ControllerEditForm } from "./ControllerEditForm";
import { checkDraft, controllerDraft, controllerEdits } from "../../lib/controllerEdit";
import { PortRow } from "./PortRow";

/** What every port row needs to know about the show. */
export interface WiringData {
  show: Show;
  nodes: NodeCounts;
  /** Channels per pixel by prop. */
  cpp: ReadonlyMap<string, number>;
  propById: ReadonlyMap<string, Prop>;
  wiring: Map<string, PropWiring>;
  channelMap: ChannelMap;
  /** Unwired props, left to right in the layout. */
  unwired: string[];
}

const ADAPTERS: Record<Controller["adapter"], string> = { fpp: "FPP", falcon: "Falcon", wled: "WLED", generic: "" };

/** The controller's name, typed over in place (double-click the name): Enter saves, Escape doesn't. */
function RenameField({ controller, onDone }: { controller: Controller; onDone: () => void }) {
  const apply = useApp((s) => s.apply);
  const [name, setName] = useState(controller.name);
  const show = useApp((s) => s.snapshot?.show);
  const { problems, warnings } = show ? checkDraft({ ...controllerDraft(controller), name }, controller, show) : { problems: {}, warnings: {} };
  const note = problems.name ?? warnings.name;
  // An empty name keeps the field open, saying why; Escape leaves the name as it was.
  const commit = () => {
    if (problems.name) return;
    void apply(controllerEdits(controller.id, { ...controllerDraft(controller), name }));
    onDone();
  };
  return (
    <span className="flex min-w-0 flex-col">
      <input
        autoFocus
        aria-label={`Name of ${controller.name}`}
        aria-invalid={!!problems.name}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          if (e.key === "Escape") {
            e.stopPropagation();
            onDone();
          }
        }}
        className="rounded-md border border-neutral-300 bg-white px-2 py-0.5 font-semibold dark:border-neutral-700 dark:bg-neutral-950"
      />
      {note && <span className={`text-xs font-normal ${problems.name ? "text-red-600 dark:text-red-400" : "text-amber-700 dark:text-amber-400"}`}>{note}</span>}
    </span>
  );
}

/** One controller: its ports as rows, each with its chain of props in wiring order. */
export function ControllerCard({ controller, data }: { controller: Controller; data: WiringData }) {
  const apply = useApp((s) => s.apply);
  const edit = useApp((s) => s.edit);
  const collapsed = useWiring((s) => s.collapsed.includes(controller.id));
  const [editing, setEditing] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const pixels = controller.ports.reduce((sum, p) => sum + portPixels(p, data.nodes), 0);
  const options = capacityOptions(data.show, controller, data.cpp);
  const over = controller.ports.filter((p) => portCapacity(p, data.nodes, options).level === "over").length;
  const kind = ADAPTERS[controller.adapter];
  const wired = controller.ports.reduce((sum, p) => sum + p.slots.length, 0);
  const sacn = controller.protocol.type === "sacn" ? controller.protocol : null;
  return (
    <section className="rounded-lg border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900" aria-label={controller.name}>
      <header className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2">
        <h2 className="min-w-0 font-semibold">
          {renaming ? (
            <RenameField controller={controller} onDone={() => setRenaming(false)} />
          ) : (
            <button
              type="button"
              aria-expanded={!collapsed}
              title="Click to fold away, double-click to rename"
              onClick={() => useWiring.getState().toggleCollapsed(controller.id)}
              onDoubleClick={() => setRenaming(true)}
              className="flex min-w-0 items-center gap-1 hover:text-accent-600 dark:hover:text-accent-400"
            >
              {collapsed ? <ChevronRight size={16} aria-hidden /> : <ChevronDown size={16} aria-hidden />}
              <span className="truncate">{controller.name}</span>
            </button>
          )}
        </h2>
        <span className="text-sm text-neutral-500">{controller.address}</span>
        <span className="rounded bg-neutral-100 px-1.5 py-0.5 text-xs dark:bg-neutral-800">
          {kind && `${kind} · `}
          {sacn ? `sACN${sacn.startUniverse !== null ? ` · from universe ${sacn.startUniverse}` : ""}` : "DDP"}
        </span>
        <span className="text-xs text-neutral-500 tabular-nums">
          {plural(controller.ports.length, "port")} · {thousands(pixels)} px
          {over > 0 && <span className="ml-1 font-medium text-red-600 dark:text-red-400">· {plural(over, "port")} over the limit</span>}
        </span>
        <div className="ml-auto flex gap-1">
          <Button
            variant="ghost"
            aria-label={`Edit ${controller.name}`}
            title="Change the name, address, or protocol"
            aria-expanded={editing}
            onClick={() => setEditing(!editing)}
          >
            <Pencil size={14} /> Edit
          </Button>
          <Button variant="ghost" aria-label={`Add a port to ${controller.name}`} title="Add a port" onClick={() => void apply((show) => addPortEdits(show, controller.id))}>
            <Plus size={14} /> Port
          </Button>
          <Button
            variant="danger"
            aria-label={`Delete ${controller.name}`}
            title={wired > 0 ? `Delete this controller (unwires ${plural(wired, "prop")}; Undo brings it back)` : "Delete this controller"}
            onClick={async () => {
              const revision = await edit([{ type: "removeController", id: controller.id }]);
              toastWithUndo(wired > 0 ? `Deleted ${controller.name} and unwired ${plural(wired, "prop")}` : `Deleted ${controller.name}`, revision);
            }}
          >
            <Trash2 size={16} />
          </Button>
        </div>
      </header>
      {editing && <ControllerEditForm controller={controller} onDone={() => setEditing(false)} />}
      {!collapsed &&
        (controller.ports.length === 0 ? (
          <p className="border-t border-neutral-200 px-3 py-3 text-sm text-neutral-500 dark:border-neutral-800">
            No ports yet. Add one with + Port.
          </p>
        ) : (
          <ul>
            {controller.ports.map((port, i) => (
              <PortRow key={`${port.number}-${i}`} controller={controller} port={port} at={i} data={data} />
            ))}
          </ul>
        ))}
    </section>
  );
}
