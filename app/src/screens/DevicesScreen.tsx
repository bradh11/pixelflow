import { AlertTriangle, ChevronRight, Loader2, Radar, Search, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { Device, DeviceKind, FoundBy } from "../api/types";
import { ImportDialog } from "../components/ImportDialog";
import { CompareDialog } from "../components/devices/CompareDialog";
import { SendSetupDialog } from "../components/devices/SendSetupDialog";
import { SetupButtons } from "../components/devices/SetupButtons";
import { FppDevicePage } from "../components/devices/FppDevicePage";
import { GoToScreen } from "../components/GoToScreen";
import { Button, EmptyState, Input, PageHeader } from "../components/ui";
import { ago } from "../lib/format";
import { type KnownDevice, useApp } from "../state/store";

const KIND_LABEL: Record<DeviceKind, string> = { fpp: "FPP", falcon: "Falcon", wled: "WLED" };
const KIND_STYLE: Record<DeviceKind, string> = {
  fpp: "bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300",
  falcon: "bg-violet-100 text-violet-800 dark:bg-violet-950 dark:text-violet-300",
  wled: "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300",
};
const FOUND_BY: Record<FoundBy, string> = {
  ping: "answered discovery",
  webSweep: "network scan",
  mdns: "announced itself",
  fppPeer: "listed by an FPP",
  manual: "address you entered",
};

function DeviceRow({
  device,
  inShow,
  onReview,
  onForget,
  onCompare,
  onSend,
}: {
  device: KnownDevice;
  inShow: boolean;
  onReview: () => void;
  onForget: () => void;
  onCompare: () => void;
  onSend: () => void;
}) {
  const backend = useApp((s) => s.backend);
  const [updateAvailable, setUpdateAvailable] = useState(false);
  // An FPP with a newer release for it gets a small badge (read-only; a failed check says nothing).
  useEffect(() => {
    if (!backend || device.kind !== "fpp" || !device.responding) return;
    let current = true;
    backend.fppSoftware(device.address).then(
      (s) => current && setUpdateAvailable(s.update !== null),
      () => current && setUpdateAvailable(false),
    );
    return () => {
      current = false;
    };
  }, [backend, device.address, device.kind, device.responding]);
  return (
    <tr
      onClick={onReview}
      className={`cursor-pointer border-t border-neutral-200 hover:bg-neutral-50 dark:border-neutral-800 dark:hover:bg-neutral-900 ${
        device.responding ? "" : "text-neutral-500"
      }`}
    >
      <td className="py-2 pr-3">
        <span className={`rounded px-1.5 py-0.5 text-xs font-medium ${KIND_STYLE[device.kind]}`}>{KIND_LABEL[device.kind]}</span>
      </td>
      <td className="pr-3">
        <div className="font-medium">{device.name}</div>
        <div className="text-xs text-neutral-500">
          {device.model} · {device.firmware}
          {device.mode ? ` · ${device.mode}` : ""}
        </div>
        {!device.responding && (
          <div className="flex items-center gap-1 text-xs text-amber-700 dark:text-amber-400">
            <AlertTriangle size={12} /> Not responding · last seen {ago(device.lastSeen)}
          </div>
        )}
      </td>
      <td className="pr-3 text-sm tabular-nums">{device.address}</td>
      <td className="pr-3 text-xs text-neutral-500">{device.foundBy.map((f) => FOUND_BY[f]).join(", ")}</td>
      <td className="pr-3">
        <span className="flex flex-wrap items-center gap-1">
          {inShow && <span className="rounded bg-neutral-100 px-1.5 py-0.5 text-xs dark:bg-neutral-800">In show</span>}
          {updateAvailable && (
            <span title="A newer FPP is available. Open this FPP to see which." className="rounded bg-sky-100 px-1.5 py-0.5 text-xs text-sky-800 dark:bg-sky-950 dark:text-sky-300">
              Update available
            </span>
          )}
          {inShow && device.responding && <SetupButtons compact name={device.name} onCompare={onCompare} onSend={onSend} />}
        </span>
      </td>
      <td className="w-px pr-6 whitespace-nowrap">
        {/* Apart from Open, and in words: forgetting is easy to do by mistake next to it. */}
        <button
          type="button"
          aria-label={`Forget ${device.name}`}
          title="Take this controller off the list on this computer (your show isn't changed)"
          onClick={(e) => {
            e.stopPropagation();
            onForget();
          }}
          className="inline-flex items-center gap-1 rounded px-1.5 py-1 text-xs text-neutral-500 hover:bg-neutral-200/70 hover:text-neutral-800 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
        >
          <X size={12} aria-hidden /> Forget
        </button>
      </td>
      <td className="w-px text-right whitespace-nowrap">
        <Button
          onClick={(e) => {
            e.stopPropagation();
            onReview();
          }}
          aria-label={`Open ${device.name}`}
        >
          Open <ChevronRight size={14} />
        </Button>
      </td>
    </tr>
  );
}

/** Finds controllers on the network and imports their configuration. */
export function DevicesScreen() {
  const snapshot = useApp((s) => s.snapshot);
  const discovery = useApp((s) => s.discovery);
  const scanning = useApp((s) => s.scanning);
  const scan = useApp((s) => s.scan);
  const forgetDevice = useApp((s) => s.forgetDevice);
  const [address, setAddress] = useState("");
  const [reviewing, setReviewing] = useState<string | null>(null);
  /** The FPP whose page is open, by address. */
  const [openFpp, setOpenFpp] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  /** The controller being compared, or sent its setup, by address. */
  const [comparing, setComparing] = useState<string | null>(null);
  const [sending, setSending] = useState<string | null>(null);

  const checkAddress = async () => {
    const host = address.trim();
    if (!host) return;
    setNotice(null);
    if (await scan([host])) {
      const found = useApp.getState().discovery?.devices.some((d) => d.address === host && d.responding);
      setNotice(found ? null : `No controller answered at ${host}. Check the address and that it's powered on.`);
      if (found) setAddress("");
    }
  };

  const inShow = (device: Device) => snapshot?.show.controllers.some((c) => c.address === device.address) ?? false;

  const dialogs = (
    <>
      {comparing && <CompareDialog address={comparing} onClose={() => setComparing(null)} />}
      {sending && <SendSetupDialog address={sending} onClose={() => setSending(null)} />}
    </>
  );
  const fpp = openFpp ? discovery?.devices.find((d) => d.address === openFpp) : undefined;
  if (fpp) {
    return (
      <>
        <FppDevicePage
          key={fpp.address}
          device={fpp}
          onBack={() => setOpenFpp(null)}
          onCompare={inShow(fpp) ? () => setComparing(fpp.address) : undefined}
          onSend={inShow(fpp) ? () => setSending(fpp.address) : undefined}
        />
        {dialogs}
      </>
    );
  }

  return (
    <div className="mx-auto max-w-7xl">
      <PageHeader
        title="Controllers"
        description="Find FPP, Falcon, and WLED controllers on your network and add them to your show. Nothing on a controller changes unless you send it a setup."
        actions={
          <Button variant="primary" onClick={() => scan()} disabled={scanning}>
            {scanning ? <Loader2 size={16} className="animate-spin" /> : <Radar size={16} />}
            {discovery ? "Scan again" : "Scan network"}
          </Button>
        }
      />
      <form
        className="mb-6 flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void checkAddress();
        }}
      >
        <Input
          aria-label="Controller address"
          placeholder="Controller IP address, e.g. 192.168.1.50"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          className="w-80"
        />
        <Button type="submit" disabled={scanning || !address.trim()}>
          <Search size={16} /> Check address
        </Button>
      </form>
      {notice && (
        <p role="status" className="mb-4 text-sm text-neutral-700 dark:text-neutral-300">
          {notice}
        </p>
      )}
      {scanning && !discovery && (
        <p className="flex items-center gap-2 text-sm text-neutral-500">
          <Loader2 size={16} className="animate-spin" /> Looking for controllers… this takes a few seconds.
        </p>
      )}
      {!scanning && !discovery && (
        <EmptyState title="Find your controllers">
          <p>Look for FPP, Falcon, and WLED controllers on your network, or check a specific address above.</p>
          <div className="mt-3 flex flex-wrap items-center justify-center gap-2">
            <Button variant="primary" onClick={() => scan()}>
              <Radar size={16} aria-hidden /> Scan my network
            </Button>
            <GoToScreen screen="wiring">Or add a controller by hand</GoToScreen>
          </div>
        </EmptyState>
      )}
      {discovery && discovery.devices.length === 0 && discovery.silent.length === 0 && discovery.locked.length === 0 && (
        <EmptyState title="No controllers found">
          Make sure this computer is on the same network as your controllers. If your computer asks whether PixelFlow
          may accept incoming connections, allow it so controllers can answer. You can also check a specific address.
          <div className="mt-3 flex flex-wrap items-center justify-center gap-2">
            <Button onClick={() => scan()}>
              <Radar size={16} aria-hidden /> Scan my network again
            </Button>
            <GoToScreen screen="wiring">Or add a controller by hand</GoToScreen>
          </div>
        </EmptyState>
      )}
      {discovery && discovery.devices.length > 0 && (
        <table className="w-full">
          <thead>
            <tr className="text-left text-xs tracking-wide text-neutral-500 uppercase">
              <th className="pb-2 font-medium">Type</th>
              <th className="pb-2 font-medium">Controller</th>
              <th className="pb-2 font-medium">Address</th>
              <th className="pb-2 font-medium">Found by</th>
              <th />
              <th>
                <span className="sr-only">Forget</span>
              </th>
              <th />
            </tr>
          </thead>
          <tbody>
            {discovery.devices.map((device) => (
              <DeviceRow
                key={device.address}
                device={device}
                inShow={inShow(device)}
                onReview={() => (device.kind === "fpp" ? setOpenFpp(device.address) : setReviewing(device.address))}
                onForget={() => forgetDevice(device.address)}
                onCompare={() => setComparing(device.address)}
                onSend={() => setSending(device.address)}
              />
            ))}
          </tbody>
        </table>
      )}
      {discovery && discovery.silent.length > 0 && (
        <ul className="mt-6 flex flex-col gap-2">
          {discovery.silent.map((peer) => (
            <li key={peer.address} className="flex items-start gap-2 text-sm text-amber-700 dark:text-amber-400">
              <AlertTriangle size={16} className="mt-0.5 shrink-0" />
              <span>
                <strong>{peer.description || peer.address}</strong> ({peer.address}) isn't responding. {peer.listedBy} lists
                it — check that it's powered on and connected, then scan again.
              </span>
            </li>
          ))}
        </ul>
      )}
      {discovery && discovery.locked.length > 0 && (
        <ul className="mt-6 flex flex-col gap-2">
          {discovery.locked.map((address) => (
            <li key={address} className="flex items-start gap-2 text-sm text-amber-700 dark:text-amber-400">
              <AlertTriangle size={16} className="mt-0.5 shrink-0" />
              <span>
                <strong>{address}</strong> asks for a password, so PixelFlow can't read it. If it's an FPP, turn off its UI
                and API password in FPP's settings, then scan again.
              </span>
            </li>
          ))}
        </ul>
      )}
      {reviewing && (
        <ImportDialog
          address={reviewing}
          onClose={() => setReviewing(null)}
          onImported={(message) => setNotice(message)}
          onCompare={() => {
            setReviewing(null);
            setComparing(reviewing);
          }}
        />
      )}
      {dialogs}
    </div>
  );
}
