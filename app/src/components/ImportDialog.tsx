import { AlertTriangle, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { DeviceDetails, DeviceInput, UseProps } from "../api/types";
import { plural, thousands } from "../lib/format";
import { useApp } from "../state/store";
import { PropPicker } from "./devices/DeviceDialog";
import { Button } from "./ui";

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

/** Reads a device's configuration and shows exactly what importing it would add. (An FPP has a
 * page of its own: see FppDevicePage.) */
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
  const show = useApp((s) => s.snapshot?.show);
  const [details, setDetails] = useState<DeviceDetails | null>(null);
  /** Strings wired to props already in the show instead of new starter props, by string key. */
  const [useProps, setUseProps] = useState<UseProps>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const cancelRef = useRef<HTMLButtonElement>(null);

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
    const chosen = Object.fromEntries(Object.entries(useProps).filter(([, prop]) => prop));
    const ok = await run((b) => b.importDevice(address, chosen));
    setBusy(false);
    if (ok) {
      const { plan } = details;
      const reused = Object.keys(chosen).length;
      onImported(
        `Added ${plan.controller.name}: ${plan.props.length - reused} props${reused ? ` and ${reused} of yours` : ""} on ${plan.controller.ports.length} ports. Undo with ⌘Z.`,
      );
      onClose();
    }
  };

  const mapping = (show?.props.length ?? 0) > 0 && details?.plan.canImport === true;
  const reused = Object.values(useProps).filter(Boolean).length;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-title"
        className="flex max-h-[85vh] w-full max-w-3xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="border-b border-neutral-200 p-5 dark:border-neutral-800">
          <h2 id="import-title" className="text-lg font-semibold">
            {details ? `Import ${details.device.name}` : `Reading ${address}…`}
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
                      {mapping && <th className="pb-1 pl-3 font-medium">In your show</th>}
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
                          {mapping && show && (
                            <td className="pl-3">
                              <PropPicker
                                show={show}
                                label={`Prop for port ${port.number} ${s.name ?? `string ${i + 1}`}`}
                                pixels={s.pixels}
                                order={s.colorOrder}
                                value={useProps[`port${port.number}/string${i + 1}`] ?? ""}
                                onChange={(id) => setUseProps((now) => ({ ...now, [`port${port.number}/string${i + 1}`]: id }))}
                              />
                            </td>
                          )}
                        </tr>
                      )),
                    )}
                  </tbody>
                </table>
              )}
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
                  {reused === 0
                    ? `PixelFlow will add the controller with ${plural(details.plan.controller.ports.length, "port")} and one new prop per string. Shape and place the props on the Layout screen afterwards.`
                    : `PixelFlow will add the controller with ${plural(details.plan.controller.ports.length, "port")}, wire ${plural(reused, "prop")} you already have, and add ${plural(details.plan.props.length - reused, "new prop")}.`}
                  {mapping && reused === 0 && " To use props you already have (from an xLights import, say), pick them under In your show."}
                </p>
              )}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-neutral-200 p-4 dark:border-neutral-800">
          <Button ref={cancelRef} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" onClick={add} disabled={!details?.plan.canImport || busy}>
            {details && !details.plan.canImport ? "Nothing to import" : "Add to show"}
          </Button>
        </div>
      </div>
    </div>
  );
}
