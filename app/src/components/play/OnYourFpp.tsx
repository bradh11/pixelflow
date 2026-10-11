import { Check, Send } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { SequenceEntry } from "../../api/types";
import { fileName } from "../../lib/format";
import { fppFileName } from "../../lib/fppNames";
import { useApp } from "../../state/store";
import { type FppChoice, SendToFppDialog, fppChoices } from "../SendToFppDialog";
import { Button } from "../ui";
import { DrawerSection } from "./DrawerSection";

/** Whether the show sequence's file is among the FPP's sequence files, under exactly the name it
 * would be sent as (the FPP's files are case-sensitive: other capitals are another file). */
function isOnFpp(entry: SequenceEntry, names: string[]): boolean {
  const name = fppFileName(fileName(entry.path).replace(/\.fseq$/i, ""), "fseq");
  return names.includes(name);
}

/**
 * Which of the show's sequences are on each FPP, with Send for the missing ones (in the playlist
 * drawer; nothing while no FPP is known or the playlist is empty). Each FPP is read once (one
 * quick request), and again after a send or on Check again; reading changes nothing.
 */
export function OnYourFpp({ sequences }: { sequences: SequenceEntry[] }) {
  const backend = useApp((s) => s.backend);
  const controllers = useApp((s) => s.snapshot?.show.controllers);
  const devices = useApp((s) => s.discovery?.devices);
  const fpps = useMemo(() => fppChoices(controllers, devices), [controllers, devices]);
  const [found, setFound] = useState<Record<string, string[] | { error: string }>>({});
  const [turn, setTurn] = useState(0);
  const [sending, setSending] = useState<{ entry: SequenceEntry; fpp: FppChoice } | null>(null);
  const addresses = fpps.map((f) => f.address).join(",");
  useEffect(() => {
    if (!backend) return;
    let current = true;
    for (const address of addresses ? addresses.split(",") : []) {
      backend.fppSequenceNames(address).then(
        (names) => current && setFound((f) => ({ ...f, [address]: names })),
        (e) => current && setFound((f) => ({ ...f, [address]: { error: errorMessage(e) } })),
      );
    }
    return () => {
      current = false;
    };
  }, [backend, addresses, turn]);
  if (fpps.length === 0 || sequences.length === 0) return null;
  return (
    <DrawerSection id="fpp" title="On your FPP">
      {fpps.map((fpp) => {
        const names = found[fpp.address];
        const on = Array.isArray(names) ? sequences.filter((s) => isOnFpp(s, names)).length : 0;
        return (
          <div key={fpp.address} className="flex flex-col gap-1.5 text-sm">
            <div className="flex items-center justify-between gap-2">
              <h3 className="min-w-0 font-medium">
                On {fpp.name} <span className="font-normal text-neutral-500">({fpp.address})</span>
              </h3>
              <Button variant="ghost" className="shrink-0" title={`Look at what's on ${fpp.name} again`} onClick={() => setTurn((t) => t + 1)}>
                Check again
              </Button>
            </div>
            {names === undefined && <p className="text-neutral-500">Checking {fpp.name}…</p>}
            {names !== undefined && !Array.isArray(names) && (
              <p className="text-amber-700 dark:text-amber-400">
                Couldn't check {fpp.name}: {names.error}
              </p>
            )}
            {Array.isArray(names) && (
              <>
                <p className="text-neutral-600 dark:text-neutral-300">
                  {on === sequences.length ? `All ${sequences.length} sequences are on ${fpp.name}.` : `${on} of ${sequences.length} sequences are on ${fpp.name}.`}
                </p>
                <ul className="flex max-h-48 flex-col gap-1 overflow-auto">
                  {sequences.map((entry) => (
                    <li key={entry.id} aria-label={entry.name} className="flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5">
                      <span className="min-w-0 truncate">{entry.name}</span>
                      {isOnFpp(entry, names) ? (
                        <span
                          className="flex shrink-0 items-center gap-1 text-xs text-green-700 dark:text-green-400"
                          title="A sequence file with this name is on the FPP. PixelFlow doesn't check that it's the same version."
                        >
                          <Check size={14} aria-hidden /> Same name on the FPP
                        </span>
                      ) : (
                        <Button
                          className="shrink-0"
                          aria-label={`Send ${entry.name} to ${fpp.name}`}
                          title={`Put ${entry.name} and its music on ${fpp.name}`}
                          onClick={() => setSending({ entry, fpp })}
                        >
                          <Send size={14} /> Send…
                        </Button>
                      )}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        );
      })}
      {sending && (
        <SendToFppDialog
          source={{ kind: "file", path: sending.entry.path }}
          title={sending.entry.name}
          music={sending.entry.audio}
          address={sending.fpp.address}
          onClose={() => {
            setSending(null);
            setTurn((t) => t + 1);
          }}
        />
      )}
    </DrawerSection>
  );
}
