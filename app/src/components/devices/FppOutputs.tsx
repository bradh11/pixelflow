import { ArrowRight, CheckCircle2, Loader2, Wand2 } from "lucide-react";
import { useState } from "react";
import { errorMessage } from "../../api/backend";
import type { Controller, DeviceDetails, FppSetupPlan } from "../../api/types";
import { plural, thousands } from "../../lib/format";
import { useApp } from "../../state/store";
import { toast } from "../../state/toast";
import { Button } from "../ui";
import { Section } from "./Section";

const NO_CONTROLLERS: Controller[] = [];

const supported = (protocol: string) => protocol === "DDP" || protocol.startsWith("sACN");

function channelRange(start: number, count: number): string {
  return count > 0 ? `channels ${thousands(start)}–${thousands(start + count - 1)}` : "no channels yet";
}

/** The plan's controllers as lines: "Falcon_F16V5_B9F5 · DDP to 192.0.2.20 · channels 1–6,147". */
function PlanList({ plan }: { plan: FppSetupPlan }) {
  return (
    <ul className="flex flex-col gap-1">
      {plan.own && (
        <li>
          <span className="font-medium">{plan.own.controller.name}</span>
          <span className="text-neutral-600 dark:text-neutral-300">
            {" "}
            · its own {plural(plan.own.controller.ports.length, "port")}, with {plural(plan.own.props.length, "starter prop")}
          </span>
        </li>
      )}
      {plan.controllers.map((c) => (
        <li key={c.id}>
          <span className="font-medium">{c.name}</span>
          <span className="text-neutral-600 dark:text-neutral-300">
            {" "}
            · {c.protocol.type === "ddp" ? "DDP" : "sACN"} to {c.address}
            {c.sequenceChannels ? ` · ${channelRange(c.sequenceChannels.start, c.sequenceChannels.count)}` : ""}
          </span>
        </li>
      ))}
    </ul>
  );
}

/**
 * Where the FPP sends its sequence, and one action that adds those controllers to the show with
 * the channels the FPP uses. It shows what it will add first; adding is one undo step.
 */
