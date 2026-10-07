import { ExternalLink, Info } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { FppSoftware as Software } from "../../api/types";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { Section } from "./Section";

const BUILD_NAME: Record<string, string> = { "Pi-": "Raspberry Pi", "Pi64-": "Raspberry Pi", "BBB-": "BeagleBone", "BB64-": "BeagleBone" };

/** What FPP is running and whether FPP has a newer release for it. Read-only: PixelFlow never
 * upgrades anything; the button opens FPP's own About page, where the user does it. */
export function FppSoftware({ address, turn }: { address: string; turn: number }) {
  const backend = useApp((s) => s.backend);
  const [software, setSoftware] = useState<Software | { error: string } | null>(null);
  useEffect(() => {
    if (!backend) return;
    let current = true;
    setSoftware(null);
    backend.fppSoftware(address).then(
      (s) => current && setSoftware(s),
      (e) => current && setSoftware({ error: errorMessage(e) }),
    );
    return () => {
      current = false;
    };
  }, [backend, address, turn]);

  const info = software && !("error" in software) ? software : null;
  const update = info?.update ?? null;
  const rows: [string, string][] = info
    ? [
        ["FPP version", info.version],
        ["OS build", [info.osBuild, info.osRelease].filter(Boolean).join(" · ")],
        ["Platform", info.platform],
        ["System", info.bits ? `${info.bits}-bit` : ""],
      ]
    : [];
  return (
    <Section
      title="Software"
      actions={
        info && !info.checked ? (
          <span role="img" aria-label="Couldn't check for updates" title="Couldn't check for updates" data-tip="Couldn't check for updates" className="text-neutral-400">
            <Info size={14} aria-hidden />
          </span>
        ) : undefined
      }
    >
      {software === null && <p className="text-neutral-500">Reading what FPP is running…</p>}
      {software !== null && !info && <p className="text-neutral-500">Couldn't read FPP's software: {(software as { error: string }).error}</p>}
      {info && (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-0.5">
          {rows
            .filter(([, value]) => value)
            .map(([label, value]) => (
              <div key={label} className="contents">
                <dt className="text-neutral-500">{label}</dt>
                <dd className="truncate">{value}</dd>
              </div>
            ))}
        </dl>
      )}
      {update && info && (
        <div role="status" className="flex flex-col gap-1 rounded-md bg-sky-50 p-2 dark:bg-sky-950/50">
          <p className="font-medium">
            FPP {update.version} is available
            {update.major && " — a major upgrade; save a backup on FPP's Backups page first"}
          </p>
          <p>
            Choose <span className="font-mono">{update.file}</span> in FPP's Upgrade OS list.
          </p>
          <p className="text-xs text-neutral-600 dark:text-neutral-400">
            {update.prefix}
            {info.bits ? ` = ${info.bits}-bit ${BUILD_NAME[update.prefix] ?? ""}`.trimEnd() : ""}, which matches this box. Avoid nightly builds for a show.
          </p>
        </div>
      )}
      {info?.checked && !update && <p className="text-neutral-500">FPP is up to date.</p>}
      <div>
        <Button onClick={() => void backend?.openDevicePage(address, "about.php")} title="Opens FPP's About page in your web browser, where you update it yourself">
          <ExternalLink size={14} aria-hidden /> Open FPP's update page
        </Button>
      </div>
    </Section>
  );
}
