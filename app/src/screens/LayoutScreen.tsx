import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Prop } from "../api/types";
import { Button, EmptyState, Input, PageHeader, Select } from "../components/ui";
import { thousands } from "../lib/format";
import { PROP_KINDS, type PropKind, newProp, shapeLabel } from "../lib/shows";
import { useApp } from "../state/store";

function PropRow({ prop, pixels }: { prop: Prop; pixels: number }) {
  const apply = useApp((s) => s.apply);
  const [name, setName] = useState(prop.name);
  const commit = () => {
    const trimmed = name.trim();
    if (trimmed && trimmed !== prop.name) void apply([{ type: "updateProp", prop: { ...prop, name: trimmed } }]);
    else setName(prop.name);
  };
  return (
    <tr className="border-t border-neutral-200 dark:border-neutral-800">
      <td className="py-1.5 pr-3">
        <Input
          aria-label={`Name of ${prop.name}`}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
          className="w-full border-transparent bg-transparent hover:border-neutral-300 focus:border-neutral-400 dark:border-transparent dark:bg-transparent dark:hover:border-neutral-700 dark:focus:border-neutral-600"
        />
      </td>
      <td className="px-3 text-sm text-neutral-500">{shapeLabel(prop.shape)}</td>
      <td className="px-3 text-right text-sm tabular-nums">{thousands(pixels)}</td>
      <td className="px-3 text-sm text-neutral-500">{prop.colorOrder}</td>
      <td className="pl-3 text-right">
        <Button variant="danger" aria-label={`Delete ${prop.name}`} onClick={() => apply([{ type: "removeProp", id: prop.id }])}>
          <Trash2 size={16} />
        </Button>
      </td>
    </tr>
  );
}

/** Props in the show. The 2D/3D layout viewport arrives with the layout editor. */
export function LayoutScreen() {
  const snapshot = useApp((s) => s.snapshot);
  const apply = useApp((s) => s.apply);
  const [kind, setKind] = useState<PropKind>("arch");
  if (!snapshot) return null;
  const pixels = new Map(snapshot.channelMap.props.map((p) => [p.prop, p.nodes]));
  const add = () => apply([{ type: "addProp", prop: newProp(kind, snapshot.show) }]);
  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader
        title="Props"
        description="Everything in your display. Add props here, then wire them to controller ports."
        actions={
          <>
            <Select aria-label="Prop type" value={kind} onChange={(e) => setKind(e.target.value as PropKind)}>
              {PROP_KINDS.map((k) => (
                <option key={k.kind} value={k.kind}>
                  {k.label}
                </option>
              ))}
            </Select>
            <Button variant="primary" onClick={add}>
              <Plus size={16} /> Add prop
            </Button>
          </>
        }
      />
      {snapshot.show.props.length === 0 ? (
        <EmptyState title="No props yet">Pick a prop type and choose Add prop to start building your display.</EmptyState>
      ) : (
        <table className="w-full">
          <thead>
            <tr className="text-left text-xs tracking-wide text-neutral-500 uppercase">
              <th className="pb-2 font-medium">Name</th>
              <th className="px-3 pb-2 font-medium">Type</th>
              <th className="px-3 pb-2 text-right font-medium">Pixels</th>
              <th className="px-3 pb-2 font-medium">Color order</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {snapshot.show.props.map((prop) => (
              <PropRow key={`${prop.id}:${prop.name}`} prop={prop} pixels={pixels.get(prop.id) ?? 0} />
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
