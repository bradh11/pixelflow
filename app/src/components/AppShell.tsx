import {
  AlertTriangle,
  Search,
  Command,
  Moon,
  Redo2,
  Save,
  Sparkles,
  Sun,
  Undo2,
} from "lucide-react";
import { useAssistant } from "../state/assistant";
import { AssistantPanel } from "./assistant/AssistantPanel";
import { useEffect, useState, type ReactNode } from "react";
import { errorMessage } from "../api/backend";
import { fileName, plural, shownPath, thousands } from "../lib/format";
import { useShallow } from "zustand/react/shallow";
import { useApp } from "../state/store";
import { DevicesScreen } from "../screens/DevicesScreen";
import { HistoryScreen } from "../screens/HistoryScreen";
import { LayoutScreen } from "../screens/LayoutScreen";
import { PlayScreen } from "../screens/PlayScreen";
import { SequenceScreen } from "../screens/SequenceScreen";
import { useSequencer } from "../state/sequencer";
import { TestScreen } from "../screens/TestScreen";
import { WiringScreen } from "../screens/WiringScreen";
import { MissingFileNotice, MissingFilesBanner } from "./MissingFiles";
import { ShowMenu } from "./ShowMenu";
import { Sidebar } from "./Sidebar";
import { useWindowBand } from "../lib/useWidth";
import { Button, UnsavedBadge } from "./ui";
import { redoTarget, saveFocused, undoFocused } from "../state/menuActions";
import { nextLabels, useUndoLabels } from "../state/undoLabels";

