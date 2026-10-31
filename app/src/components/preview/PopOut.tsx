import { PictureInPicture2 } from "lucide-react";
import type { ReactNode } from "react";
import { usePreviewWindow } from "../../state/previewWindow";
import { Button, IconButton } from "../ui";

/** Opens the preview in a window of its own (see state/previewWindow.ts). */
export function PopOutButton({ className }: { className?: string }) {
  const popOut = usePreviewWindow((s) => s.popOut);
  return (
    <IconButton
      label="Pop out the preview"
      hint="Open the preview in its own window: more room here, or the lights on another display"
      className={className}
      onClick={() => void popOut()}
    >
      <PictureInPicture2 size={14} aria-hidden />
    </IconButton>
  );
}

/**
 * For a screen with no preview of its own to put away (Layout: its canvas is the editor): opens
 * the preview's window beside it, lit by whatever is playing or being tested, and closes it again.
 */
export function PreviewWindowToggle() {
  const poppedOut = usePreviewWindow((s) => s.poppedOut);
  const { popOut, bringBack } = usePreviewWindow.getState();
  return (
    <button
      type="button"
      aria-label="Preview window"
      aria-pressed={poppedOut}
      data-tip={
        poppedOut
          ? "Close the preview's own window"
          : "Preview window: see the lights in a window of their own while you work here, on another display or in 3D beside this 2D view"
      }
      onClick={() => void (poppedOut ? bringBack() : popOut())}
      className={`inline-flex items-center rounded-md border px-2.5 py-2 ${
        poppedOut
          ? "border-accent-500 bg-accent-50 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400"
          : "border-neutral-300 bg-white hover:bg-neutral-100 dark:border-neutral-700 dark:bg-neutral-900 dark:hover:bg-neutral-800"
      }`}
    >
      <PictureInPicture2 size={16} aria-hidden />
    </button>
  );
}

/**
 * What a preview shows in its place while it's in its own window: a slim note with the way back.
 * `children`: controls that still apply to the preview there. `tall`: fills a preview's whole
 * place (the Play screen's) rather than one line.
 */
export function PoppedOutNote({ children, tall = false }: { children?: ReactNode; tall?: boolean }) {
  const bringBack = usePreviewWindow((s) => s.bringBack);
  return (
    <section
      aria-label="Preview"
      className={`flex items-center text-sm text-neutral-600 dark:text-neutral-300 ${tall ? "h-full flex-col justify-center gap-3" : "flex-wrap gap-x-3 gap-y-1.5"}`}
    >
      <span className="inline-flex items-center gap-1.5">
        <PictureInPicture2 size={tall ? 18 : 14} className="shrink-0 text-neutral-400" aria-hidden />
        Preview is in its own window
      </span>
      {children}
      <Button className={tall ? undefined : "ml-auto"} onClick={() => void bringBack()}>
        Bring the preview back
      </Button>
    </section>
  );
}
