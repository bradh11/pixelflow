import { AlertTriangle, CheckCircle2, Loader2, RotateCcw, Send, XCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { RestoreReport, SendPlan, SendReport } from "../../api/types";
import { plural } from "../../lib/format";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { DeviceDialog } from "./DeviceDialog";
import { SetupChanges } from "./SetupChanges";

type Step = { kind: "review" } | { kind: "sending" } | { kind: "done"; report: SendReport } | { kind: "restoring"; report: SendReport } | { kind: "restored"; report: SendReport; restore: RestoreReport };

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

/**
 * "Send setup to this device…": shows, port by port, what sending the show's setup would change
 * on the controller, and sends it only when the user clicks Send. The controller's setup is kept
 * first; it's read back afterwards, and if anything went wrong one click puts the old one back.
 */
export function SendSetupDialog({ address, onClose }: { address: string; onClose: () => void }) {
  const backend = useApp((s) => s.backend);
  const [plan, setPlan] = useState<SendPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [step, setStep] = useState<Step>({ kind: "review" });
  const close = useCallback(() => onClose(), [onClose]);

  useEffect(() => {
    if (!backend) return;
    let current = true;
    backend.planDeviceSetup(address).then(
      (p) => current && setPlan(p),
      (e) => current && setError(errorMessage(e)),
    );
    return () => {
      current = false;
    };
  }, [backend, address]);

  const send = async () => {
    if (!backend || !plan || step.kind !== "review") return;
    setStep({ kind: "sending" });
    try {
      const report = await backend.sendDeviceSetup(
        address,
        plan.changes.map((c) => c.id),
      );
      setStep({ kind: "done", report });
    } catch (e) {
      setStep({ kind: "done", report: { status: "refused", message: errorMessage(e), mismatches: [], canRestore: false } });
    }
  };

  const restore = async (report: SendReport) => {
    if (!backend) return;
    setStep({ kind: "restoring", report });
    try {
      setStep({ kind: "restored", report, restore: await backend.restoreDeviceSetup(address) });
    } catch (e) {
      setStep({ kind: "restored", report, restore: { restored: false, message: errorMessage(e) } });
    }
  };

  const name = plan?.device.name ?? address;
  const warnings = plan?.changes.filter((c) => c.warning).length ?? 0;
  const busy = step.kind === "sending" || step.kind === "restoring";
  const report = step.kind === "review" || step.kind === "sending" ? null : step.report;

  return (
    <DeviceDialog
      title={`Send setup to ${name}`}
      subtitle={plan ? `What ${name} has now → what your show's ${plan.controllerName} needs. Only its outputs change; network settings are never touched.` : undefined}
      busy={busy}
      onClose={close}
      footer={
        step.kind === "review" ? (
          <>
            <Button data-autofocus onClick={onClose}>
              Cancel
            </Button>
            {plan?.canSend && (
              <Button variant="primary" onClick={send}>
                <Send size={14} aria-hidden /> Send to {name}
              </Button>
            )}
          </>
        ) : (
          <>
            {report?.canRestore && step.kind !== "restored" && (
              <Button onClick={() => restore(report)} disabled={busy}>
                {step.kind === "restoring" ? <Loader2 size={14} className="animate-spin" aria-hidden /> : <RotateCcw size={14} aria-hidden />} Put back the previous setup
              </Button>
            )}
            <Button variant="primary" onClick={onClose} disabled={busy}>
              Close
            </Button>
          </>
        )
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
      {plan && step.kind === "review" && (
        <div className="flex flex-col gap-3">
          {plan.busy && (
            <p className="flex items-start gap-2 rounded-md border border-amber-300 bg-amber-50 p-2 text-amber-800 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
              <AlertTriangle size={16} className="mt-0.5 shrink-0" aria-hidden /> {plan.busy}
            </p>
          )}
          {!plan.canSend && plan.reason && (
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
                {warnings > 0 ? `; ${plural(warnings, "change")} turn pixels off or remove strings` : ""}. A copy of its current setup is kept so you can put it back.
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
      {step.kind === "sending" && (
        <p role="status" className="flex items-center gap-2 text-neutral-600 dark:text-neutral-300">
          <Loader2 size={16} className="animate-spin" aria-hidden /> Keeping a copy of {name}'s setup, sending the new one, and reading it back…
        </p>
      )}
      {report && (
        <div className="flex flex-col gap-3">
          <Outcome report={report} />
          {report.mismatches.length > 0 && <SetupChanges label="Still different after sending" changes={report.mismatches} />}
          {report.canRestore && step.kind !== "restored" && <p className="text-neutral-600 dark:text-neutral-300">You can put back the setup {name} had before sending.</p>}
          {step.kind === "restored" && (
            <p role="status" className={`flex items-start gap-2 ${step.restore.restored ? "" : "text-red-700 dark:text-red-400"}`}>
              {step.restore.restored ? <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-emerald-600" aria-hidden /> : <XCircle size={16} className="mt-0.5 shrink-0" aria-hidden />}
              {step.restore.message}
            </p>
          )}
        </div>
      )}
    </DeviceDialog>
  );
}
