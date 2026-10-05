import { AlertTriangle, Loader2, Plus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { Controller, DeviceDetails, DeviceInput } from "../api/types";
import { thousands } from "../lib/format";
import { useApp } from "../state/store";
import { FppPanel } from "./FppPanel";
import { Button } from "./ui";

const NO_CONTROLLERS: Controller[] = [];

function describeInput(input: DeviceInput): string {
  switch (input.type) {
    case "ddp":
      return "DDP";
    case "sacn":
      return `sACN, universes ${input.startUniverse}–${input.startUniverse + input.universeCount - 1}`;
    case "unsupported":
      return `${input.description} (not supported yet; PixelFlow will send DDP)`;
  }
}

/** Reads a device's configuration and shows exactly what importing it would add. */
export function ImportDialog({
  address,
  onClose,
  onImported,
}: {
  address: string;
  onClose: () => void;
  onImported: (message: string) => void;
}) {
  const backend = useApp((s) => s.backend);
  const run = useApp((s) => s.run);
  const [details, setDetails] = useState<DeviceDetails | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const controllers = useApp((s) => s.snapshot?.show.controllers ?? NO_CONTROLLERS);

  useEffect(() => {
    let cancelled = false;
    backend?.inspectDevice(address).then(
      (d) => !cancelled && setDetails(d),
      (e) => !cancelled && setError(errorMessage(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [backend, address]);

  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const add = async () => {
    if (!details || busy) return;
    setBusy(true);
    const ok = await run((b) => b.importDevice(address));
    setBusy(false);
    if (ok) {
      const { plan } = details;
      onImported(
        `Added ${plan.controller.name}: ${plan.props.length} props on ${plan.controller.ports.length} ports. Undo with ⌘Z.`,
      );
      onClose();
    }
  };

  const addDestination = async (destination: string, name: string) => {
    if (busy) return;
    setBusy(true);
    const ok = await run((b) => b.importFppDestination(address, destination));
    setBusy(false);
    if (ok) onImported(`Added ${name}. Import it from its own row once it's online to add its strings.`);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-title"
        className="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="border-b border-neutral-200 p-5 dark:border-neutral-800">
          <h2 id="import-title" className="text-lg font-semibold">
            {details ? (details.device.kind === "fpp" ? details.device.name : `Import ${details.device.name}`) : `Reading ${address}…`}
          </h2>
          {details && (
            <p className="mt-1 text-sm text-neutral-500">
              {details.device.model} · {details.device.firmware} · {details.device.address}
            </p>
          )}
        </div>
        <div className="flex-1 overflow-auto p-5 text-sm">
          {!details && !error && (
            <p className="flex items-center gap-2 text-neutral-500">
              <Loader2 size={16} className="animate-spin" /> Reading the controller's configuration…
            </p>
          )}
          {error && (
            <p role="alert" className="text-red-600 dark:text-red-400">
              {error}
            </p>
          )}
          {details && (
            <div className="flex flex-col gap-4">
              {details.device.kind === "fpp" && <FppPanel address={details.device.address} />}
              <p>
                <span className="text-neutral-500">Receives:</span> {describeInput(details.config.input)}
              </p>
              {details.config.ports.length > 0 && (
                <table className="w-full">
                  <thead>
                    <tr className="text-left text-xs tracking-wide text-neutral-500 uppercase">
                      <th className="pb-1 font-medium">Port</th>
                      <th className="pb-1 font-medium">String</th>
                      <th className="pb-1 text-right font-medium">Pixels</th>
                      <th className="pb-1 pl-3 font-medium">Order</th>
                      <th className="pb-1 text-right font-medium">Nulls</th>
                      <th className="pb-1 pl-3 font-medium">Direction</th>
                      <th className="pb-1 text-right font-medium">Brightness</th>
                      <th className="pb-1 text-right font-medium">Gamma</th>
                    </tr>
                  </thead>
                  <tbody>
                    {details.config.ports.flatMap((port) =>
                      port.strings.map((s, i) => (
                        <tr key={`${port.number}-${i}`} className="border-t border-neutral-200 dark:border-neutral-800">
                          <td className="py-1">{i === 0 ? port.number : ""}</td>
                          <td>{s.name ?? `String ${i + 1}`}</td>
                          <td className="text-right tabular-nums">{thousands(s.pixels)}</td>
                          <td className="pl-3">{s.colorOrder}</td>
                          <td className="text-right tabular-nums">{s.nullPixels}</td>
                          <td className="pl-3">{s.reverse ? "Reversed" : "Forward"}</td>
                          <td className="text-right tabular-nums">{s.brightness}%</td>
                          <td className="text-right tabular-nums">{s.gamma}</td>
                        </tr>
                      )),
                    )}
                  </tbody>
                </table>
              )}
              {details.config.destinations.map((d) => {
                const name = d.description || d.address;
                const inShow = controllers.some((c) => c.address === d.address);
                return (
                  <div key={`${d.address}-${d.protocol}`} className="flex items-center justify-between gap-3">
                    <p>
                      Sends {thousands(d.channels)} channels by {d.protocol} to {name} ({d.address}).
                    </p>
                    {details.device.kind === "fpp" &&
                      (inShow ? (
                        <span className="shrink-0 text-neutral-500">In your show</span>
                      ) : (
                        <Button
                          aria-label={`Add ${name} to show`}
                          onClick={() => addDestination(d.address, name)}
                          disabled={busy}
                        >
                          <Plus size={14} /> Add to show
                        </Button>
                      ))}
                  </div>
                );
              })}
              {details.plan.alreadyInShow && (
                <p className="flex items-start gap-2 text-amber-700 dark:text-amber-400">
                  <AlertTriangle size={16} className="mt-0.5 shrink-0" />
                  A controller at this address is already in your show. Importing adds another copy.
                </p>
              )}
              {details.plan.notes.length > 0 && (
                <ul className="flex flex-col gap-1 text-amber-700 dark:text-amber-400">
                  {details.plan.notes.map((note) => (
                    <li key={note}>{note}</li>
                  ))}
                </ul>
              )}
              {details.plan.canImport && (
                <p className="text-neutral-500">
                  PixelFlow will add the controller with {details.plan.controller.ports.length} ports and one prop per
                  string. Shape and place the props on the Layout screen afterwards.
                </p>
              )}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-neutral-200 p-4 dark:border-neutral-800">
          {details?.device.kind === "fpp" && !details.plan.canImport ? (
            <Button ref={cancelRef} onClick={onClose}>
              Close
            </Button>
          ) : (
            <>
              <Button ref={cancelRef} onClick={onClose}>
                Cancel
              </Button>
              <Button variant="primary" onClick={add} disabled={!details?.plan.canImport || busy}>
                {details && !details.plan.canImport ? "Nothing to import" : "Add to show"}
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
