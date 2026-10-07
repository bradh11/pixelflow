import { useEffect } from "react";
import { AppShell } from "./components/AppShell";
import { AiSettings } from "./components/assistant/AiSettings";
import { DraftPreview } from "./components/assistant/DraftPreview";
import { CommandPalette } from "./components/CommandPalette";
import { ContextMenuLayer } from "./components/ContextMenu";
import { ShortcutSheet } from "./components/ShortcutSheet";
import { ConfirmClose, ConfirmDiscard, ConfirmReplaceSequence } from "./components/ConfirmDiscard";
import { ErrorBanner } from "./components/ErrorBanner";
import { ImportReport } from "./components/ImportReport";
import { FilesReport } from "./components/MissingFiles";
import { SequenceImportReport } from "./components/SequenceImportReport";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { Toasts } from "./components/Toasts";
import { TooltipLayer } from "./components/Tooltip";
import { NameShowDialog, OpeningStatus } from "./components/ShowDialogs";
import { Welcome } from "./components/Welcome";
import { useModalFocus } from "./components/useModalFocus";
import { useShortcuts } from "./components/useShortcuts";
import { requestWindowClose } from "./state/busy";
import { runMenuAction } from "./state/menuActions";
import { systemTheme, useApp } from "./state/store";

export function App() {
  const started = useApp((s) => s.started);
  const theme = useApp((s) => s.theme);
  const backend = useApp((s) => s.backend);
  useShortcuts();
  useModalFocus();
  // Closing the window with unsaved work asks first.
  useEffect(() => {
    if (!backend) return;
    let stop: (() => void) | null = null;
    let gone = false;
    void backend.onCloseRequested(requestWindowClose).then(
      (unlisten) => (gone ? unlisten() : (stop = unlisten)),
      () => undefined,
    );
    return () => {
      gone = true;
      stop?.();
    };
  }, [backend]);
  // The menu bar's File menu (macOS) runs the same actions as the show menu.
  useEffect(() => {
    if (!backend) return;
    let stop: (() => void) | null = null;
    let gone = false;
    void backend.onMenu((action) => void runMenuAction(action)).then(
      (unlisten) => (gone ? unlisten() : (stop = unlisten)),
      () => undefined,
    );
    return () => {
      gone = true;
      stop?.();
    };
  }, [backend]);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  // Following the computer's setting: change along with it.
  const themeChoice = useApp((s) => s.themeChoice);
  useEffect(() => {
    if (themeChoice !== "system" || typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(prefers-color-scheme: light)");
    const follow = () => useApp.setState({ theme: systemTheme() });
    query.addEventListener("change", follow);
    return () => query.removeEventListener("change", follow);
  }, [themeChoice]);
  // Coming back to PixelFlow: files may have come back or gone away meanwhile.
  useEffect(() => {
    const onFocus = () => void useApp.getState().checkFiles(true);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);
  return (
    <>
      {started ? <AppShell /> : <Welcome />}
      <CommandPalette />
      <ShortcutSheet />
      <ConfirmDiscard />
      <ConfirmReplaceSequence />
      <ConfirmClose />
      <ErrorBanner />
      <ImportReport />
      <FilesReport />
      <SequenceImportReport />
      <AiSettings />
      <DraftPreview />
      <Toasts />
      <ConfirmDialog />
      <NameShowDialog />
      <OpeningStatus />
      <ContextMenuLayer />
      <TooltipLayer />
    </>
  );
}