function IconButton({
  label,
  hint,
  shortcut,
  onClick,
  disabled,
  dim,
  children,
}: {
  label: string;
  hint?: string;
  shortcut?: string;
  onClick: () => void;
  disabled?: boolean;
  dim?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      data-tip={hint ?? label}
      data-tip-key={shortcut}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-md p-2 text-neutral-600 hover:bg-neutral-200/70 disabled:opacity-30 disabled:hover:bg-transparent dark:text-neutral-300 dark:hover:bg-neutral-800 ${dim ? "opacity-40 hover:opacity-100" : ""}`}
    >
      {children}
    </button>
  );
}

/**
 * The Undo (or Redo) button: what it acts on (see `undoTarget`), whether it can, and what it would
 * take back, for its tooltip ("Undo: Move Mega Tree"). Only what the button needs is watched, so
 * playback doesn't redraw the top bar.
 */
function useUndoButton(redo: boolean) {
  const onSequence = useApp((s) => s.screen === "sequence");
  const hasSequence = useSequencer((s) => s.doc !== null);
  const seq = useSequencer(useShallow((s) => ({ can: redo ? s.canRedo : s.canUndo, revision: s.revision })));
  const show = useApp(useShallow((s) => ({ can: (redo ? s.snapshot?.canRedo : s.snapshot?.canUndo) ?? false, revision: s.snapshot?.revision })));
  const names = useUndoLabels();
  const both = onSequence && hasSequence;
  // On the Sequence screen Undo works on the sequence only; Redo follows what was undone there.
  const target = !both ? "show" : redo ? redoTarget() : "sequence";
  const verb = redo ? "Redo" : "Undo";
  const label = both ? `${verb} (${target})` : verb;
  const name = target === "sequence" ? nextLabels(names.sequence, seq.revision) : nextLabels(names.show, show.revision);
  const what = redo ? name.redo : name.undo;
  const can = target === "sequence" ? seq.can : show.can;
  return { label, hint: what && can ? `${label}: ${what}` : label, can };
}

function TopBar() {
  const snapshot = useApp((s) => s.snapshot);
  const theme = useApp((s) => s.theme);
  const { setPaletteOpen, setTheme, closeShow } = useApp.getState();
  const undo = useUndoButton(false);
  const redo = useUndoButton(true);
  const onSequence = useApp((s) => s.screen === "sequence");
  const hasSequence = useSequencer((s) => s.doc !== null);
  const target = { sequence: onSequence && hasSequence };
  const sequenceName = useSequencer((s) => s.doc?.name ?? null);
  const sequenceDirty = useSequencer((s) => s.dirty);
  const sequencePath = useSequencer((s) => s.path);
  if (!snapshot) return null;
  // On the Sequence screen, the show and the sequence each say whether they're saved.
  const both = target.sequence && sequenceName !== null;
  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-neutral-200 px-3 dark:border-neutral-800">
      <button
        type="button"
        title="Close the show and go to the start page"
        aria-label="PixelFlow home (closes the show)"
        onClick={() => void closeShow()}
        className="shrink-0 rounded-md px-1 font-semibold text-accent-600 hover:bg-neutral-200/70 dark:text-accent-400 dark:hover:bg-neutral-800"
      >
        PixelFlow
      </button>
      <span className="text-neutral-300 dark:text-neutral-700">/</span>
      <ShowMenu />
      {snapshot.dirty && <UnsavedBadge doc="show" />}
      {snapshot.path && !both && <span className="hidden truncate text-xs text-neutral-500 lg:inline">{fileName(snapshot.path)}</span>}
      {both && (
        <>
          <span className="text-neutral-300 dark:text-neutral-700">/</span>
          <span className="truncate font-medium" title={sequencePath ? shownPath(sequencePath) : undefined}>
            {sequenceName}
          </span>
        </>
      )}
      <div className="ml-auto flex items-center gap-1">
        <IconButton label={undo.label} hint={undo.hint} shortcut="⌘Z" onClick={() => void undoFocused(false)} disabled={!undo.can}>
          <Undo2 size={18} />
        </IconButton>
        <IconButton label={redo.label} hint={redo.hint} shortcut="⇧⌘Z" onClick={() => void undoFocused(true)} disabled={!redo.can}>
          <Redo2 size={18} />
        </IconButton>
        {/* The same save as ⌘S and File → Save; quiet when there's nothing to save. */}
        <IconButton
          label={target.sequence ? "Save (the sequence, and the show if it changed)" : "Save"}
          onClick={() => void saveFocused(false)}
          dim={!snapshot.dirty && !(target.sequence && sequenceDirty)}
        >
          <Save size={18} />
        </IconButton>
        <IconButton label={theme === "dark" ? "Light theme" : "Dark theme"} onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
          {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
        </IconButton>
        <AssistantButton />
        <button
          type="button"
          onClick={() => setPaletteOpen(true)}
          className="ml-1 flex items-center gap-2 rounded-md border border-neutral-300 px-2.5 py-1 text-sm text-neutral-500 hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-800"
        >
          <Command size={14} /> Commands <kbd className="text-xs">⌘K</kbd>
        </button>
      </div>
    </header>
  );
}

/** Output keeps running when the user leaves the Test screen, so show it everywhere. */
function LiveOutput() {
  const backend = useApp((s) => s.backend);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    const poll = async () => {
      try {
        const next = await backend.outputStatus();
        if (!cancelled) setRunning(next.running);
      } catch {
        // Polling errors are transient; the next poll retries.
      }
    };
    void poll();
    const timer = setInterval(poll, 1000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend]);

  if (!backend || !running) return null;
  const stop = async () => {
    try {
      const next = await backend.stopOutput();
      setRunning(next.running);
    } catch (e) {
      useApp.setState({ error: errorMessage(e) });
    }
  };
  return (
    <span className="flex items-center gap-2 text-green-600 dark:text-green-400">
      <span className="h-2 w-2 animate-pulse rounded-full bg-green-500" />
      Live output
      <button
        type="button"
        aria-label="Stop live output"
        onClick={stop}
        className="rounded border border-current px-1.5 py-0.5 hover:bg-green-500/10"
      >
        Stop
      </button>
    </span>
  );
}

/** "No problems", "3 warnings", "1 error", or "1 error, 2 warnings": only what there is. */
export function problemCount(errors: number, warnings: number): string {
  if (errors === 0 && warnings === 0) return "No problems";
  return [errors ? plural(errors, "error") : "", warnings ? plural(warnings, "warning") : ""].filter(Boolean).join(", ");
}

function StatusBar() {
  const snapshot = useApp((s) => s.snapshot);
  const findMissingFiles = useApp((s) => s.findMissingFiles);
  const busy = useApp((s) => s.busy);
  const [open, setOpen] = useState(false);
  const missing = snapshot?.missingFiles ?? [];
  const issueCount = (snapshot?.issues.length ?? 0) + missing.length;

  useEffect(() => {
    if (!open) return;
    if (issueCount === 0) {
      setOpen(false);
      return;
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, issueCount]);

  if (!snapshot) return null;
  const { summary, issues } = snapshot;
  const errors = issues.filter((i) => i.severity === "error").length;
  // A file that isn't where it was is a warning: the show still opens and plays without it.
  const warnings = issues.length - errors + missing.length;
  return (
    <footer className="relative flex h-8 shrink-0 items-center gap-4 border-t border-neutral-200 px-3 text-xs text-neutral-500 dark:border-neutral-800">
      <span>
        {plural(summary.props, "prop")} · {thousands(summary.pixels)} pixels · {plural(summary.controllers, "controller")} ·{" "}
        {plural(summary.universes, "universe")}
      </span>
      <LiveOutput />
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        data-tip={issueCount === 0 ? "Nothing in the show needs fixing" : "Show what needs fixing"}
        className={`ml-auto flex h-7 items-center gap-1 rounded px-2 hover:bg-neutral-200/70 dark:hover:bg-neutral-800 ${
          errors ? "text-red-600 dark:text-red-400" : warnings ? "text-amber-600 dark:text-amber-400" : ""
        }`}
      >
        <AlertTriangle size={12} aria-hidden />
        {problemCount(errors, warnings)}
      </button>
      {open && issueCount > 0 && (
        <div
          role="dialog"
          aria-label="Problems"
          className="absolute right-2 bottom-9 z-20 max-h-80 w-[28rem] overflow-auto rounded-lg border border-neutral-200 bg-white p-3 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        >
          {missing.length > 0 && (
            <section aria-label="Missing files" className="mb-3 flex flex-col gap-2">
              <div className="flex items-center justify-between gap-2">
                <h3 className="font-medium text-amber-700 dark:text-amber-400">
                  {missing.length === 1 ? "1 file isn't where it was" : `${missing.length} files aren't where they were`}
                </h3>
                {missing.length > 1 && snapshot.path && (
                  <Button disabled={busy} onClick={() => void findMissingFiles()}>
                    <Search size={14} aria-hidden /> Find all missing files
                  </Button>
                )}
              </div>
              {missing.map((m) => (
                <MissingFileNotice key={`${m.file.kind}:${"id" in m.file ? m.file.id : ""}`} missing={m} showOwner />
              ))}
            </section>
          )}
          <ul className="flex flex-col gap-3">
            {issues.map((issue, i) => (
              <li key={i}>
                <span className={issue.severity === "error" ? "text-red-600 dark:text-red-400" : "text-amber-600 dark:text-amber-400"}>
                  {issue.severity === "error" ? "Error" : "Warning"}:
                </span>{" "}
                <span className="text-neutral-800 dark:text-neutral-200">{issue.message}</span>
                {issue.fix && <p className="mt-0.5 text-neutral-500">Fix: {issue.fix}</p>}
              </li>
            ))}
          </ul>
        </div>
      )}
    </footer>
  );
}

