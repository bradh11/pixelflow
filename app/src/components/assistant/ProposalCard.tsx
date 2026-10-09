import { Check, Eye, EyeOff, Magnet, Minus, Pencil, Play, Plus, ShieldAlert, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { Change, DiffSection, ProposalView, SectionSummary } from "../../api/assistant";
import { plural } from "../../lib/format";
import { formatTime } from "../../lib/timelineMath";
import { useAssistant } from "../../state/assistant";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { TimelineThumbnail } from "./TimelineThumbnail";

const SECTION_LABELS: Record<DiffSection, string> = {
  show: "Show",
  prop: "Props",
  group: "Groups",
  controller: "Controllers",
  playlist: "Playlist",
  sequence: "Sequence",
  row: "Sequence rows",
  effect: "Effects",
  timingTrack: "Timing tracks",
};

const ORDER: DiffSection[] = ["show", "prop", "group", "controller", "playlist", "sequence", "row", "effect", "timingTrack"];

/** Details shown before "Show N more". */
const FIRST_DETAILS = 5;
/** Changes listed in a section before "Show all". */
const FIRST_CHANGES = 12;

/** What a sequence proposal does in each section of the song. */
function Sections({ sections }: { sections: SectionSummary[] }) {
  return (
    <div className="mt-2">
      <h4 className="text-xs font-medium tracking-wide text-neutral-500 uppercase">By section</h4>
      <ul className="mt-0.5 text-xs">
        {sections.map((s, i) => {
          const counts = [s.added && `${s.added} added`, s.changed && `${s.changed} changed`, s.removed && `${s.removed} removed`].filter(Boolean).join(", ");
          return (
            <li key={`${s.label}-${i}`} className="py-0.5">
              <span className="font-medium">{s.label}</span>{" "}
              <span className="text-neutral-500">
                {formatTime(s.startMs, 1000)}–{formatTime(s.endMs, 1000)}
              </span>
              {": "}
              {counts ? `${counts} on ${plural(s.rows, "row")}` : "no changes"}
              {s.kinds.length > 0 && <span className="text-neutral-500"> · {s.kinds.join(", ")}</span>}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** Sections whose long lists fold: a sequence's many effects, rows, and timing tracks. Everything
 * else (props, controllers, the playlist...) is always listed in full, so no warning is ever out
 * of sight. */
const FOLDING: DiffSection[] = ["effect", "row", "timingTrack"];

/** One section of the change list; a long effect, row, or timing-track list shows its first few
 * until asked. A change with a warning is always shown. */
function ChangeSection({ section, label, changes }: { section: DiffSection; label: string; changes: Change[] }) {
  const [all, setAll] = useState(false);
  const folds = FOLDING.includes(section) && changes.length > FIRST_CHANGES;
  const shown = all || !folds ? changes : changes.filter((c, i) => i < FIRST_CHANGES || c.warnings.length > 0);
  return (
    <div className="mt-1.5">
      <h4 className="text-xs font-medium tracking-wide text-neutral-500 uppercase">{label}</h4>
      <ul>
        {shown.map((change, i) => (
          <ChangeLine key={`${change.id ?? change.name}-${i}`} change={change} />
        ))}
      </ul>
      {folds && (
        <button
          type="button"
          aria-expanded={all}
          onClick={() => setAll(!all)}
          className="text-xs text-accent-600 underline-offset-2 hover:underline dark:text-accent-400"
        >
          {all ? "Show fewer" : `Show all ${changes.length}`}
        </button>
      )}
    </div>
  );
}

function Warning({ text }: { text: string }) {
  return (
    <li className="flex items-start gap-1 text-amber-700 dark:text-amber-400">
      <ShieldAlert size={12} className="mt-0.5 shrink-0" aria-hidden />
      <span>
        <span className="sr-only">Check: </span>
        {text}
      </span>
    </li>
  );
}

function ChangeLine({ change }: { change: Change }) {
  const [all, setAll] = useState(false);
  const { icon, tone, word } =
    change.action === "added"
      ? { icon: <Plus size={12} aria-hidden />, tone: "text-green-600 dark:text-green-400", word: "Add" }
      : change.action === "removed"
        ? { icon: <Minus size={12} aria-hidden />, tone: "text-red-600 dark:text-red-400", word: "Remove" }
        : { icon: <Pencil size={12} aria-hidden />, tone: "text-amber-600 dark:text-amber-400", word: "Change" };
  const hidden = change.details.length - FIRST_DETAILS;
  const details = all || hidden <= 0 ? change.details : change.details.slice(0, FIRST_DETAILS);
  return (
    <li className="py-0.5">
      <span className={`inline-flex items-center gap-1 ${tone}`}>
        {icon}
        <span className="sr-only">{word}:</span>
      </span>{" "}
      <span className="font-medium">{change.name}</span>
      {change.warnings.length > 0 && (
        <ul className="ml-4 text-xs">
          {change.warnings.map((warning, i) => (
            <Warning key={i} text={warning} />
          ))}
        </ul>
      )}
      {details.length > 0 && (
        <ul className="ml-4 list-disc text-xs break-words text-neutral-500 dark:text-neutral-400">
          {details.map((detail, i) => (
            <li key={i}>{detail}</li>
          ))}
        </ul>
      )}
      {hidden > 0 && (
        <button
          type="button"
          aria-expanded={all}
          onClick={() => setAll(!all)}
          className="ml-4 text-xs text-accent-600 underline-offset-2 hover:underline dark:text-accent-400"
        >
          {all ? "Show fewer" : `Show ${hidden} more`}
        </button>
      )}
    </li>
  );
}

function appliedNote(proposal: ProposalView): string {
  if (proposal.changesSequence && !proposal.changesShow) {
    return "Applied as one step. Undo (⌘Z) on the Sequence screen takes it all back.";
  }
  return "Applied as one step. Undo (⌘Z) takes it all back.";
}

/** Whether lights are running now (live output or playback), checked once when the card shows. */
function useLightsRunning(): boolean {
  const backend = useApp((s) => s.backend);
  const [running, setRunning] = useState(false);
  useEffect(() => {
    if (!backend) return;
    let current = true;
    void Promise.all([backend.outputStatus(), backend.playbackStatus()]).then(
      ([output, playback]) => current && setRunning(output.running || (playback !== null && playback.state === "playing")),
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [backend]);
  return running;
}

/** The assistant's proposal: its summary, every change by section, and Preview / Apply / Discard. */
export function ProposalCard({ proposal, current }: { proposal: ProposalView; current: boolean }) {
  const status = useAssistant((s) => s.proposalStatus);
  const previewing = useAssistant((s) => s.preview !== null);
  const busy = useAssistant((s) => s.busy);
  const streaming = useAssistant((s) => s.streaming);
  const { apply, discard, showPreview, hidePreview } = useAssistant.getState();
  const running = useLightsRunning();
  const changes = proposal.diff.changes;
  const counts = {
    added: changes.filter((c) => c.action === "added").length,
    changed: changes.filter((c) => c.action === "changed").length,
    removed: changes.filter((c) => c.action === "removed").length,
  };
  const tally = [
    counts.added && `${counts.added} to add`,
    counts.changed && `${counts.changed} to change`,
    counts.removed && `${counts.removed} to remove`,
  ]
    .filter(Boolean)
    .join(", ");
  const open = current && status === "open";
  const touchesControllers = changes.some((c) => c.section === "controller");
  return (
    <section
      aria-label="Proposed changes"
      className="rounded-lg border border-accent-500/40 bg-accent-50/60 p-3 text-sm dark:border-accent-400/30 dark:bg-accent-600/10"
    >
      <div className="flex items-start justify-between gap-2">
        <h3 className="font-semibold">Proposed changes</h3>
        <span className="shrink-0 text-xs text-neutral-500">{tally}</span>
      </div>
      <p className="mt-1">{proposal.summary}</p>
      {proposal.lockedEdges > 0 && (
        <p className="mt-0.5 flex items-center gap-1 text-xs text-neutral-500" title="Effect edges and timing marks moved onto the nearest section start, accent, bar, or beat">
          <Magnet size={12} aria-hidden /> Locked {plural(proposal.lockedEdges, "edge")} to the music
        </p>
      )}
      {proposal.timeline && <TimelineThumbnail timeline={proposal.timeline} />}
      {proposal.sections.length > 0 && <Sections sections={proposal.sections} />}
      {open && running && touchesControllers && (
        <ul className="mt-2 text-xs">
          <Warning text="Your lights are running: applying changes where their data is sent right away." />
        </ul>
      )}
      <div className="relative mt-2 max-h-80 overflow-auto">
        {ORDER.filter((section) => changes.some((c) => c.section === section)).map((section) => (
          <ChangeSection key={section} section={section} label={SECTION_LABELS[section]} changes={changes.filter((c) => c.section === section)} />
        ))}
      </div>
      {open ? (
        <div className="mt-3 flex flex-wrap gap-2">
          {(proposal.changesShow || proposal.changesSequence) && (
            <Button disabled={streaming} onClick={() => void (previewing ? hidePreview() : showPreview())} aria-pressed={previewing}>
              {previewing ? (
                <EyeOff size={14} aria-hidden />
              ) : proposal.changesSequence ? (
                <Play size={14} aria-hidden />
              ) : (
                <Eye size={14} aria-hidden />
              )}
              {previewing ? "Hide preview" : proposal.changesSequence ? "Play preview" : "Preview"}
            </Button>
          )}
          <Button variant="primary" disabled={busy || streaming} onClick={() => void apply()}>
            <Check size={14} aria-hidden /> Apply
          </Button>
          <Button variant="danger" disabled={busy || streaming} onClick={() => void discard()}>
            <X size={14} aria-hidden /> Discard
          </Button>
        </div>
      ) : (
        <p role="status" className="mt-2 text-xs text-neutral-500">
          {!current
            ? "Replaced by a newer proposal."
            : status === "applied"
              ? appliedNote(proposal)
              : status === "dropped"
                ? "A different show is open now, so this suggestion was dropped."
                : "Discarded. Nothing was changed."}
        </p>
      )}
    </section>
  );
}
