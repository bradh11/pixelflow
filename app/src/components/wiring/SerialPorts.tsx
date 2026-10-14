import { useEffect, useState } from "react";
import type { SerialPortChoice } from "../../api/types";
import { useApp } from "../../state/store";

/** The serial ports a USB DMX interface could be on, while `wanted` (listing them opens none). */
export function useSerialPorts(wanted: boolean): SerialPortChoice[] {
  const backend = useApp((s) => s.backend);
  const [ports, setPorts] = useState<SerialPortChoice[]>([]);
  useEffect(() => {
    if (!wanted || !backend) return;
    let live = true;
    const take = (found: SerialPortChoice[]) => {
      if (live) setPorts(found);
    };
    backend.listSerialPorts().then(take, () => take([]));
    return () => {
      live = false;
    };
  }, [wanted, backend]);
  return ports;
}

/** The ports to pick from for an address field (`list={id}`): each by its name, read as what the device is. */
export function SerialPortList({ id, ports }: { id: string; ports: SerialPortChoice[] }) {
  return (
    <datalist id={id}>
      {ports.map((p) => (
        <option key={p.name} value={p.name} label={p.label} />
      ))}
    </datalist>
  );
}

/** What the device on `address` is, when it's one of the ports listed. */
export function portLabel(ports: SerialPortChoice[], address: string): string | undefined {
  const name = address.trim();
  return ports.find((p) => p.name === name)?.label;
}
