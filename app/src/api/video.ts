import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { pickPath } from "./tauri";

/** The event the engine sends while exporting a video. */
export const VIDEO_PROGRESS_EVENT = "video-export-progress";

/** The event the shell sends while Cisco's video encoder downloads. */
export const H264_DOWNLOAD_PROGRESS_EVENT = "h264-download-progress";

/** What the video export dialog can offer. */
export interface VideoChoices {
  /** ffmpeg's H.264 encoder when ffmpeg is installed ("x264" or "VideoToolbox"), else null. */
  ffmpeg: string | null;
  /** Whether the show has a layout photo to draw behind the props. */
  photo: boolean;
}

/** What to export. */
export interface VideoRequest {
  width: number;
  height: number;
  fps: number;
  startMs: number;
  /** The sequence's end when null. */
  endMs: number | null;
  photo: boolean;
  /** The dots' size against the preview's (1 is the same). */
  pixelSize: number;
  /** How much each lit dot glows, 0 (crisp dots) to 1. */
  glow: number;
  /** Leave pixels that are off out: only lit ones show, as at night (dim when left out). */
  hideUnlit?: boolean;
  /** How much dim levels are lifted so fades show on a screen, 0 (not at all, when left out) to 3. */
  boost?: number;
  /** Encode with ffmpeg instead of the built-in encoders. */
  ffmpeg: boolean;
}

export type VideoStage = "rendering" | "sound" | "writing";

/** How far a video export has got. */
export interface VideoProgress {
  path: string;
  stage: VideoStage;
  /** What's being done, for the label by the bar. */
  label: string;
  /** 0–1 of this stage, or null when it can't tell. */
  fraction: number | null;
}

/** What was written. */
export interface VideoSummary {
  frames: number;
  durationMs: number;
  width: number;
  height: number;
  fps: number;
  bytes: number;
  /** How the picture and sound were encoded, in words. */
  video: string;
  sound: string | null;
  elapsedMs: number;
  notes: string[];
}

/**
 * Whether Cisco's OpenH264 encoder can be used: Cisco makes none for this system (or PixelFlow
 * has nowhere to keep it), not downloaded, downloaded but turned off, or downloaded and on.
 */
export type H264State = "unsupported" | "missing" | "off" | "ready";

/**
 * Cisco's OpenH264 video encoder, the built-in encoder for video export. PixelFlow doesn't ship
 * it: it's downloaded from Cisco only when the user agrees (Cisco pays the H.264 patent licence
 * for the copies it hands out itself), checked, and kept in PixelFlow's data folder.
 */
export interface H264Encoder {
  state: H264State;
  downloading: boolean;
  /** Cisco's release used ("2.6.0"). */
  version: string;
  /** Cisco's server ("ciscobinary.openh264.org"). */
  host: string;
  /** Where it's downloaded from; null when Cisco makes none for this system. */
  url: string | null;
  /** The download's size in bytes, about. */
  bytes: number;
  /** What Cisco's licence asks be shown where it's turned on and off. */
  attribution: string;
  /** Cisco's licence for its binary, word for word. */
  licence: string;
  licenceUrl: string;
  /** Why it can't be used on this computer, when it can't. */
  problem: string | null;
}

/** How far the encoder's download has got. */
export interface H264Progress {
  received: number;
  total: number;
}

/** Exporting the open sequence as an MP4 video of the preview with its music. */
export interface VideoApi {
  videoExportChoices(): Promise<VideoChoices>;
  /** Rejects with "The export was cancelled." after cancelVideoExport (no file is written). */
  exportVideo(path: string, request: VideoRequest, onProgress?: (progress: VideoProgress) => void): Promise<VideoSummary>;
  cancelVideoExport(): Promise<void>;
  /** Where to save the video (a native dialog); null when cancelled. */
  pickVideoPath(defaultName: string): Promise<string | null>;
  /** Cisco's OpenH264 encoder: whether it's downloaded and on. */
  h264Encoder(): Promise<H264Encoder>;
  /**
   * Downloads Cisco's encoder from Cisco's server (call only after the user has agreed), checks
   * it, keeps it, and turns it on. Rejects with "Stopped." after cancelH264Download, keeping
   * nothing, and with a sentence saying what to do when it fails.
   */
  downloadH264Encoder(onProgress?: (progress: H264Progress) => void): Promise<H264Encoder>;
  cancelH264Download(): Promise<void>;
  /** Turns Cisco's encoder on or off for video export (kept across runs). */
  setH264EncoderEnabled(on: boolean): Promise<H264Encoder>;
  /** Deletes Cisco's encoder from PixelFlow's data folder. */
  removeH264Encoder(): Promise<H264Encoder>;
}

/** The real engine, in the Tauri desktop shell. */
export const tauriVideo: VideoApi = {
  videoExportChoices: () => invoke("video_export_choices"),
  exportVideo: async (path, request, onProgress) => {
    const unlisten = onProgress
      ? await listen<VideoProgress>(VIDEO_PROGRESS_EVENT, (event) => {
          if (event.payload.path === path) onProgress(event.payload);
        })
      : null;
    try {
      return await invoke<VideoSummary>("export_video", { path, request });
    } finally {
      unlisten?.();
    }
  },
  cancelVideoExport: () => invoke("cancel_video_export"),
  pickVideoPath: (defaultName) => pickPath("videoExport", defaultName),
  h264Encoder: () => invoke("h264_encoder"),
  downloadH264Encoder: async (onProgress) => {
    const unlisten = onProgress ? await listen<H264Progress>(H264_DOWNLOAD_PROGRESS_EVENT, (event) => onProgress(event.payload)) : null;
    try {
      return await invoke<H264Encoder>("download_h264_encoder");
    } finally {
      unlisten?.();
    }
  },
  cancelH264Download: () => invoke("cancel_h264_download"),
  setH264EncoderEnabled: (on) => invoke("set_h264_encoder_enabled", { on }),
  removeH264Encoder: () => invoke("remove_h264_encoder"),
};