function CurrentScreen() {
  const screen = useApp((s) => s.screen);
  switch (screen) {
    case "layout":
      return <LayoutScreen />;
    case "wiring":
      return <WiringScreen />;
    case "devices":
      return <DevicesScreen />;
    case "sequence":
      return <SequenceScreen />;
    case "play":
      return <PlayScreen />;
    case "test":
      return <TestScreen />;
    case "history":
      return <HistoryScreen />;
  }
}

/** Opens and closes the assistant panel (⌘L). */
function AssistantButton() {
  const open = useAssistant((s) => s.open);
  const toggle = useAssistant((s) => s.toggle);
  return (
    <button
      type="button"
      aria-pressed={open}
      onClick={toggle}
      title="Assistant (⌘L)"
      className={`ml-1 flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-sm ${
        open
          ? "border-accent-500 bg-accent-50 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400"
          : "border-neutral-300 text-neutral-500 hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-800"
      }`}
    >
      <Sparkles size={14} aria-hidden /> Assistant <kbd className="text-xs">⌘L</kbd>
    </button>
  );
}

const WORK_SCREENS = new Set(["layout", "wiring", "play"]);

export function AppShell() {
  const screen = useApp((s) => s.screen);
  const assistantOpen = useAssistant((s) => s.open);
  // In a narrow window the assistant floats over the screen rather than squeezing it; on a laptop
  // it takes a narrower column (and the sidebar folds to icons).
  const band = useWindowBand();
  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <MissingFilesBanner />
      <div className="relative flex min-h-0 flex-1">
        <Sidebar />
        {/* Work screens give the room to their canvas or list; overview screens get more air. */}
        <main className={`min-w-0 flex-1 ${screen === "sequence" ? "overflow-hidden" : WORK_SCREENS.has(screen) ? "overflow-auto p-4" : "overflow-auto p-6"}`}>
          <CurrentScreen />
        </main>
        {assistantOpen && <AssistantPanel overlay={band === "narrow"} compact={band !== "wide"} />}
      </div>
      <StatusBar />
    </div>
  );
}
