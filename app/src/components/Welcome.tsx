import { FolderOpen, Network, Sparkles, Upload } from "lucide-react";
import type { ReactNode } from "react";
import { useApp } from "../state/store";

function Choice({
  icon,
  title,
  description,
  onClick,
  disabled,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex items-start gap-4 rounded-xl border border-neutral-200 bg-white p-5 text-left transition hover:border-accent-500 hover:shadow-md disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:border-neutral-200 disabled:hover:shadow-none dark:border-neutral-800 dark:bg-neutral-900 dark:disabled:hover:border-neutral-800"
    >
      <span className="rounded-lg bg-accent-50 p-2 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400">
        {icon}
      </span>
      <span>
        <span className="block font-medium">{title}</span>
        <span className="mt-1 block text-sm text-neutral-500 dark:text-neutral-400">{description}</span>
      </span>
    </button>
  );
}

/** First-run screen: pick how to start. */
export function Welcome() {
  const newShow = useApp((s) => s.newShow);
  const openShow = useApp((s) => s.openShow);
  const discover = useApp((s) => s.discoverFromWelcome);
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="w-full max-w-2xl">
        <h1 className="text-3xl font-semibold tracking-tight">Welcome to PixelFlow</h1>
        <p className="mt-2 text-neutral-500 dark:text-neutral-400">
          Design your display, wire it to your controllers, and light it up.
        </p>
        <div className="mt-8 grid gap-3 sm:grid-cols-2">
          <Choice
            icon={<Sparkles size={20} />}
            title="Start fresh"
            description="Create an empty show and add your props."
            onClick={newShow}
          />
          <Choice
            icon={<FolderOpen size={20} />}
            title="Open a show"
            description="Continue with a PixelFlow show file."
            onClick={openShow}
          />
          <Choice
            icon={<Upload size={20} />}
            title="Import from xLights"
            description="Coming in a later update."
            disabled
          />
          <Choice
            icon={<Network size={20} />}
            title="Discover my devices"
            description="Find FPP, Falcon, and WLED controllers on your network and import them."
            onClick={discover}
          />
        </div>
      </div>
    </div>
  );
}
