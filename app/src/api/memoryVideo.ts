// Video export in memory, for tests and the plain-browser demo: it checks the request like the
// desktop shell, then walks through the export's stages reporting progress (slowly enough to see
// in the demo), and answers with the summary the engine would. No file is written. Cisco's
// OpenH264 encoder starts not downloaded, as in a new install, and its download is pretended.

import { fileName } from "../lib/format";
import type { Sequence } from "./sequence";
import type { H264Encoder, H264Progress, H264State, VideoApi, VideoChoices, VideoProgress, VideoRequest, VideoStage, VideoSummary } from "./video";

/** Cisco's licence for its binary, shortened (the shell sends it word for word). */
const CISCO_LICENCE = `About The Cisco-Provided Binary of OpenH264 Video Codec

Cisco provides this program under the terms of the BSD license.

Additionally, this binary is licensed under Cisco’s AVC/H.264 Patent Portfolio License from MPEG LA, at no cost to you, provided that the requirements and conditions shown below in the AVC/H.264 Patent Portfolio sections are met.

…

3. Third party software, in the location where end users can control the use of the Cisco-provided binary, must display the following text:

       "OpenH264 Video Codec provided by Cisco Systems, Inc."`;

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
  /** Cisco's encoder: not downloaded at first, as in a new install. */
  h264State: H264State = "missing";
  /** How long the encoder's download takes (0 in tests). */
  h264DownloadMs = 0;
  /** What the next download fails with, once (a sentence like the shell's), if anything. */
  h264Failure: string | null = null;
  /** How many times the encoder was downloaded, for test assertions. */
  h264Downloads = 0;
  private h264Started = 0;
  private h264Downloading = false;

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
    if (!request.ffmpeg && this.h264State === "missing") fail("Cisco's OpenH264 video encoder isn't downloaded yet.");
    if (!request.ffmpeg && this.h264State === "off") fail("Cisco's OpenH264 video encoder is turned off in Settings, under Video export.");
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

  private h264View(): H264Encoder {
    const supported = this.h264State !== "unsupported";
    return {
      state: this.h264State,
      downloading: this.h264Downloading,
      version: "2.6.0",
      host: "ciscobinary.openh264.org",
      url: supported ? "https://ciscobinary.openh264.org/libopenh264-2.6.0-mac-arm64.dylib.bz2" : null,
      bytes: supported ? 482_124 : 0,
      attribution: "OpenH264 Video Codec provided by Cisco Systems, Inc.",
      licence: CISCO_LICENCE,
      licenceUrl: "https://www.openh264.org/BINARY_LICENSE.txt",
      problem: supported ? null : "Cisco doesn't make its OpenH264 encoder for this system (linux, riscv64), so videos can only be exported with ffmpeg here.",
    };
  }

  async h264Encoder() {
    return this.h264View();
  }

  /** A pretend download: progress over h264DownloadMs, stopped by cancelH264Download. */
  async downloadH264Encoder(onProgress?: (progress: H264Progress) => void) {
    if (this.h264State === "unsupported") fail(this.h264View().problem ?? "");
    if (this.h264Downloading) fail("The encoder is already downloading.");
    const total = this.h264View().bytes;
    const started = ++this.h264Started;
    this.h264Downloading = true;
    try {
      const parts = Math.max(1, Math.round(this.h264DownloadMs / 100));
      for (let i = 0; i <= parts; i++) {
        if (started !== this.h264Started) fail("Stopped.");
        if (i === Math.ceil(parts / 2) && this.h264Failure) {
          const failure = this.h264Failure;
          this.h264Failure = null;
          fail(failure);
        }
        onProgress?.({ received: Math.round((total * i) / parts), total });
        if (i < parts && this.h264DownloadMs > 0) await new Promise((r) => setTimeout(r, this.h264DownloadMs / parts));
      }
      if (started !== this.h264Started) fail("Stopped.");
      this.h264State = "ready";
      this.h264Downloads++;
    } finally {
      if (started === this.h264Started) this.h264Downloading = false;
    }
    return this.h264View();
  }

  async cancelH264Download() {
    if (!this.h264Downloading) return;
    this.h264Started++;
    this.h264Downloading = false;
  }

  async setH264EncoderEnabled(on: boolean) {
    if (this.h264State === "ready" || this.h264State === "off") this.h264State = on ? "ready" : "off";
    return this.h264View();
  }

  async removeH264Encoder() {
    if (this.h264Downloading) fail("Wait for the download to finish, or stop it, first.");
    if (this.h264State !== "unsupported") this.h264State = "missing";
    return this.h264View();
  }
}
