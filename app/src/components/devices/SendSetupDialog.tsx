import { AlertTriangle, CheckCircle2, Loader2, RotateCcw, Send, XCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { RestoreReport, SendPlan, SendReport } from "../../api/types";
import { plural } from "../../lib/format";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { DeviceDialog } from "./DeviceDialog";
import { SetupChanges } from "./SetupChanges";

function Outcome({ report }: { report: SendReport }) {
  const tone =
    report.status === "sent"
      ? { icon: <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-emerald-600" aria-hidden />, text: "" }
      : report.status === "failed"
        ? { icon: <XCircle size={16} className="mt-0.5 shrink-0 text-red-600 dark:text-red-400" aria-hidden />, text: "text-red-700 dark:text-red-400" }
        : { icon: <AlertTriangle size={16} className="mt-0.5 shrink-0 text-amber-600" aria-hidden />, text: "text-amber-800 dark:text-amber-300" };
  return (
    <p role={report.status === "sent" ? "status" : "alert"} className={`flex items-start gap-2 ${tone.text}`}>
      {tone.icon} {report.message}
    </p>
  );
}

type PutBack = { kind: "idle" } | { kind: "running" } | { kind: "done"; result: RestoreReport };

/**
 * "Send setup to this device…": shows, port by port, what sending the show's setup would change
 * on the controller, and sends it only when the user clicks Send. A copy of the controller's
 * setup is taken first and kept until dismissed: Put back sends it again, after any send, after
 * reopening this dialog, or to try again after a Put back that didn't work.
 */
export function SendSetupDialog({ address, onClose }: { address: string; onClose: () => void }) {
  const backend = useApp((s) => s.backend);
  const [plan, setPlan] = useState<SendPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [report, setReport] = useState<SendReport | null>(null);
  const [putBack, setPutBack] = useState<PutBack>({ kind: "idle" });
  /** A copy is kept (from an earlier send, or this one). */
  const [copy, setCopy] = useState(false);
  const close = useCallback(() => onClose(), [onClose]);

  useEffect(() => {
    if (!backend) return;
    let current = true;
    backend.planDeviceSetup(address).then(
      (p) => {
        if (!current) return;
        setPlan(p);
        setCopy(p.restorePoint !== null);
      },
      (e) => current && setError(errorMessage(e)),
    );
    return () => {
      current = false;
    };
  }, [backend, address]);

  const send = async () => {
    if (!backend || !plan || sending || report) return;
    setSending(true);
    try {
      const sent = await backend.sendDeviceSetup(
        address,
        plan.changes.map((c) => c.id),
      );
      setReport(sent);
      if (sent.canRestore) setCopy(true);
    } catch (e) {
      setReport({ status: "refused", message: errorMessage(e), mismatches: [], canRestore: false });
    }
    setSending(false);
    setPutBack({ kind: "idle" });
  };

  const restore = async () => {
    if (!backend || putBack.kind === "running") return;
    setPutBack({ kind: "running" });
    try {
      setPutBack({ kind: "done", result: await backend.restoreDeviceSetup(address) });
    } catch (e) {
      setPutBack({ kind: "done", result: { restored: false, message: errorMessage(e) } });
    }
  };

  const forget = async () => {
    if (!backend) return;
    await backend.forgetDeviceSetupCopy(address);
    setCopy(false);
  };

  const name = plan?.device.name ?? address;
  const warnings = plan?.changes.filter((c) => c.warning).length ?? 0;
  const busy = sending || putBack.kind === "running";
  const putBackDone = putBack.kind === "done" && putBack.result.restored;
  const offerPutBack = copy && !putBackDone;
  const taken = plan?.restorePoint ? new Date(plan.restorePoint.takenAtMs).toLocaleString() : null;

  return (
    <DeviceDialog
      title={`Send setup to ${name}`}
      subtitle={plan ? `What ${name} has now → what your show's ${plan.controllerName} needs. Only its outputs change; network settings are never touched.` : undefined}
      busy={busy}
      onClose={close}
      footer={
        <>
          {offerPutBack && (
            <Button onClick={restore} disabled={busy}>
              {putBack.kind === "running" ? <Loader2 size={14} className="animate-spin" aria-hidden /> : <RotateCcw size={14} aria-hidden />} Put back the previous setup
            </Button>
          )}
          {report ? (
            <Button variant="primary" onClick={onClose} disabled={busy}>
              Close
            </Button>
          ) : (
            <>
              <Button data-autofocus onClick={onClose} disabled={busy}>
                Cancel
              </Button>
              {plan?.canSend && (
                <Button variant="primary" onClick={send} disabled={busy}>
                  <Send size={14} aria-hidden /> Send to {name}
                </Button>
              )}
            </>
          )}
        </>
      }
    >
      {!plan && !error && (
        <p className="flex items-center gap-2 text-neutral-500">
          <Loader2 size={16} className="animate-spin" aria-hidden /> Reading {address}'s setup…
        </p>
      )}
      {error && (
        <p role="alert" className="text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      {plan && !report && !sending && (
        <div className="flex flex-col gap-3">
          {copy && taken && (
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-md border border-neutral-200 p-2 dark:border-neutral-800">
              <span>
                A copy of {plan.restorePoint!.deviceName}'s setup from {taken}, before an earlier send, is kept.
              </span>
              <Button variant="ghost" className="px-2 py-0.5 text-xs" onClick={forget} disabled={busy}>
                Forget this copy
              </Button>
            </p>
          )}
          {plan.busy && (
            <p className="flex items-start gap-2 rounded-md border border-amber-300 bg-amber-50 p-2 text-amber-800 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
              <AlertTriangle size={16} className="mt-0.5 shrink-0" aria-hidden /> {plan.busy}
            </p>
          )}
          {plan.problems.length > 0 && (
            <div role="alert" className="flex flex-col gap-1 rounded-md border border-red-300 p-2 text-red-700 dark:border-red-900 dark:text-red-400">
              <p className="font-medium">PixelFlow won't send this as it is:</p>
              <ul className="list-disc pl-5">
                {plan.problems.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            </div>
          )}
          {!plan.canSend && plan.reason && plan.problems.length === 0 && (
            <p className="flex items-start gap-2">
              {plan.changes.length === 0 && plan.reason.startsWith("The controller already") ? (
                <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-emerald-600" aria-hidden />
              ) : (
                <AlertTriangle size={16} className="mt-0.5 shrink-0 text-amber-600" aria-hidden />
              )}
              {plan.reason}
            </p>
          )}
          {plan.changes.length > 0 && (
            <>
              <p className="text-neutral-600 dark:text-neutral-300">
                Sending makes {plural(plan.changes.length, "change")} on {name}
                {warnings === 1 ? "; 1 of them turns pixels off or moves them" : warnings > 1 ? `; ${warnings} of them turn pixels off or move them` : ""}. A copy of its
                current setup is kept so you can put it back.
              </p>
              <SetupChanges label="Changes to send" changes={plan.changes} />
            </>
          )}
          {plan.notes.map((note) => (
            <p key={note} className="text-xs text-neutral-500">
              {note}
            </p>
          ))}
        </div>
      )}
      {sending && (
        <p role="status" className="flex items-center gap-2 text-neutral-600 dark:text-neutral-300">
          <Loader2 size={16} className="animate-spin" aria-hidden /> Keeping a copy of {name}'s setup, sending the new one, and reading it back…
        </p>
      )}
      {report && (
        <div className="flex flex-col gap-3">
          <Outcome report={report} />
          {report.mismatches.length > 0 && <SetupChanges label="Still different after sending" changes={report.mismatches} />}
          {report.canRestore && !putBackDone && <p className="text-neutral-600 dark:text-neutral-300">If the lights look wrong, put back the setup {name} had before sending.</p>}
        </div>
      )}
      {putBack.kind === "done" && (
        <p role="status" className={`mt-3 flex items-start gap-2 ${putBack.result.restored ? "" : "text-red-700 dark:text-red-400"}`}>
          {putBack.result.restored ? <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-emerald-600" aria-hidden /> : <XCircle size={16} className="mt-0.5 shrink-0" aria-hidden />}
          {putBack.result.message}
          {!putBack.result.restored && " You can try again."}
        </p>
      )}
    </DeviceDialog>
  );
}
