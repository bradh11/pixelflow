// Reading a camera-mapping video: the platform's own decoder (a <video> element) plays the file,
// frames are drawn small and averaged here, and the engine finds the pixels in them.

import type { Backend } from "../api/backend";
import type { CodeBase, DecodedCapture, TargetSpec } from "../api/types";

/** Something frames can be read from: a video file, or the demo's made-up capture. */
export interface FrameSource {
  /** Seconds. */
  duration: number;
  width: number;
  height: number;
  /** The frame at `t` seconds, scaled to `width` × `height`, as RGBA bytes. */
  grab(t: number, width: number, height: number): Promise<Uint8ClampedArray>;
  dispose(): void;
}

/** Frames are analysed at most this wide (half of 1080p: sub-pixel centres keep ~1 px at 1080p). */
export const ANALYSIS_WIDTH = 960;
/** Frames per second sampled when looking for the sequence's start. */
const SYNC_FPS = 10;
/** Frames per second averaged within each slot. */
const SLOT_FPS = 30;
/** Slots that light every pixel white (the preamble's lit slots). */
const WHITE_SLOTS = [1, 3, 4];

export interface CaptureRead {
  decoded: DecodedCapture;
  /** The capture with every pixel lit (RGBA, decoded.width × decoded.height), to review on. */
  picture: Uint8ClampedArray;
}

/** What reading a capture is doing now, and how far along (0–1). */
export interface ReadProgress {
  step: "sync" | "frames" | "decode";
  done: number;
}

/**
 * Finds the sequence in `source`, averages each slot's frames, and has the engine decode them.
 * Rejects with a plain message (no sequence found, a frame that won't decode) or when `signal`
 * aborts.
 */
export async function readCapture(
  source: FrameSource,
  backend: Backend,
  pixels: number,
  base: CodeBase,
  onProgress: (p: ReadProgress) => void,
  signal?: AbortSignal,
): Promise<CaptureRead> {
  const stop = () => {
    if (signal?.aborted) throw new Error("Reading the video was cancelled.");
  };
  const width = Math.min(ANALYSIS_WIDTH, source.width);
  const height = Math.max(1, Math.round((source.height * width) / source.width));

  // 1. Overall brightness through the whole video, to find where the sequence starts.
  const samples = [];
  const steps = Math.floor(source.duration * SYNC_FPS);
  const small = { w: 160, h: Math.max(1, Math.round((160 * source.height) / source.width)) };
  for (let i = 0; i <= steps; i++) {
    stop();
    const t = i / SYNC_FPS;
    const rgba = await source.grab(t, small.w, small.h);
    let sum = 0;
    for (let p = 0; p < rgba.length; p += 4) sum += rgba[p] + rgba[p + 1] + rgba[p + 2];
    samples.push({ t, v: sum / (rgba.length / 4) / 3 });
    onProgress({ step: "sync", done: i / Math.max(1, steps) });
  }
  const sync = await backend.cameraMapSync(samples, pixels, base);

  // 2. Each slot's frames, averaged.
  const each = width * height * 3;
  const frames = new Uint8Array(each * sync.windows.length);
  const picture = new Uint8ClampedArray(width * height * 4);
  const whiteSum = new Float32Array(each);
  for (const [k, [a, b]] of sync.windows.entries()) {
    const times: number[] = [];
    for (let t = Math.ceil(a * SLOT_FPS) / SLOT_FPS; t <= b; t += 1 / SLOT_FPS) times.push(t);
    if (times.length === 0) times.push((a + b) / 2);
    const sum = new Float32Array(each);
    for (const t of times) {
      stop();
      const rgba = await source.grab(t, width, height);
      for (let p = 0, q = 0; q < each; p += 4, q += 3) {
        sum[q] += rgba[p];
        sum[q + 1] += rgba[p + 1];
        sum[q + 2] += rgba[p + 2];
      }
    }
    const slot = frames.subarray(k * each, (k + 1) * each);
    for (let q = 0; q < each; q++) slot[q] = Math.round(sum[q] / times.length);
    if (WHITE_SLOTS.includes(k)) for (let q = 0; q < each; q++) whiteSum[q] += slot[q] / WHITE_SLOTS.length;
    onProgress({ step: "frames", done: (k + 1) / sync.windows.length });
  }
  for (let p = 0, q = 0; q < each; p += 4, q += 3) {
    picture[p] = whiteSum[q];
    picture[p + 1] = whiteSum[q + 1];
    picture[p + 2] = whiteSum[q + 2];
    picture[p + 3] = 255;
  }

  // 3. The engine finds the pixels and reads their numbers.
  stop();
  onProgress({ step: "decode", done: 0 });
  const decoded = await backend.cameraMapDecode(frames, { width, height, pixels, base });
  onProgress({ step: "decode", done: 1 });
  return { decoded, picture };
}

/** A video file, played by the platform's decoder (H.264 everywhere; HEVC on macOS). */
export function openVideo(file: Blob): Promise<FrameSource> {
  const url = URL.createObjectURL(file);
  const video = document.createElement("video");
  video.muted = true;
  video.preload = "auto";
  video.playsInline = true;
  video.src = url;
  const canvas = document.createElement("canvas");
  const dispose = () => {
    video.removeAttribute("src");
    video.load();
    URL.revokeObjectURL(url);
  };
  const seek = (t: number) =>
    new Promise<void>((resolve, reject) => {
      if (Math.abs(video.currentTime - t) < 1e-4 && video.readyState >= 2) return resolve();
      const timer = setTimeout(() => done(new Error("The video stopped responding while being read.")), 10_000);
      const done = (error?: Error) => {
        clearTimeout(timer);
        video.removeEventListener("seeked", ok);
        video.removeEventListener("error", bad);
        if (error) reject(error);
        else resolve();
      };
      const ok = () => done();
      const bad = () => done(new Error(UNPLAYABLE));
      video.addEventListener("seeked", ok);
      video.addEventListener("error", bad);
      video.currentTime = t;
    });
  return new Promise((resolve, reject) => {
    video.addEventListener(
      "loadeddata",
      () => {
        if (!video.videoWidth || !Number.isFinite(video.duration)) {
          dispose();
          return reject(new Error(UNPLAYABLE));
        }
        resolve({
          duration: video.duration,
          width: video.videoWidth,
          height: video.videoHeight,
          grab: async (t, width, height) => {
            await seek(Math.min(t, video.duration));
            canvas.width = width;
            canvas.height = height;
            const ctx = canvas.getContext("2d", { willReadFrequently: true });
            if (!ctx) throw new Error("This computer can't read video frames here.");
            ctx.drawImage(video, 0, 0, width, height);
            return ctx.getImageData(0, 0, width, height).data;
          },
          dispose,
        });
      },
      { once: true },
    );
    video.addEventListener(
      "error",
      () => {
        dispose();
        reject(new Error(UNPLAYABLE));
      },
      { once: true },
    );
  });
}

export const UNPLAYABLE =
  "This video can't be played here. Use an MP4 (H.264) file — on iPhone, set Settings › Camera › Formats to Most Compatible before recording.";

/** The target as the key the capture was made for (a different target needs a new capture). */
export function targetKey(target: TargetSpec): string {
  return JSON.stringify(target);
}
