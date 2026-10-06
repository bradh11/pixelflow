import { AlertTriangle, Plus } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { usePreviewProps } from "../components/layout/useLayoutData";
import { ControllerCard, type WiringData } from "../components/wiring/ControllerCard";
import { PropsPanel } from "../components/wiring/PropsPanel";
import { SlotSettings } from "../components/wiring/SlotSettings";
import { WiringPreview } from "../components/wiring/WiringPreview";
import { useDragEscape } from "../components/wiring/useWiringDrag";
import { Button, Card, EmptyState, Field, Input, PageHeader, Select } from "../components/ui";
import { CONTROLLER_KINDS, FALCON_PIXELS_AT_40FPS, controllerOfKind, kindById, kindPixelLimit } from "../lib/controllerKinds";
import { addressProblem } from "../lib/controllerEdit";
import { uniqueName } from "../lib/shows";
import { channelsPerPixel, findPort, nodeCounts, propWiring, resolveSlot, slotLabel, unwiredInLayoutOrder, wiringProblems } from "../lib/wiringMath";
import { useApp } from "../state/store";
import { useWiring } from "../state/wiring";

function AddControllerForm({ onDone }: { onDone: () => void }) {
  const snapshot = useApp((s) => s.snapshot);
  const apply = useApp((s) => s.apply);
  const [name, setName] = useState(() => uniqueName("Controller", snapshot?.show.controllers.map((c) => c.name) ?? []));
  const [address, setAddress] = useState("");
  const [protocol, setProtocol] = useState<"ddp" | "sacn">("ddp");
  const [kind, setKind] = useState("other");
  const [ports, setPorts] = useState(4);
  const known = kindById(kind);
  const limit = kindPixelLimit(known, ports);
  const problem = !name.trim() ? "Give the controller a name." : addressProblem(address);
  const submit = async () => {
    if (problem) return;
    const ok = await apply([{ type: "addController", controller: controllerOfKind(kind, name.trim(), address.trim(), protocol, ports) }]);
    if (ok) onDone();
  };
  return (
    <Card className="mb-6">
      <form
        className="grid grid-cols-2 gap-3 md:grid-cols-6"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="Controller type">
          <Select
            value={kind}
            onChange={(e) => {
              setKind(e.target.value);
              const k = kindById(e.target.value);
              if (k.ports) setPorts(k.ports);
            }}
          >
            {CONTROLLER_KINDS.map((k) => (
              <option key={k.id} value={k.id}>
                {k.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="IP address">
          <Input value={address} placeholder="e.g. 192.168.1.50" onChange={(e) => setAddress(e.target.value)} />
        </Field>
        <Field label="Protocol">
          <Select value={protocol} onChange={(e) => setProtocol(e.target.value as "ddp" | "sacn")}>
            <option value="ddp">DDP</option>
            <option value="sacn">sACN (E1.31)</option>
          </Select>
        </Field>
        <Field label="Ports">
          <Input type="number" min={1} max={256} value={ports} onChange={(e) => setPorts(Math.max(1, Math.min(256, Math.floor(Number(e.target.value)) || 1)))} />
        </Field>
        <div className="flex items-end gap-2">
          <Button type="submit" variant="primary" disabled={problem !== null} title={problem ?? undefined}>
            Add
          </Button>
          <Button variant="ghost" onClick={onDone}>
            Cancel
          </Button>
        </div>
        {problem && <p className="col-span-full text-xs text-neutral-500">{problem}</p>}
        {limit !== null && (
          <p className="col-span-full text-xs text-neutral-500">
            With {ports} ports in use, each drives up to {limit.toLocaleString("en-US")} pixels
            {known.adapter === "falcon" && ` (about ${FALCON_PIXELS_AT_40FPS} at 40 fps)`}; the Wiring screen warns when a port gets close.
            {known.ports !== null && ports !== known.ports && ` The ${known.label} has ${known.ports} on the board; more come from expansion boards or smart receivers.`}
          </p>
        )}
      </form>
    </Card>
  );
}

/** The chip being dragged, following the pointer. */
function DragGhost({ data }: { data: WiringData }) {
  const drag = useWiring((s) => s.drag);
  if (!drag) return null;
  const item = drag.item;
  const prop = data.propById.get(item.prop);
  const slot = item.kind === "slot" ? item.from : null;
  const name = prop?.name ?? "Missing prop";
  const hint = drag.over?.kind === "props" ? (item.kind === "slot" ? "Unwire" : null) : drag.over ? `Port ${drag.over.port}` : null;
  return (
    <div
      aria-hidden
      className="pointer-events-none fixed z-50 rounded-full bg-accent-600 px-2.5 py-0.5 text-xs font-medium text-white shadow-lg"
      style={{ left: drag.x + 12, top: drag.y + 8 }}
    >
      {slot ? slotLabel(name, slot) : name}
      {hint && <span className="ml-1.5 font-normal opacity-80">→ {hint}</span>}
    </div>
  );
}

/** Controllers, their ports, and which props each port drives (in wiring order), wired by dragging. */
export function WiringScreen() {
  const snapshot = useApp((s) => s.snapshot);
  const selected = useWiring((s) => s.selected);
  const props = snapshot?.show.props;
  // Pixel positions only change with the props themselves, not with wiring edits.
  const layoutKey = useMemo(() => (props ? JSON.stringify(props) : ""), [props]);
  const preview = usePreviewProps(layoutKey);
  const [adding, setAdding] = useState(false);
  useDragEscape();

  const data = useMemo<WiringData | null>(() => {
    if (!snapshot) return null;
    const nodes = nodeCounts(snapshot.channelMap);
    const wiring = propWiring(snapshot.show, nodes);
    return {
      show: snapshot.show,
      nodes,
      cpp: channelsPerPixel(snapshot.channelMap),
      propById: new Map(snapshot.show.props.map((p) => [p.id, p])),
      wiring,
      channelMap: snapshot.channelMap,
      unwired: unwiredInLayoutOrder(snapshot.show, preview.props, wiring),
    };
  }, [snapshot, preview]);

  // The open settings follow their slot (by prop and pixels) wherever it is on its port now, and
  // close when it's gone: undone, unwired, or moved away. Checked against the selection as it is
  // when this runs, which may be newer than this render's.
  const show = data?.show;
  const port = show && selected ? findPort(show, selected) : null;
  const index = port && selected ? resolveSlot(port, selected) : null;
  const shown = selected && index !== null ? { ...selected, index } : null;
  useEffect(() => {
    const current = useWiring.getState().selected;
    if (!show || !current) return;
    const p = findPort(show, current);
    const i = p ? resolveSlot(p, current) : null;
    if (i === null) useWiring.setState({ selected: null });
    else if (i !== current.index) useWiring.setState({ selected: { ...current, index: i } });
  }, [show, selected]);

  if (!snapshot || !data) return null;
  const problems = wiringProblems(data.show, data.nodes, data.cpp);
  const controllers = data.show.controllers;

  return (
    <div className="mx-auto max-w-[110rem]">
      <PageHeader
        title="Wiring"
        description="Drag each prop onto the controller port it's plugged into, in the order the wire reaches them. Channels and universes are assigned automatically."
        actions={
          !adding && (
            <Button variant="primary" onClick={() => setAdding(true)}>
              <Plus size={16} /> Add controller
            </Button>
          )
        }
      />
      {adding && <AddControllerForm onDone={() => setAdding(false)} />}
      <p id="wiring-chip-help" className="sr-only">
        Drag to another place or port, or onto the props list to unwire. Arrow keys move between props; Option or Alt with the up
        and down arrows moves this prop earlier or later on its port; Delete unwires it; Enter opens its settings.
      </p>
      <div className="grid items-start gap-4 lg:grid-cols-[16rem_minmax(0,1fr)] xl:grid-cols-[16rem_minmax(0,1fr)_21rem]">
        <PropsPanel props={data.show.props} wiring={data.wiring} />
        <div className="flex min-w-0 flex-col gap-4">
          {problems.length > 0 && (
            <div role="region" aria-label="Wiring problems" className="rounded-lg border border-red-300 bg-red-50 p-3 text-sm dark:border-red-900 dark:bg-red-950/40">
              <h2 className="mb-1 flex items-center gap-1.5 font-medium text-red-700 dark:text-red-300">
                <AlertTriangle size={14} aria-hidden /> {problems.length === 1 ? "1 thing to fix" : `${problems.length} things to fix`}
              </h2>
              <ul className="flex flex-col gap-1">
                {problems.map((p, i) => (
                  <li key={i}>
                    <span className="text-neutral-800 dark:text-neutral-200">{p.message}</span> <span className="text-neutral-500">{p.fix}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {controllers.length === 0 && !adding ? (
            <EmptyState title="No controllers yet">Add a controller (or import one on the Devices screen), then drag props onto its ports.</EmptyState>
          ) : (
            controllers.map((c) => <ControllerCard key={c.id} controller={c} data={data} />)
          )}
        </div>
        <div className="flex min-w-0 flex-col gap-4 lg:col-span-2 xl:sticky xl:top-0 xl:col-span-1">
          {shown && <SlotSettings selected={shown} data={data} />}
          <WiringPreview show={data.show} props={preview.props} />
        </div>
      </div>
      <DragGhost data={data} />
    </div>
  );
}
