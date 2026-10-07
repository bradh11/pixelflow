import { Command } from "cmdk";
import { useEffect } from "react";
import { fileName } from "../lib/format";
import { hintFor } from "../lib/shortcuts";
import { PROP_KINDS } from "../lib/shows";
import { addPropInView } from "../state/addProp";
import { useAssistant } from "../state/assistant";
import { saveFocused, undoFocused } from "../state/menuActions";
import { useSequencer } from "../state/sequencer";
import { useShortcutSheet } from "../state/shortcutSheet";
import { useView3d } from "../state/view3d";
import { setLayoutMode } from "./layout3d/useLayout3dKeys";
import { currentSetupKey, useSetup } from "../state/setup";
import { type Screen, useApp } from "../state/store";

interface Action {
  id: string;
  label: string;
  shortcut?: string;
  run: () => unknown;
}

/** ⌘K / Ctrl+K: search and run any command. */
export function CommandPalette() {
  const open = useApp((s) => s.paletteOpen);
  const setOpen = useApp((s) => s.setPaletteOpen);
  const state = useApp();
  const sequenceHasMusic = useSequencer((s) => Boolean(s.doc?.audio));
  const in3d = useView3d((s) => s.mode === "3d");

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  useEffect(() => {
    if (open) void useApp.getState().refreshRecent();
  }, [open]);

  if (!open) return null;

  const go = (screen: Screen, label: string): Action => ({
    id: `go-${screen}`,
    label: `Go to ${label}`,
    run: () => state.setScreen(screen),
  });
  const recent: Action[] = state.recent.map((show) => ({
    id: `recent-${show.path}`,
    label: `Open recent: ${show.name} (${fileName(show.path)})${show.status === "missing" ? " — moved, Locate…" : ""}`,
    run: () => (show.status === "missing" ? state.locateRecent(show.path) : state.openRecent(show.path)),
  }));
  const actions: Action[] = [
    { id: "new", label: "New show", shortcut: hintFor("new"), run: state.newShow },
    { id: "open", label: "Open show…", shortcut: hintFor("open"), run: state.openShow },
    { id: "open-recent", label: "Open recent show…", shortcut: hintFor("open-recent"), run: () => state.setShowMenu("recent") },
    { id: "close-show", label: "Close show", shortcut: hintFor("close-show"), run: state.closeShow },
    { id: "rename-show", label: "Rename show…", run: () => state.setRenaming(true) },
    { id: "clear-recent", label: "Clear recent shows", run: state.clearRecent },
    { id: "demo", label: "Try the demo show", run: state.openSample },
    { id: "import-xlights", label: "Import from xLights…", run: state.importXlights },
    { id: "import-xlights-sequence", label: "Import xLights sequence…", run: state.importXlightsSequence },
    { id: "save", label: "Save", shortcut: hintFor("save"), run: () => saveFocused(false) },
    { id: "save-as", label: "Save as…", shortcut: hintFor("save-as"), run: state.saveAs },
    { id: "undo", label: "Undo", shortcut: hintFor("undo"), run: () => undoFocused(false) },
    { id: "redo", label: "Redo", shortcut: hintFor("redo"), run: () => undoFocused(true) },
    {
      id: "assistant",
      label: useAssistant.getState().open ? "Close the assistant" : "Open the assistant",
      shortcut: hintFor("assistant"),
      run: useAssistant.getState().toggle,
    },
    { id: "shortcuts", label: "Keyboard shortcuts", shortcut: hintFor("shortcuts"), run: () => useShortcutSheet.getState().setOpen(true) },
    { id: "ai-settings", label: "AI settings…", run: () => useAssistant.getState().setSettingsOpen(true) },
    { id: "setup", label: "Show the setup checklist", run: () => useSetup.getState().setDismissed(currentSetupKey(), false) },
    go("layout", "Layout"),
    go("devices", "Controllers"),
    go("wiring", "Wiring"),
    go("test", "Test"),
    go("sequence", "Sequence"),
    go("play", "Play"),
    go("history", "History"),
    {
      id: "open-sequence",
      label: "Open sequence…",
      run: () => {
        state.setScreen("sequence");
        const sequencer = useSequencer.getState();
        return sequencer.replaceAfterAsking(async () => {
          const path = await useSequencer.getState().api?.pickSequenceDocPath();
          if (path) await useSequencer.getState().open(path);
        });
      },
    },
    ...(sequenceHasMusic ? [{ id: "detect-beats", label: "Detect beats (find the beats and bars)", run: () => useSequencer.getState().detectBeats() }] : []),
    {
      id: "scan",
      label: "Scan the network for controllers",
      run: () => {
        state.setScreen("devices");
        return state.scan();
      },
    },
    { id: "stop-output", label: "Stop live output (test pattern or playback to the lights)", run: () => state.backend?.stopOutput() },
    {
      id: "layout-mode",
      label: in3d ? "Layout: switch to the 2D view" : "Layout: switch to the 3D view",
      shortcut: hintFor("layout-mode"),
      run: () => {
        state.setScreen("layout");
        setLayoutMode(in3d ? "2d" : "3d");
      },
    },
    ...PROP_KINDS.map(({ kind, label }) => ({
      id: `add-${kind}`,
      label: `Add prop: ${label}`,
      run: () => void addPropInView(kind),
    })),
    ...(["light", "dark", "system"] as const).map((choice) => ({
      id: `theme-${choice}`,
      label: `Theme: ${choice === "system" ? "match this computer" : choice}${state.themeChoice === choice ? " (now)" : ""}`,
      run: () => state.setTheme(choice),
    })),
  ];

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center bg-black/40 pt-[15vh]" onClick={() => setOpen(false)}>
      <Command
        label="Commands"
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-lg overflow-hidden rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <Command.Input
          autoFocus
          placeholder="Type a command…"
          className="w-full border-b border-neutral-200 bg-transparent px-4 py-3 text-sm outline-none dark:border-neutral-800"
        />
        <Command.List className="max-h-80 overflow-auto p-2">
          <Command.Empty className="px-3 py-6 text-center text-sm text-neutral-500">No matching commands.</Command.Empty>
          {[
            { heading: "Commands", list: actions },
            { heading: "Open Recent", list: recent },
          ].map(({ heading, list }) =>
            list.length === 0 ? null : (
              <Command.Group
                key={heading}
                heading={heading}
                className="[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-neutral-500"
              >
                {list.map((action) => (
                  <Command.Item
                    key={action.id}
                    value={action.label}
                    onSelect={() => {
                      setOpen(false);
                      void action.run();
                    }}
                    className="flex cursor-pointer items-center justify-between rounded-md px-3 py-2 text-sm data-[selected=true]:bg-accent-50 data-[selected=true]:text-accent-600 dark:data-[selected=true]:bg-accent-600/15 dark:data-[selected=true]:text-accent-400"
                  >
                    {action.label}
                    {action.shortcut && <kbd className="text-xs text-neutral-400">{action.shortcut}</kbd>}
                  </Command.Item>
                ))}
              </Command.Group>
            ),
          )}
        </Command.List>
      </Command>
    </div>
  );
}
