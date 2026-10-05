import { useEffect } from "react";
import { AppShell } from "./components/AppShell";
import { CommandPalette } from "./components/CommandPalette";
import { ConfirmDiscard } from "./components/ConfirmDiscard";
import { ErrorBanner } from "./components/ErrorBanner";
import { Welcome } from "./components/Welcome";
import { useShortcuts } from "./components/useShortcuts";
import { useApp } from "./state/store";

export function App() {
  const started = useApp((s) => s.started);
  const theme = useApp((s) => s.theme);
  useShortcuts();
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  return (
    <>
      {started ? <AppShell /> : <Welcome />}
      <CommandPalette />
      <ConfirmDiscard />
      <ErrorBanner />
    </>
  );
}
