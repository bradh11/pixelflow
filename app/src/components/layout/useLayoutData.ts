import { useEffect, useState } from "react";
import type { PreviewProp } from "../../api/types";
import { useApp } from "../../state/store";

/** How often the canvas picks up colors while a test pattern or sequence is running. */
const FRAME_MS = 100;
/** How often it checks whether something started, while nothing is running. */
const IDLE_MS = 1000;

/** Every prop's pixels from the engine, fetched again after each change to the show. */
export function usePreviewProps(): PreviewProp[] {
  const backend = useApp((s) => s.backend);
  const revision = useApp((s) => s.snapshot?.revision);
  const [props, setProps] = useState<PreviewProp[]>([]);
  useEffect(() => {
    if (!backend) return;
    // Only the latest request counts: an older answer arriving late is ignored.
    let latest = true;
    void backend.previewProps().then(
      (p) => latest && setProps(p),
      () => latest && setProps([]),
    );
    return () => {
      latest = false;
    };
  }, [backend, revision]);
  return props;
}

/** The props' current colors while something plays: checked quickly while frames arrive, slowly otherwise. */
export function useLiveFrame(): Uint8Array | null {
  const backend = useApp((s) => s.backend);
  const [frame, setFrame] = useState<Uint8Array | null>(null);
  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = () => {
      backend.liveFrame().then(
        (f) => {
          if (cancelled) return;
          const lit = f.length > 0;
          setFrame(lit ? f : null);
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
  }, [backend]);
  return frame;
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
  image: HTMLImageElement | null;
  /** Height divided by width; a typical photo's until the image has loaded. */
  aspect: number;
  /** Why the photo can't be shown, in plain language. */
  problem: string | null;
}

export const FALLBACK_ASPECT = 0.75;

/** The background photo at `path`, loaded through the backend (no file access in the window). */
export function useBackgroundImage(path: string | null | undefined): PhotoImage {
  const backend = useApp((s) => s.backend);
  const [state, setState] = useState<PhotoImage & { path: string | null }>({
    path: null,
    image: null,
    aspect: FALLBACK_ASPECT,
    problem: null,
  });
  useEffect(() => {
    if (!backend || !path) return;
    let cancelled = false;
    let url: string | null = null;
    void backend.readImage(path).then(
      async (bytes) => {
        url = blobUrl(bytes, path);
        if (!url || cancelled) return;
        const image = await loadImage(url);
        if (cancelled) return;
        setState(
          image
            ? { path, image, aspect: image.naturalHeight / image.naturalWidth, problem: null }
            : { path, image: null, aspect: FALLBACK_ASPECT, problem: "This photo couldn't be shown. Try a PNG or JPEG." },
        );
      },
      (e: unknown) => {
        if (!cancelled) {
          const problem = e instanceof Error ? e.message : String(e);
          setState({ path, image: null, aspect: FALLBACK_ASPECT, problem });
        }
      },
    );
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [backend, path]);
  if (!path || state.path !== path) return { image: null, aspect: FALLBACK_ASPECT, problem: null };
  return state;
}
