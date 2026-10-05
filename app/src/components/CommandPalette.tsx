import { Command } from "cmdk";
import { useEffect } from "react";
import { besideOthers } from "../lib/layoutEdits";
import { PROP_KINDS, newProp } from "../lib/shows";
import { type Screen, useApp } from "../state/store";

interface Action {
  id: string;
  label: string;
  shortcut?: string;
  run: () => void;
}

/** ⌘K / Ctrl+K: search and run any command. */
export function CommandPalette() {
  const open = useApp((s) => s.paletteOpen);
  const setOpen = useApp((s) => s.setPaletteOpen);
  const state = useApp();

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  if (!open) return null;

  const go = (screen: Screen, label: string): Action => ({
    id: `go-${screen}`,
    label: `Go to ${label}`,
    run: () => state.setScreen(screen),
  });
  const actions: Action[] = [
    { id: "new", label: "New show", shortcut: "⌘N", run: state.newShow },
    { id: "open", label: "Open show…", shortcut: "⌘O", run: state.openShow },
    { id: "import-xlights", label: "Import from xLights…", run: state.importXlights },
    { id: "import-xlights-sequence", label: "Import xLights sequence…", run: state.importXlightsSequence },
    { id: "save", label: "Save", shortcut: "⌘S", run: state.save },
    { id: "save-as", label: "Save as…", shortcut: "⇧⌘S", run: state.saveAs },
    { id: "undo", label: "Undo", shortcut: "⌘Z", run: state.undo },
    { id: "redo", label: "Redo", shortcut: "⇧⌘Z", run: state.redo },
    go("layout", "Layout"),
    go("wiring", "Wiring"),
    go("devices", "Devices"),
    go("test", "Test"),
    go("history", "History"),
    ...PROP_KINDS.map(({ kind, label }) => ({
      id: `add-${kind}`,
      label: `Add prop: ${label}`,
      run: () => {
        const show = useApp.getState().snapshot?.show;
        if (show) void state.apply([{ type: "addProp", prop: besideOthers(newProp(kind, show), show) }]);
      },
    })),
    {
      id: "theme",
      label: state.theme === "dark" ? "Switch to light theme" : "Switch to dark theme",
      run: () => state.setTheme(state.theme === "dark" ? "light" : "dark"),
    },
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
          {actions.map((action) => (
            <Command.Item
              key={action.id}
              value={action.label}
              onSelect={() => {
                setOpen(false);
                action.run();
              }}
              className="flex cursor-pointer items-center justify-between rounded-md px-3 py-2 text-sm data-[selected=true]:bg-accent-50 data-[selected=true]:text-accent-600 dark:data-[selected=true]:bg-accent-600/15 dark:data-[selected=true]:text-accent-400"
            >
              {action.label}
              {action.shortcut && <kbd className="text-xs text-neutral-400">{action.shortcut}</kbd>}
            </Command.Item>
          ))}
        </Command.List>
      </Command>
    </div>
  );
}
