import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { DEMO_PHOTO, demoDevices, demoHousePhoto, demoMissingFiles, demoPlayers, demoShow } from "./api/demo";
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
} else {
  const demo = new URLSearchParams(location.search).has("demo");
  const backend = new MemoryBackend(demo ? demoShow() : undefined);
  if (demo) {
    backend.deviceNetwork = demoDevices();
    backend.fppPlayers = demoPlayers();
    backend.nextSequencePath = "/Shows/Christmas Medley 2017.fseq";
    backend.images.set(DEMO_PHOTO, demoHousePhoto());
    backend.nextImagePath = DEMO_PHOTO;
    // `?demo&missing`: the show as if its folder had moved, with files to find again.
    if (new URLSearchParams(location.search).has("missing")) demoMissingFiles(backend);
  }
  void useApp.getState().connect(backend);
  const sequencer = new MemorySequencer(backend);
  if (demo) {
    // A sample sequence, open on the Sequence screen.
    backend.nextAudioPath = DEMO_MUSIC;
    sequencer.files.set(DEMO_SEQUENCE_PATH, demoSequence(backend.show, backend.sequenceDurationMs, { singing: true }));
    sequencer.nextOpenPath = DEMO_SEQUENCE_PATH;
    sequencer.nextSavePath = DEMO_SEQUENCE_PATH;
    void sequencer.openSequenceDoc(DEMO_SEQUENCE_PATH).then(() => useSequencer.getState().connect(sequencer));
    useApp.setState({ started: true });
  } else {
    void useSequencer.getState().connect(sequencer);
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
