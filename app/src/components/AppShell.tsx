import {
  AlertTriangle,
  Search,
  AudioLines,
  Cable,
  Command,
  Film,
  FlaskConical,
  History,
  LayoutGrid,
  Moon,
  Network,
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
import { type Screen, useApp } from "../state/store";
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
import { Button, UnsavedBadge } from "./ui";
import { saveFocused } from "../state/menuActions";

const NAV: { screen: Screen; label: string; icon: ReactNode }[] = [
  { screen: "layout", label: "Layout", icon: <LayoutGrid size={18} /> },
  { screen: "wiring", label: "Wiring", icon: <Cable size={18} /> },
  { screen: "devices", label: "Devices", icon: <Network size={18} /> },
  { screen: "sequence", label: "Sequence", icon: <AudioLines size={18} /> },
  { screen: "play", label: "Play", icon: <Film size={18} /> },
  { screen: "test", label: "Test", icon: <FlaskConical size={18} /> },
  { screen: "history", label: "History", icon: <History size={18} /> },
];

function IconButton({ label, onClick, disabled, dim, children }: { label: string; onClick: () => void; disabled?: boolean; dim?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-md p-2 text-neutral-600 hover:bg-neutral-200/70 disabled:opacity-30 disabled:hover:bg-transparent dark:text-neutral-300 dark:hover:bg-neutral-800 ${dim ? "opacity-40 hover:opacity-100" : ""}`}
    >
      {children}
    </button>
  );
}

/** On the Sequence screen, undo and redo act on the open sequence; elsewhere on the show.
 * (Only what the buttons need is watched, so playback doesn't redraw the top bar.) */
function useUndoTarget() {
  const onSequence = useApp((s) => s.screen === "sequence");
  const hasSequence = useSequencer((s) => s.doc !== null);
  const seq = useSequencer(useShallow((s) => ({ canUndo: s.canUndo, canRedo: s.canRedo })));
  const show = useApp(useShallow((s) => ({ canUndo: s.snapshot?.canUndo ?? false, canRedo: s.snapshot?.canRedo ?? false })));
  if (onSequence && hasSequence) {
    const { undo, redo } = useSequencer.getState();
    return { sequence: true, undo, redo, ...seq };
  }
  const { undo, redo } = useApp.getState();
  return { sequence: false, undo, redo, ...show };
}

function TopBar() {
  const snapshot = useApp((s) => s.snapshot);
  const theme = useApp((s) => s.theme);
  const { setPaletteOpen, setTheme, closeShow } = useApp.getState();
  const target = useUndoTarget();
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
        <IconButton label={target.sequence ? "Undo (sequence)" : "Undo"} onClick={target.undo} disabled={!target.canUndo}>
          <Undo2 size={18} />
        </IconButton>
        <IconButton label={target.sequence ? "Redo (sequence)" : "Redo"} onClick={target.redo} disabled={!target.canRedo}>
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

function Sidebar() {
  const screen = useApp((s) => s.screen);
  const setScreen = useApp((s) => s.setScreen);
  const toRecover = useSequencer((s) => s.recoveries.length > 0);
  return (
    <nav aria-label="Screens" className="flex w-44 shrink-0 flex-col gap-1 border-r border-neutral-200 p-2 dark:border-neutral-800">
      {NAV.map((item) => (
        <button
          key={item.screen}
          type="button"
          aria-current={screen === item.screen ? "page" : undefined}
          onClick={() => setScreen(item.screen)}
          className={`flex items-center gap-2 rounded-md px-3 py-2 text-sm ${
            screen === item.screen
              ? "bg-accent-50 font-medium text-accent-600 dark:bg-accent-600/15 dark:text-accent-400"
              : "text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
          }`}
        >
          {item.icon}
          {item.label}
          {item.screen === "sequence" && toRecover && (
            <span className="ml-auto h-2 w-2 rounded-full bg-amber-500" title="Unsaved work to recover" aria-hidden />
          )}
        </button>
      ))}
    </nav>
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
        className={`ml-auto flex items-center gap-1 rounded px-2 py-0.5 ${
          errors ? "text-red-600 dark:text-red-400" : warnings ? "text-amber-600 dark:text-amber-400" : ""
        }`}
      >
        <AlertTriangle size={12} />
        {issueCount === 0 ? "No problems" : `${plural(errors, "error")}, ${plural(warnings, "warning")}`}
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

export function AppShell() {
  const screen = useApp((s) => s.screen);
  const assistantOpen = useAssistant((s) => s.open);
  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <MissingFilesBanner />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className={`min-w-0 flex-1 ${screen === "sequence" ? "overflow-hidden" : "overflow-auto p-6"}`}>
          <CurrentScreen />
        </main>
        {assistantOpen && <AssistantPanel />}
      </div>
      <StatusBar />
    </div>
  );
}
