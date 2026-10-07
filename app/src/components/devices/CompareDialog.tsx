import { CheckCircle2, Loader2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { ColorOrder, DeviceComparison, UseProps } from "../../api/types";
import { plural } from "../../lib/format";
import { useApp } from "../../state/store";
import { toast } from "../../state/toast";
import { Button } from "../ui";
import { DeviceDialog, PropPicker } from "./DeviceDialog";
import { SetupChanges } from "./SetupChanges";

/** The pixel count at the start of a row's value ("30 pixels, BGR" → 30). */
const pixelsOf = (text: string) => Number.parseInt(text.replace(/,/g, ""), 10) || 0;

/** The color order at the end of a new string's value ("30 pixels, BGR" → "BGR"). */
const orderOf = (text: string) => (text.match(/, (RGBW|GRBW|RGB|RBG|GRB|GBR|BRG|BGR)$/)?.[1] as ColorOrder | undefined) ?? null;

/**
 * "Compare with this device": reads the controller and lists where it differs from the show.
 * The user picks which differences to take into the show (one undo step). The controller itself
 * isn't changed.
 */
export function CompareDialog({ address, onClose }: { address: string; onClose: () => void }) {
  const backend = useApp((s) => s.backend);
  const show = useApp((s) => s.snapshot?.show);
  const run = useApp((s) => s.run);
  const [comparison, setComparison] = useState<DeviceComparison | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [useProps, setUseProps] = useState<UseProps>({});
  const [busy, setBusy] = useState(false);
  const close = useCallback(() => onClose(), [onClose]);

  useEffect(() => {
    if (!backend) return;
    let current = true;
    backend.compareDevice(address).then(
      (c) => {
        if (!current) return;
        setComparison(c);
        // Differences that turn nothing off are picked to start with.
        setPicked(new Set(c.changes.filter((ch) => ch.canTake && !ch.warning).map((ch) => ch.id)));
      },
      (e) => current && setError(errorMessage(e)),
    );
    return () => {
      current = false;
    };
  }, [backend, address]);

  const toggle = (id: string) =>
    setPicked((now) => {
      const next = new Set(now);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const take = async () => {
    if (!comparison || busy || picked.size === 0) return;
    setBusy(true);
    const ids = comparison.changes.filter((c) => picked.has(c.id)).map((c) => c.id);
    const chosen = Object.fromEntries(Object.entries(useProps).filter(([id, prop]) => prop && picked.has(id)));
    const ok = await run((b) => b.takeFromDevice(address, ids, chosen));
    setBusy(false);
    if (ok) {
      toast(`Took ${plural(ids.length, "change")} from ${comparison.device.name} into your show.`, { label: "Undo", run: () => useApp.getState().undo() });
      onClose();
    }
  };

  const name = comparison?.device.name ?? address;
  return (
    <DeviceDialog
      title={`Compare with ${name}`}
      subtitle={comparison ? `Your show's ${comparison.controllerName} → what ${name} has now. Nothing on the controller is changed.` : undefined}
      onClose={close}
      footer={
        <>
          <Button data-autofocus onClick={onClose}>
            {comparison && comparison.changes.length === 0 ? "Close" : "Cancel"}
          </Button>
          {comparison && comparison.changes.length > 0 && (
            <Button variant="primary" onClick={take} disabled={busy || picked.size === 0}>
              {busy && <Loader2 size={14} className="animate-spin" aria-hidden />}
              Take {plural(picked.size, "change")} into my show
            </Button>
          )}
        </>
      }
    >
      {!comparison && !error && (
        <p className="flex items-center gap-2 text-neutral-500">
          <Loader2 size={16} className="animate-spin" aria-hidden /> Reading {address}…
        </p>
      )}
      {error && (
        <p role="alert" className="text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      {comparison && comparison.changes.length === 0 && (
        <p className="flex items-center gap-2">
          <CheckCircle2 size={16} className="text-emerald-600" aria-hidden /> Your show and {name} match.
        </p>
      )}
      {comparison && comparison.changes.length > 0 && show && (
        <div className="flex flex-col gap-3">
          <p className="text-neutral-600 dark:text-neutral-300">Pick the differences to take into your show. Undo puts them all back at once.</p>
          <SetupChanges
            label="Differences"
            changes={comparison.changes}
            picked={picked}
            onToggle={toggle}
            extra={(change) =>
              change.kind === "stringAdded" ? (
                <PropPicker
                  show={show}
                  label={`Prop for ${change.subject} on port ${change.port}`}
                  pixels={pixelsOf(change.after)}
                  order={orderOf(change.after)}
                  value={useProps[change.id] ?? ""}
                  onChange={(id) => setUseProps((now) => ({ ...now, [change.id]: id }))}
                />
              ) : null
            }
          />
        </div>
      )}
      {comparison?.notes.map((note) => (
        <p key={note} className="mt-2 text-xs text-neutral-500">
          {note}
        </p>
      ))}
    </DeviceDialog>
  );
}
