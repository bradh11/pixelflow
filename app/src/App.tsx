import { useEffect } from "react";
import { AppShell } from "./components/AppShell";
import { CommandPalette } from "./components/CommandPalette";
import { ConfirmClose, ConfirmDiscard, ConfirmReplaceSequence } from "./components/ConfirmDiscard";
import { ErrorBanner } from "./components/ErrorBanner";
import { ImportReport } from "./components/ImportReport";
import { FilesReport } from "./components/MissingFiles";
import { SequenceImportReport } from "./components/SequenceImportReport";
import { Welcome } from "./components/Welcome";
import { useShortcuts } from "./components/useShortcuts";
import { useCloseGuard } from "./state/closeGuard";
import { useApp } from "./state/store";

export function App() {
  const started = useApp((s) => s.started);
  const theme = useApp((s) => s.theme);
  const backend = useApp((s) => s.backend);
  useShortcuts();
  // Closing the window with unsaved work asks first.
  useEffect(() => {
    if (!backend) return;
    let stop: (() => void) | null = null;
    let gone = false;
    void backend.onCloseRequested(() => useCloseGuard.getState().request()).then(
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
      <ConfirmDiscard />
      <ConfirmReplaceSequence />
      <ConfirmClose />
      <ErrorBanner />
      <ImportReport />
      <FilesReport />
      <SequenceImportReport />
    </>
  );
}
