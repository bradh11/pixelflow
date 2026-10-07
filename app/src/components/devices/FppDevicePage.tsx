import { ArrowLeft, ExternalLink, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { ControllerCheck, Device, DeviceDetails } from "../../api/types";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { FppHealth } from "./FppHealth";
import { FppLibrary } from "./FppLibrary";
import { FppOutputs } from "./FppOutputs";
import { FppScheduleList } from "./FppScheduleList";
import { NowPlaying } from "./NowPlaying";
import { Dot } from "./Section";
import { useFppStatus } from "./useFppStatus";

const NO_DEVICES: Device[] = [];

/**
 * An FPP's own page on the Controllers screen: what it's playing, its health, what's stored on it,
 * where it sends its sequence (and setting up the show from that), and its schedule. Only Play,
 * Stop, and Send change anything on the FPP, and only when clicked.
 */
export function FppDevicePage({ device, onBack }: { device: Device; onBack: () => void }) {
  const backend = useApp((s) => s.backend);
  const devices = useApp((s) => s.discovery?.devices ?? NO_DEVICES);
  const { address } = device;
  const [turn, setTurn] = useState(0);
  const { status, error, refresh: refreshStatus } = useFppStatus(address);
  const [details, setDetails] = useState<DeviceDetails | null>(null);
  const [detailsError, setDetailsError] = useState<string | null>(null);
  const [reach, setReach] = useState<Record<string, ControllerCheck>>({});
  const back = useRef<HTMLButtonElement>(null);

  useEffect(() => back.current?.focus(), []);

  useEffect(() => {
    if (!backend) return;
    let current = true;
    setDetailsError(null);
    backend.inspectDevice(address).then(
      (d) => current && setDetails(d),
      (e) => {
        if (!current) return;
        setDetails(null);
        setDetailsError(errorMessage(e));
      },
    );
    return () => {
      current = false;
    };
  }, [backend, address, turn]);

  const targets = useMemo(() => [...new Set(details?.config.destinations.map((d) => d.address) ?? [])], [details]);
  useEffect(() => {
    if (!backend || targets.length === 0) return;
    let current = true;
    setReach({});
    backend.checkControllers(targets).then(
      (checks) => current && setReach(Object.fromEntries(checks.map((c) => [c.address, c]))),
      () => current && setReach({}),
    );
    return () => {
      current = false;
    };
  }, [backend, targets, turn]);

  const refresh = () => {
    setTurn((t) => t + 1);
    refreshStatus();
  };

  const shown = details?.device ?? device;
  const state = error ? "Not answering" : !status ? "Checking…" : status.state === "playing" ? "Playing" : status.state === "idle" ? "Idle" : status.state === "paused" ? "Paused" : status.state === "stopping" ? "Stopping" : "Busy";

  return (
    <div className="@container mx-auto flex max-w-[1680px] flex-col gap-3">
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <button
          ref={back}
          type="button"
          aria-label="Back to controllers"
          data-tip="Back to controllers"
          onClick={onBack}
          className="rounded-md p-1.5 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
        >
          <ArrowLeft size={18} aria-hidden />
        </button>
        <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-0.5">
          <h1 className="text-lg font-semibold">{shown.name}</h1>
          <span className="text-sm text-neutral-500">
            {[shown.model, shown.firmware, address].filter(Boolean).join(" · ")}
          </span>
          <span role="status" className="flex items-center gap-1.5 self-center text-xs text-neutral-600 dark:text-neutral-300">
            <Dot tone={error ? "bad" : status ? "ok" : "idle"} /> {state}
          </span>
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-2">
          <a
            href={`http://${address}/`}
            target="_blank"
            rel="noreferrer"
            title="Opens in your web browser"
            onClick={(e) => {
              e.preventDefault();
              void backend?.openDevicePage(address);
            }}
            className="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
          >
            <ExternalLink size={14} aria-hidden /> Open FPP's web page
          </a>
          <Button onClick={refresh} title="Read everything from the FPP again">
            <RefreshCw size={14} aria-hidden /> Refresh
          </Button>
        </div>
      </header>
      <div className="grid items-start gap-3 @[1000px]:grid-cols-[minmax(0,1fr)_minmax(320px,400px)] @[1400px]:grid-cols-[minmax(0,1fr)_440px]">
        <div className="flex min-w-0 flex-col gap-3">
          <NowPlaying address={address} status={status} error={error} onChanged={refreshStatus} />
          <FppLibrary address={address} fppName={shown.name} turn={turn} onPlayed={refreshStatus} />
        </div>
        <div className="flex min-w-0 flex-col gap-3">
          <FppHealth address={address} status={status} error={error} destinations={details?.config.destinations ?? []} reach={reach} devices={devices} />
          <FppOutputs address={address} details={details} error={detailsError} />
          <FppScheduleList address={address} turn={turn} />
        </div>
      </div>
    </div>
  );
}
