import {
  AlertTriangle,
  Cable,
  Command,
  History,
  LayoutGrid,
  Moon,
  Network,
  Play,
  Redo2,
  Save,
  Sun,
  Undo2,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { errorMessage } from "../api/backend";
import { fileName, plural, thousands } from "../lib/format";
import { type Screen, useApp } from "../state/store";
import { DevicesScreen } from "../screens/DevicesScreen";
import { HistoryScreen } from "../screens/HistoryScreen";
import { LayoutScreen } from "../screens/LayoutScreen";
import { TestScreen } from "../screens/TestScreen";
import { WiringScreen } from "../screens/WiringScreen";

const NAV: { screen: Screen; label: string; icon: ReactNode }[] = [
  { screen: "layout", label: "Layout", icon: <LayoutGrid size={18} /> },
  { screen: "wiring", label: "Wiring", icon: <Cable size={18} /> },
  { screen: "devices", label: "Devices", icon: <Network size={18} /> },
  { screen: "test", label: "Test", icon: <Play size={18} /> },
  { screen: "history", label: "History", icon: <History size={18} /> },
];

function IconButton({ label, onClick, disabled, children }: { label: string; onClick: () => void; disabled?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className="rounded-md p-2 text-neutral-600 hover:bg-neutral-200/70 disabled:opacity-30 disabled:hover:bg-transparent dark:text-neutral-300 dark:hover:bg-neutral-800"
    >
      {children}
    </button>
  );
}

function TopBar() {
  const snapshot = useApp((s) => s.snapshot);
  const { undo, redo, save, setPaletteOpen, theme, setTheme } = useApp();
  if (!snapshot) return null;
  const title = snapshot.show.name;
  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-neutral-200 px-3 dark:border-neutral-800">
      <span className="font-semibold text-accent-600 dark:text-accent-400">PixelFlow</span>
      <span className="text-neutral-300 dark:text-neutral-700">/</span>
      <span className="truncate font-medium" title={snapshot.path ?? undefined}>
        {title}
      </span>
      {snapshot.dirty && (
        <span className="text-xs text-neutral-500" aria-label="Unsaved changes">
          ● Unsaved
        </span>
      )}
      {snapshot.path && <span className="hidden truncate text-xs text-neutral-500 lg:inline">{fileName(snapshot.path)}</span>}
      <div className="ml-auto flex items-center gap-1">
        <IconButton label="Undo" onClick={undo} disabled={!snapshot.canUndo}>
          <Undo2 size={18} />
        </IconButton>
        <IconButton label="Redo" onClick={redo} disabled={!snapshot.canRedo}>
          <Redo2 size={18} />
        </IconButton>
        <IconButton label="Save" onClick={save}>
          <Save size={18} />
        </IconButton>
        <IconButton label={theme === "dark" ? "Light theme" : "Dark theme"} onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
          {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
        </IconButton>
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
  const [open, setOpen] = useState(false);
  const issueCount = snapshot?.issues.length ?? 0;

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
  const warnings = issues.length - errors;
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
        {issues.length === 0 ? "No problems" : `${plural(errors, "error")}, ${plural(warnings, "warning")}`}
      </button>
      {open && issues.length > 0 && (
        <div
          role="dialog"
          aria-label="Problems"
          className="absolute right-2 bottom-9 z-20 max-h-80 w-[28rem] overflow-auto rounded-lg border border-neutral-200 bg-white p-3 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        >
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
    case "test":
      return <TestScreen />;
    case "history":
      return <HistoryScreen />;
  }
}

export function AppShell() {
  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="min-w-0 flex-1 overflow-auto p-6">
          <CurrentScreen />
        </main>
      </div>
      <StatusBar />
    </div>
  );
}
