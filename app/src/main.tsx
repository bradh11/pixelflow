import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { tauriAssistant } from "./api/assistant";
import { FakeAssistant } from "./api/memoryAssistant";
import { useAssistant } from "./state/assistant";
import { DEMO_PHOTO, DEMO_PICTURE, DEMO_SHOW_PATH, demoDevices, demoFppFileDetails, demoFppFiles, demoFppSchedules, demoFppSoftware, demoHousePhoto, demoMissingFiles, demoPicture, demoPlayers, demoRecentShows, demoShow, demoShowDevices } from "./api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "./api/demoSequence";
import { MemoryBackend } from "./api/memory";
import { MemorySequencer } from "./api/memorySequencer";
import { DEMO_VENDOR_PATH, demoVendorPackage } from "./api/memoryVendor";
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
    // Reading music takes a moment, with progress along the way; `?demo&slowmusic`: music files
    // that don't say how long they are, so New sequence reads them through too.
    backend.musicReadMs = 1200;
    backend.probeFromHeader = !new URLSearchParams(location.search).has("slowmusic");
    // Camera mapping offers a made-up video of the lights flashing.
    backend.cameraMapSamples = true;
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
    assistant.alignDownloadMs = 4000;
    if (!useAssistant.getState().models.anthropic) useAssistant.getState().setModel("claude-opus-5-5");
  }
  void useAssistant.getState().connect(assistant);
  const sequencer = new MemorySequencer(backend);
  assistant.sequencer = sequencer;
  // Find lyrics waits for the assistant to be set up, like the app.
  sequencer.hasAssistantKey = (provider) => assistant.keys.has(provider);
  if (demo) {
    sequencer.lyricsStepMs = 600;
    sequencer.analysisDelayMs = 1500;
    sequencer.audioTrackMs = 2500;
    // A video export takes a few seconds, as if ffmpeg were installed too.
    sequencer.video.stepMs = 150;
    sequencer.video.ffmpeg = "x264";
  }
  if (demo) {
    // A sample sequence, open on the Sequence screen.
    backend.nextAudioPath = DEMO_MUSIC;
    sequencer.files.set(DEMO_SEQUENCE_PATH, demoSequence(backend.show, backend.sequenceDurationMs, { singing: true }));
    sequencer.nextOpenPath = DEMO_SEQUENCE_PATH;
    sequencer.nextSavePath = DEMO_SEQUENCE_PATH;
    // A vendor's sequence to import and map onto the demo house.
    // A picture in the show's images folder, and the same one for the dialog to hand back.
    sequencer.pictures.set("images/Snowman.svg", demoPicture());
    backend.images.set(DEMO_PICTURE, demoPicture());
    sequencer.nextPicturePath = DEMO_PICTURE;
    sequencer.vendorPackages.set(DEMO_VENDOR_PATH, demoVendorPackage());
    sequencer.nextXlightsSequencePath = DEMO_VENDOR_PATH;
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
