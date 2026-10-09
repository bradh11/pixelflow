// Video export in memory, for tests and the plain-browser demo: it checks the request like the
// desktop shell, then walks through the export's stages reporting progress (slowly enough to see
// in the demo), and answers with the summary the engine would. No file is written.

import { fileName } from "../lib/format";
import type { Sequence } from "./sequence";
import type { VideoApi, VideoChoices, VideoProgress, VideoRequest, VideoStage, VideoSummary } from "./video";

const STAGES: { stage: VideoStage; label: string; steps: number }[] = [
  { stage: "rendering", label: "Rendering frames", steps: 20 },
  { stage: "sound", label: "Encoding the sound", steps: 5 },
  { stage: "writing", label: "Writing the MP4", steps: 5 },
];

function fail(message: string): never {
  throw new Error(message);
}

export class MemoryVideo implements VideoApi {
  /** What the dialog is offered (ffmpeg's encoder name when "installed"). */
  ffmpeg: string | null = null;
  /** How long each progress step takes (0 in tests). */
  stepMs = 0;
  /** What the save dialog returns: by default a file in the Movies folder named as asked. */
  nextPath: string | null | undefined = undefined;
  /** The requests exported, for test assertions. */
  requests: { path: string; request: VideoRequest }[] = [];
  private cancels = 0;

  constructor(
    private readonly doc: () => Sequence | null,
    private readonly hasPhoto: () => boolean,
  ) {}

  async videoExportChoices(): Promise<VideoChoices> {
    return { ffmpeg: this.ffmpeg, photo: this.hasPhoto() };
  }

  async exportVideo(path: string, request: VideoRequest, onProgress?: (progress: VideoProgress) => void): Promise<VideoSummary> {
    const doc = this.doc() ?? fail("No sequence is open. Create or open one first.");
    const sizes = (request.width === 1280 && request.height === 720) || (request.width === 1920 && request.height === 1080);
    if (!sizes) fail("Videos are 720p (1280×720) or 1080p (1920×1080).");
    if (request.fps !== 30 && request.fps !== 60) fail("Videos are 30 or 60 frames a second.");
    if (request.ffmpeg && !this.ffmpeg) fail("ffmpeg isn't installed any more. Export with the built-in encoder instead.");
    const endMs = Math.min(request.endMs ?? doc.durationMs, doc.durationMs);
    const frames = Math.ceil((Math.max(0, endMs - request.startMs) * request.fps) / 1000);
    if (frames === 0) fail("The range to export is empty. Choose a start before its end, inside the sequence.");
    this.requests.push({ path, request });
    const started = this.cancels;
    const stages = STAGES.filter((s) => s.stage !== "sound" || (doc.audio && !request.ffmpeg));
    for (const { stage, label, steps } of stages) {
      for (let step = 0; step <= steps; step++) {
        onProgress?.({ path, stage, label, fraction: step / steps });
        await new Promise((resolve) => setTimeout(resolve, this.stepMs));
        if (this.cancels !== started) fail("The export was cancelled.");
      }
    }
    const durationMs = Math.floor((frames * 1000) / request.fps);
    const notes = doc.audio ? [] : ["The sequence has no music, so the video has no sound."];
    return {
      frames,
      durationMs,
      width: request.width,
      height: request.height,
      fps: request.fps,
      // About the size the built-in encoder makes.
      bytes: Math.round((durationMs / 1000) * request.width * request.height * request.fps * 0.0113),
      video: request.ffmpeg ? `H.264 (ffmpeg, ${this.ffmpeg})` : "H.264 (OpenH264)",
      sound: doc.audio ? (request.ffmpeg ? "AAC 256 kb/s (ffmpeg)" : "AAC 192 kb/s") : null,
      elapsedMs: stages.length * this.stepMs,
      notes,
    };
  }

  async cancelVideoExport() {
    this.cancels++;
  }

  async pickVideoPath(defaultName: string) {
    return this.nextPath === undefined ? `/Users/you/Movies/${fileName(defaultName)}` : this.nextPath;
  }
}