export function FppOutputs({ address, details, error }: { address: string; details: DeviceDetails | null; error: string | null }) {
  const run = useApp((s) => s.run);
  const backend = useApp((s) => s.backend);
  const controllers = useApp((s) => s.snapshot?.show.controllers ?? NO_CONTROLLERS);
  const [plan, setPlan] = useState<FppSetupPlan | "loading" | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const preview = async () => {
    if (!backend) return;
    setPlan("loading");
    setPlanError(null);
    try {
      setPlan(await backend.fppSetupPlan(address));
    } catch (e) {
      setPlan(null);
      setPlanError(errorMessage(e));
    }
  };

  const add = async (shown: FppSetupPlan) => {
    if (busy) return;
    setBusy(true);
    const names = [...(shown.own ? [shown.own.controller] : []), ...shown.controllers].map((c) => c.name);
    const ok = await run((b) => b.fppSetUpShow(address, [...(shown.own ? [shown.own.controller.address] : []), ...shown.controllers.map((c) => c.address)]));
    setBusy(false);
    if (ok) {
      setPlan(null);
      toast(`Added ${names.join(", ")} to your show.`, { label: "Undo", run: () => useApp.getState().undo() });
    }
  };

  if (!details) {
    return (
      <Section title="Outputs → your show">
        {error ? (
          <p className="text-neutral-500">Can't read where this FPP sends while it isn't answering.</p>
        ) : (
          <p className="flex items-center gap-2 text-neutral-500">
            <Loader2 size={14} className="animate-spin" aria-hidden /> Reading this FPP's outputs…
          </p>
        )}
      </Section>
    );
  }

  const { config } = details;
  const inShow = (a: string) => controllers.some((c) => c.address === a);
  const ownPorts = config.ports.length;
  const toAdd = config.destinations.filter((d) => supported(d.protocol) && !inShow(d.address)).length + (ownPorts > 0 && !inShow(address) ? 1 : 0);
  const shown = plan !== null && plan !== "loading" ? plan : null;
  const shownCount = shown ? (shown.own ? 1 : 0) + shown.controllers.length : 0;

  return (
    <Section title="Outputs → your show">
      {config.destinations.length === 0 && ownPorts === 0 && <p className="text-neutral-500">This FPP doesn't send to any controllers.</p>}
      {(config.destinations.length > 0 || ownPorts > 0) && (
        <ul className="flex flex-col divide-y divide-neutral-100 dark:divide-neutral-800">
          {ownPorts > 0 && (
            <li className="flex items-center justify-between gap-2 py-1.5">
              <span>Its own outputs: {plural(ownPorts, "port")}</span>
              {inShow(address) && <span className="shrink-0 text-xs text-neutral-500">In your show</span>}
            </li>
          )}
          {config.destinations.map((d) => (
            <li key={`${d.address}-${d.protocol}`} className="flex items-center justify-between gap-2 py-1.5">
              <span className="min-w-0">
                <span className="flex items-center gap-1.5">
                  <span className="text-neutral-500">{d.protocol}</span>
                  <ArrowRight size={12} className="shrink-0 text-neutral-400" aria-hidden />
                  <span className="truncate font-medium">{d.description || d.address}</span>
                </span>
                <span className="block text-xs text-neutral-500 tabular-nums">
                  {d.description ? `${d.address} · ` : ""}
                  {thousands(d.channels)} channels ({channelRange(d.startChannel, d.channels)})
                </span>
              </span>
              <span className="shrink-0 text-xs text-neutral-500">{!supported(d.protocol) ? "Not supported yet" : inShow(d.address) ? "In your show" : ""}</span>
            </li>
          ))}
        </ul>
      )}

      {shown ? (
        <div role="group" aria-label="What will be added" className="flex flex-col gap-2 rounded-md border border-accent-500/40 bg-accent-50/60 p-3 dark:bg-accent-600/10">
          {shownCount > 0 ? (
            <>
              <p className="font-medium">This adds to your show:</p>
              <PlanList plan={shown} />
              {shown.controllers.length > 0 && (
                <p className="text-xs text-neutral-600 dark:text-neutral-300">Their strings aren't known yet: open each controller here once it's online to add them.</p>
              )}
            </>
          ) : (
            <p>Nothing to add: your show already has what this FPP sends to.</p>
          )}
          {shown.skipped.length > 0 && (
            <ul className="flex flex-col gap-0.5 text-xs text-neutral-600 dark:text-neutral-300">
              {shown.skipped.map((s) => (
                <li key={`${s.address}-${s.reason}`}>
                  Left out: {s.name} — {s.reason}
                </li>
              ))}
            </ul>
          )}
          {shown.notes.map((n) => (
            <p key={n} className="text-xs text-neutral-600 dark:text-neutral-300">
              {n}
            </p>
          ))}
          <div className="flex justify-end gap-2">
            <Button onClick={() => setPlan(null)}>{shownCount > 0 ? "Cancel" : "Close"}</Button>
            {shownCount > 0 && (
              <Button variant="primary" onClick={() => add(shown)} disabled={busy}>
                Add {plural(shownCount, "controller")}
              </Button>
            )}
          </div>
        </div>
      ) : toAdd > 0 ? (
        <div>
          <Button variant="primary" onClick={preview} disabled={plan === "loading"}>
            {plan === "loading" ? <Loader2 size={14} className="animate-spin" aria-hidden /> : <Wand2 size={14} aria-hidden />} Set up my show from this FPP
          </Button>
        </div>
      ) : (
        (config.destinations.length > 0 || ownPorts > 0) && (
          <p className="flex items-center gap-1.5 text-neutral-600 dark:text-neutral-300">
            <CheckCircle2 size={14} className="shrink-0 text-emerald-600" aria-hidden /> Your show has every controller this FPP sends to.
          </p>
        )
      )}
      {planError && (
        <p role="alert" className="text-red-600 dark:text-red-400">
          {planError}
        </p>
      )}
      {details.plan.notes.map((n) => (
        <p key={n} className="text-xs text-neutral-500">
          {n}
        </p>
      ))}
    </Section>
  );
}
