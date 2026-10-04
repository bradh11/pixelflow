import { Plus, Trash2, X } from "lucide-react";
import { useState } from "react";
import type { Controller, Port, Prop } from "../api/types";
import { Button, Card, EmptyState, Field, Input, PageHeader, Select } from "../components/ui";
import { thousands } from "../lib/format";
import { newController, newPort, uniqueName } from "../lib/shows";
import { useApp } from "../state/store";

function AddControllerForm({ onDone }: { onDone: () => void }) {
  const snapshot = useApp((s) => s.snapshot);
  const apply = useApp((s) => s.apply);
  const [name, setName] = useState(() => uniqueName("Controller", snapshot?.show.controllers.map((c) => c.name) ?? []));
  const [address, setAddress] = useState("");
  const [protocol, setProtocol] = useState<"ddp" | "sacn">("ddp");
  const [ports, setPorts] = useState(4);
  const submit = async () => {
    if (!name.trim() || !address.trim()) return;
    const ok = await apply([{ type: "addController", controller: newController(name.trim(), address.trim(), protocol, ports) }]);
    if (ok) onDone();
  };
  return (
    <Card className="mb-6">
      <form
        className="grid grid-cols-2 gap-3 md:grid-cols-5"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="IP address">
          <Input value={address} placeholder="192.168.1.50" onChange={(e) => setAddress(e.target.value)} />
        </Field>
        <Field label="Protocol">
          <Select value={protocol} onChange={(e) => setProtocol(e.target.value as "ddp" | "sacn")}>
            <option value="ddp">DDP</option>
            <option value="sacn">sACN (E1.31)</option>
          </Select>
        </Field>
        <Field label="Ports">
          <Input type="number" min={1} max={48} value={ports} onChange={(e) => setPorts(Number(e.target.value) || 1)} />
        </Field>
        <div className="flex items-end gap-2">
          <Button type="submit" variant="primary" disabled={!name.trim() || !address.trim()}>
            Add
          </Button>
          <Button variant="ghost" onClick={onDone}>
            Cancel
          </Button>
        </div>
      </form>
    </Card>
  );
}

function PortRow({ controller, port, props, pixels }: { controller: Controller; port: Port; props: Prop[]; pixels: Map<string, number> }) {
  const apply = useApp((s) => s.apply);
  const update = (next: Port) =>
    apply([
      {
        type: "updateController",
        controller: { ...controller, ports: controller.ports.map((p) => (p.number === port.number ? next : p)) },
      },
    ]);
  const used = port.slots.reduce((sum, s) => sum + s.nullPixels + (pixels.get(s.prop) ?? 0), 0);
  const name = (id: string) => props.find((p) => p.id === id)?.name ?? "Missing prop";
  return (
    <li className="flex flex-wrap items-center gap-2 border-t border-neutral-200 py-2 dark:border-neutral-800">
      <span className="w-16 text-sm font-medium">Port {port.number}</span>
      {port.slots.map((slot, i) => (
        <span key={i} className="flex items-center gap-1 rounded-full bg-neutral-100 py-0.5 pr-1 pl-3 text-sm dark:bg-neutral-800">
          {name(slot.prop)}
          <button
            type="button"
            aria-label={`Unwire ${name(slot.prop)} from port ${port.number}`}
            onClick={() => update({ ...port, slots: port.slots.filter((_, j) => j !== i) })}
            className="rounded-full p-0.5 hover:bg-neutral-200 dark:hover:bg-neutral-700"
          >
            <X size={12} />
          </button>
        </span>
      ))}
      <Select
        aria-label={`Add a prop to port ${port.number}`}
        value=""
        onChange={(e) => {
          if (!e.target.value) return;
          void update({
            ...port,
            slots: [
              ...port.slots,
              { prop: e.target.value, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null },
            ],
          });
        }}
      >
        <option value="">+ Add prop…</option>
        {props.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </Select>
      <span className="ml-auto text-xs text-neutral-500 tabular-nums">{thousands(used)} px</span>
    </li>
  );
}

function ControllerCard({ controller }: { controller: Controller }) {
  const snapshot = useApp((s) => s.snapshot);
  const apply = useApp((s) => s.apply);
  if (!snapshot) return null;
  const pixels = new Map(snapshot.channelMap.props.map((p) => [p.prop, p.nodes]));
  const addPort = () =>
    apply([
      {
        type: "updateController",
        controller: { ...controller, ports: [...controller.ports, newPort(controller.ports.length + 1)] },
      },
    ]);
  return (
    <Card>
      <div className="mb-2 flex items-center gap-3">
        <h2 className="font-semibold">{controller.name}</h2>
        <span className="text-sm text-neutral-500">{controller.address}</span>
        <span className="rounded bg-neutral-100 px-1.5 py-0.5 text-xs dark:bg-neutral-800">
          {controller.protocol.type === "ddp" ? "DDP" : "sACN"}
        </span>
        <div className="ml-auto flex gap-1">
          <Button variant="ghost" onClick={addPort}>
            <Plus size={14} /> Port
          </Button>
          <Button variant="danger" aria-label={`Delete ${controller.name}`} onClick={() => apply([{ type: "removeController", id: controller.id }])}>
            <Trash2 size={16} />
          </Button>
        </div>
      </div>
      <ul>
        {controller.ports.map((port) => (
          <PortRow key={port.number} controller={controller} port={port} props={snapshot.show.props} pixels={pixels} />
        ))}
      </ul>
    </Card>
  );
}

/** Controllers, their ports, and which props each port drives (in wiring order). */
export function WiringScreen() {
  const snapshot = useApp((s) => s.snapshot);
  const [adding, setAdding] = useState(false);
  if (!snapshot) return null;
  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader
        title="Wiring"
        description="Add your controllers and choose which props each port drives. Channels and universes are assigned automatically."
        actions={
          !adding && (
            <Button variant="primary" onClick={() => setAdding(true)}>
              <Plus size={16} /> Add controller
            </Button>
          )
        }
      />
      {adding && <AddControllerForm onDone={() => setAdding(false)} />}
      {snapshot.show.controllers.length === 0 && !adding ? (
        <EmptyState title="No controllers yet">Add a controller, then wire props to its ports.</EmptyState>
      ) : (
        <div className="flex flex-col gap-4">
          {snapshot.show.controllers.map((c) => (
            <ControllerCard key={c.id} controller={c} />
          ))}
        </div>
      )}
    </div>
  );
}
