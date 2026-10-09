import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { pickPath } from "./tauri";

/** The event the engine sends while exporting a video. */
export const VIDEO_PROGRESS_EVENT = "video-export-progress";

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

/** Exporting the open sequence as an MP4 video of the preview with its music. */
export interface VideoApi {
  videoExportChoices(): Promise<VideoChoices>;
  /** Rejects with "The export was cancelled." after cancelVideoExport (no file is written). */
  exportVideo(path: string, request: VideoRequest, onProgress?: (progress: VideoProgress) => void): Promise<VideoSummary>;
  cancelVideoExport(): Promise<void>;
  /** Where to save the video (a native dialog); null when cancelled. */
  pickVideoPath(defaultName: string): Promise<string | null>;
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
};
