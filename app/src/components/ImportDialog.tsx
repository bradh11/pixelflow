import { AlertTriangle, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { DeviceDetails, DeviceInput, Show, UseProps } from "../api/types";
import { stringKey } from "../lib/deviceSetup";
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

/** What importing the device at `address` changed from `before` to `after`: the controller added
 * (or filled in) there, its new props, the props already in the show it wired, and its ports. */
function importedSummary(before: Show, after: Show, address: string): string {
  const oldProps = new Set(before.props.map((p) => p.id));
  const oldControllers = new Set(before.controllers.map((c) => c.id));
  const here = after.controllers.filter((c) => c.address === address);
  const controller = here.find((c) => !oldControllers.has(c.id)) ?? here[here.length - 1];
  if (!controller) return "Added the controller. Undo with ⌘Z.";
  const added = after.props.filter((p) => !oldProps.has(p.id)).length;
  const yours = controller.ports.flatMap((p) => p.slots).filter((s) => oldProps.has(s.prop)).length;
  return `Added ${controller.name}: ${plural(added, "prop")}${yours ? ` and ${yours} of yours` : ""} on ${plural(controller.ports.length, "port")}. Undo with ⌘Z.`;
}

/** A table cell's spacing; numbers are right-aligned. */
const CELL = "px-2 py-1.5 first:pl-0 last:pr-0";
const NUMBER = `${CELL} text-right tabular-nums`;

/** Reads a device's configuration and shows exactly what importing it would add. (An FPP has a
 * page of its own: see FppDevicePage.) When the controller is already in the show, `onCompare`
 * (Compare with this device) is the main way on, rather than adding another copy. */
export function ImportDialog({
  address,
  onClose,
  onImported,
  onCompare,
}: {
  address: string;
  onClose: () => void;
  onImported: (message: string) => void;
  onCompare?: () => void;
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
      (d) => {
        if (cancelled) return;
        setDetails(d);
        // "In your show" starts on the props the strings most likely are.
        setUseProps(Object.fromEntries(Object.entries(d.plan.suggested).map(([key, match]) => [key, match.prop])));
      },
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
    const before = useApp.getState().snapshot?.show;
    const ok = await run((b) => b.importDevice(address, chosen));
    setBusy(false);
    if (ok) {
      // The import reads the device again, so describe what it added, not what was reviewed.
      const after = useApp.getState().snapshot?.show;
      onImported(after && before ? importedSummary(before, after, address) : `Added ${details.plan.controller.name}.`);
      onClose();
    }
  };

  const mapping = (show?.props.length ?? 0) > 0 && details?.plan.canImport === true;
  const reused = Object.values(useProps).filter(Boolean).length;
  const compare = details?.plan.alreadyInShow && onCompare ? onCompare : null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-title"
        className="flex max-h-[85vh] w-full max-w-4xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
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
                <div className="overflow-x-auto">
                  <table className="w-full">
                    <thead>
                      <tr className="text-left text-xs tracking-wide whitespace-nowrap text-neutral-500 uppercase">
                        <th className={`${NUMBER} font-medium`}>Port</th>
                        <th className={`${CELL} font-medium`}>String</th>
                        <th className={`${NUMBER} font-medium`}>Pixels</th>
                        <th className={`${CELL} font-medium`}>Order</th>
                        <th className={`${NUMBER} font-medium`}>Nulls</th>
                        <th className={`${CELL} font-medium`}>Direction</th>
                        <th className={`${NUMBER} font-medium`}>Brightness</th>
                        <th className={`${NUMBER} font-medium`}>Gamma</th>
                        {mapping && <th className={`${CELL} font-medium`}>In your show</th>}
                      </tr>
                    </thead>
                    <tbody>
                      {details.config.ports.flatMap((port) =>
                        port.strings.map((s, i) => {
                          const key = stringKey(port.number, i);
                          return (
                            <tr key={key} className="border-t border-neutral-200 dark:border-neutral-800">
                              <td className={NUMBER}>{i === 0 ? port.number : ""}</td>
                              <td className={CELL}>{s.name ?? `String ${i + 1}`}</td>
                              <td className={NUMBER}>{thousands(s.pixels)}</td>
                              <td className={CELL}>{s.colorOrder}</td>
                              <td className={NUMBER}>{s.nullPixels}</td>
                              <td className={CELL}>{s.reverse ? "Reversed" : "Forward"}</td>
                              <td className={NUMBER}>{s.brightness}%</td>
                              <td className={NUMBER}>{s.gamma}</td>
                              {mapping && show && (
                                <td className={`${CELL} min-w-56`}>
                                  <PropPicker
                                    show={show}
                                    label={`Prop for port ${port.number} ${s.name ?? `string ${i + 1}`}`}
                                    pixels={s.pixels}
                                    order={s.colorOrder}
                                    value={useProps[key] ?? ""}
                                    suggestion={details.plan.suggested[key]}
                                    onChange={(id) => setUseProps((now) => ({ ...now, [key]: id }))}
                                  />
                                </td>
                              )}
                            </tr>
                          );
                        }),
                      )}
                    </tbody>
                  </table>
                </div>
              )}
              {details.plan.alreadyInShow && (
                <p className="flex items-start gap-2 text-amber-700 dark:text-amber-400">
                  <AlertTriangle size={16} className="mt-0.5 shrink-0" />
                  {compare
                    ? "A controller at this address is already in your show. Compare with this device to update it; Add another copy adds a second one."
                    : "A controller at this address is already in your show. Importing adds another copy."}
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
          {compare ? (
            <>
              <Button onClick={add} disabled={!details?.plan.canImport || busy}>
                Add another copy
              </Button>
              <Button variant="primary" onClick={compare}>
                Compare with this device
              </Button>
            </>
          ) : (
            <Button variant="primary" onClick={add} disabled={!details?.plan.canImport || busy}>
              {details && !details.plan.canImport ? "Nothing to import" : "Add to show"}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
