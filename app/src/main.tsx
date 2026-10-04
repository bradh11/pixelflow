import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { demoShow } from "./api/demo";
import { MemoryBackend } from "./api/memory";
import { inTauri, tauriBackend } from "./api/tauri";
import { useApp } from "./state/store";
import "./styles.css";

// Inside the desktop app, talk to the real engine. In a plain browser (UI development), use
// the in-memory stand-in; `?demo` opens a sample show.
if (inTauri()) {
  void useApp.getState().connect(tauriBackend);
} else {
  const demo = new URLSearchParams(location.search).has("demo");
  void useApp.getState().connect(new MemoryBackend(demo ? demoShow() : undefined));
  if (demo) useApp.setState({ started: true });
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
