import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { tauriAssistant } from "./api/assistant";
import { FakeAssistant } from "./api/memoryAssistant";
import { useAssistant } from "./state/assistant";
import { DEMO_PHOTO, DEMO_SHOW_PATH, demoDevices, demoFppFileDetails, demoFppFiles, demoFppSchedules, demoFppSoftware, demoHousePhoto, demoMissingFiles, demoPlayers, demoRecentShows, demoShow, demoShowDevices } from "./api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "./api/demoSequence";
import { MemoryBackend } from "./api/memory";
import { MemorySequencer } from "./api/memorySequencer";
import { tauriSequencer } from "./api/sequencer";
import { inTauri, tauriBackend } from "./api/tauri";
import { useSequencer } from "./state/sequencer";
import { useApp } from "./state/store";
import "./styles.css";

// Inside the desktop app, talk to the real engine. In a plain browser (UI development), use
// the in-memory stand-in; `?demo` opens a sample show.
if (inTauri()) {
  void useApp.getState().connect(tauriBackend);
  void useSequencer.getState().connect(tauriSequencer);
  void useAssistant.getState().connect(tauriAssistant);
} else {
  const demo = new URLSearchParams(location.search).has("demo");
  const backend = new MemoryBackend(demo ? demoShow() : undefined);
  if (demo) {
    backend.deviceNetwork = demoDevices();
    backend.fppPlayers = demoPlayers();
    // The demo show's own FPP answers too, set up a little differently from the show.
    const own = demoShowDevices(backend.show);
    backend.deviceNetwork.details.push(...own.details);
    Object.assign(backend.fppPlayers, own.players);
    backend.fppFiles = demoFppFiles();
    backend.fppFileDetails = demoFppFileDetails();
    backend.fppSchedules = demoFppSchedules();
    backend.fppSoftwares = demoFppSoftware();
    backend.openUrl = (url) => void window.open(url, "_blank", "noopener");
    backend.fppSendStepMs = 150;
    backend.nextSequencePath = "/Shows/Christmas Medley 2017.fseq";
    backend.images.set(DEMO_PHOTO, demoHousePhoto());
    backend.nextImagePath = DEMO_PHOTO;
    // `?demo&missing`: the show as if its folder had moved, with files to find again.
    if (new URLSearchParams(location.search).has("missing")) demoMissingFiles(backend);
    // Saved, with a few recent shows (one of them moved).
    backend.path = DEMO_SHOW_PATH;
    demoRecentShows(backend);
  }
  void useApp.getState().connect(backend);
  // A scripted stand-in assistant: no key or network. In the demo it's already set up.
  const assistant = new FakeAssistant(backend);
  if (demo) {
    assistant.keys.set("anthropic", "keychain");
    assistant.delayMs = 20;
    if (!useAssistant.getState().models.anthropic) useAssistant.getState().setModel("claude-opus-5-5");
  }
  void useAssistant.getState().connect(assistant);
  const sequencer = new MemorySequencer(backend);
  assistant.sequencer = sequencer;
  if (demo) {
    // A sample sequence, open on the Sequence screen.
    backend.nextAudioPath = DEMO_MUSIC;
    sequencer.files.set(DEMO_SEQUENCE_PATH, demoSequence(backend.show, backend.sequenceDurationMs, { singing: true }));
    sequencer.nextOpenPath = DEMO_SEQUENCE_PATH;
    sequencer.nextSavePath = DEMO_SEQUENCE_PATH;
    void sequencer.openSequenceDoc(DEMO_SEQUENCE_PATH).then(() => useSequencer.getState().connect(sequencer));
    // `?demo&start`: the start page, with the recent shows.
    if (!new URLSearchParams(location.search).has("start")) useApp.setState({ started: true });
  } else {
    void useSequencer.getState().connect(sequencer);
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
