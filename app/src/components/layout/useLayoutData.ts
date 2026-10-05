import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { PreviewSet, PreviewSet3d } from "../../api/types";
import { useApp } from "../../state/store";

/** How often the canvas picks up colors while a test pattern or sequence is running. */
const FRAME_MS = 100;
/** How often it checks whether something started, while nothing is running. */
const IDLE_MS = 1000;

const NO_PREVIEW: PreviewSet = { revision: -1, props: [] };

/** Every prop's pixels from the engine, fetched again after each change to the show. */
export function usePreviewProps(): PreviewSet {
  const backend = useApp((s) => s.backend);
  const revision = useApp((s) => s.snapshot?.revision);
  const [preview, setPreview] = useState<PreviewSet>(NO_PREVIEW);
  useEffect(() => {
    if (!backend) return;
    // Only the latest request counts: an older answer arriving late is ignored.
    let latest = true;
    void backend.previewProps().then(
      (p) => latest && setPreview(p),
      (e: unknown) => {
        if (!latest) return;
        setPreview({ revision: revision ?? -1, props: [] });
        // Without positions the canvas can't show (or pick) any prop: say so instead of showing nothing.
        useApp.setState({ error: errorMessage(e) });
      },
    );
    return () => {
      latest = false;
    };
  }, [backend, revision]);
  return preview;
}

const NO_PREVIEW_3D: PreviewSet3d = { revision: -1, props: [] };

/** Every prop's pixels in 3D, fetched after each change to the show while `enabled`. */
export function usePreviewProps3d(enabled: boolean): PreviewSet3d {
  const backend = useApp((s) => s.backend);
  const revision = useApp((s) => s.snapshot?.revision);
  const [preview, setPreview] = useState<PreviewSet3d>(NO_PREVIEW_3D);
  useEffect(() => {
    if (!backend || !enabled) return;
    let latest = true;
    void backend.previewProps3d().then(
      (p) => latest && setPreview(p),
      (e: unknown) => {
        if (!latest) return;
        setPreview({ revision: revision ?? -1, props: [] });
        useApp.setState({ error: errorMessage(e) });
      },
    );
    return () => {
      latest = false;
    };
  }, [backend, revision, enabled]);
  return preview;
}

/**
 * Hands the props' current colors to `onFrame` while something plays (null when nothing is):
 * checked quickly while frames arrive, slowly otherwise. Nothing re-renders. Off when `enabled`
 * is false (the screen fetches colors itself).
 */
export function useLiveFrame(onFrame: (frame: Uint8Array | null) => void, enabled = true) {
  const backend = useApp((s) => s.backend);
  const callback = useRef(onFrame);
  useEffect(() => {
    callback.current = onFrame;
  }, [onFrame]);
  useEffect(() => {
    if (!backend || !enabled) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = () => {
      backend.liveFrame().then(
        (f) => {
          if (cancelled) return;
          const lit = f.length > 0;
          callback.current(lit ? f : null);
          timer = setTimeout(poll, lit ? FRAME_MS : IDLE_MS);
        },
        () => {
          if (!cancelled) timer = setTimeout(poll, IDLE_MS);
        },
      );
    };
    timer = setTimeout(poll, FRAME_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [backend, enabled]);
}

const IMAGE_TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
  gif: "image/gif",
  bmp: "image/bmp",
  svg: "image/svg+xml",
};

/** The image's media type, from its file name. */
export function imageType(path: string): string {
  return IMAGE_TYPES[path.split(".").pop()?.toLowerCase() ?? ""] ?? "application/octet-stream";
}

/** A blob URL for image bytes, or null where the browser can't make one (tests). */
function blobUrl(bytes: Uint8Array, path: string): string | null {
  if (typeof URL.createObjectURL !== "function") return null;
  return URL.createObjectURL(new Blob([bytes as BlobPart], { type: imageType(path) }));
}

/** The loaded image, or null if it can't be shown (or doesn't load within a few seconds). */
function loadImage(url: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const image = new Image();
    const timer = setTimeout(() => resolve(null), 5000);
    image.onload = () => {
      clearTimeout(timer);
      resolve(image.naturalWidth > 0 ? image : null);
    };
    image.onerror = () => {
      clearTimeout(timer);
      resolve(null);
    };
    image.src = url;
  });
}

/** An image's height divided by its width, or null when it can't be read. */
export async function imageAspect(bytes: Uint8Array, path: string): Promise<number | null> {
  const url = blobUrl(bytes, path);
  if (!url) return null;
  try {
    const image = await loadImage(url);
    return image ? image.naturalHeight / image.naturalWidth : null;
  } finally {
    URL.revokeObjectURL(url);
  }
}

export interface PhotoImage {
  image: HTMLImageElement | ImageBitmap | null;
  /** Height divided by width; a typical photo's until the image has loaded. */
  aspect: number;
  /** Why the photo can't be shown, in plain language. */
  problem: string | null;
  /** Reads the photo again: after fixing a problem, or choosing a file with the same name. */
  reload(): void;
}

export const FALLBACK_ASPECT = 0.75;
/** Wider photos are scaled down once to this width: the canvas never needs more, and it draws faster. */
export const MAX_PHOTO_WIDTH = 4096;

/** The image, scaled down to `MAX_PHOTO_WIDTH` when it's wider (where the browser can). */
async function shrink(image: HTMLImageElement): Promise<HTMLImageElement | ImageBitmap> {
  if (image.naturalWidth <= MAX_PHOTO_WIDTH || typeof createImageBitmap !== "function") return image;
  try {
    return await createImageBitmap(image, { resizeWidth: MAX_PHOTO_WIDTH, resizeQuality: "high" });
  } catch {
    return image;
  }
}

/** The background photo at `path`, loaded through the backend (no file access in the window). */
export function useBackgroundImage(path: string | null | undefined): PhotoImage {
  const backend = useApp((s) => s.backend);
  const [attempt, setAttempt] = useState(0);
  const reload = useCallback(() => setAttempt((a) => a + 1), []);
  const [state, setState] = useState<Omit<PhotoImage, "reload"> & { path: string | null }>({
    path: null,
    image: null,
    aspect: FALLBACK_ASPECT,
    problem: null,
  });
  useEffect(() => {
    if (!backend || !path) return;
    let cancelled = false;
    // Trying again: the old problem no longer stands while the photo is read.
    setState((s) => (s.path === path && s.problem ? { ...s, problem: null } : s));
    void (async () => {
      let url: string | null = null;
      try {
        const bytes = await backend.readImage(path);
        if (cancelled) return;
        url = blobUrl(bytes, path);
        if (!url) return;
        const loaded = await loadImage(url);
        if (cancelled) return;
        if (!loaded) {
          setState({ path, image: null, aspect: FALLBACK_ASPECT, problem: "This photo couldn't be shown. Try a PNG or JPEG." });
          return;
        }
        const image = await shrink(loaded);
        if (!cancelled) setState({ path, image, aspect: loaded.naturalHeight / loaded.naturalWidth, problem: null });
      } catch (e) {
        if (!cancelled) setState({ path, image: null, aspect: FALLBACK_ASPECT, problem: errorMessage(e) });
      } finally {
        // A loaded image keeps its picture; the URL isn't needed any more.
        if (url) URL.revokeObjectURL(url);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [backend, path, attempt]);
  if (!path || state.path !== path) return { image: null, aspect: FALLBACK_ASPECT, problem: null, reload };
  return { ...state, reload };
}
