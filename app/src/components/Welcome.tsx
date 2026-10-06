import { FolderOpen, Network, PartyPopper, Sparkles, Upload } from "lucide-react";
import { type ReactNode, useEffect } from "react";
import { useApp } from "../state/store";
import { RecentShowCard } from "./RecentShows";

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
      className="flex items-start gap-3 rounded-xl border border-neutral-200 bg-white p-4 text-left transition hover:border-accent-500 hover:shadow-md focus-visible:border-accent-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-500/40 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:border-neutral-200 disabled:hover:shadow-none dark:border-neutral-800 dark:bg-neutral-900 dark:disabled:hover:border-neutral-800"
    >
      <span className="rounded-lg bg-accent-50 p-2 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400">{icon}</span>
      <span className="min-w-0">
        <span className="block font-medium">{title}</span>
        <span className="mt-0.5 block text-sm text-neutral-500 dark:text-neutral-400">{description}</span>
      </span>
    </button>
  );
}

/** The start page: the recent shows to pick up where you left off, and every way to begin. */
export function Welcome() {
  const { newShow, openShow, importXlights, openSample, clearRecent, refreshRecent } = useApp.getState();
  const discover = useApp((s) => s.discoverFromWelcome);
  const recent = useApp((s) => s.recent);
  const busy = useApp((s) => s.opening !== null);
  const now = Date.now();

  useEffect(() => {
    void refreshRecent();
  }, [refreshRecent]);

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto w-full max-w-5xl px-4 py-10 sm:px-8">
        <h1 className="text-3xl font-semibold tracking-tight">Welcome to PixelFlow</h1>
        <p className="mt-2 text-neutral-500 dark:text-neutral-400">Design your display, wire it to your controllers, and light it up.</p>
        <div className="mt-8 grid gap-8 md:grid-cols-[minmax(0,1fr)_18rem]">
          <section aria-labelledby="recent-shows-heading" className="min-w-0">
            <div className="flex items-baseline justify-between gap-2">
              <h2 id="recent-shows-heading" className="text-sm font-semibold uppercase tracking-wide text-neutral-500 dark:text-neutral-400">
                Recent shows
              </h2>
              {recent.length > 0 && (
                <button
                  type="button"
                  onClick={() => void clearRecent()}
                  className="rounded px-1.5 py-0.5 text-xs text-neutral-500 hover:bg-neutral-100 hover:text-neutral-800 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
                >
                  Clear recent shows
                </button>
              )}
            </div>
            {recent.length === 0 ? (
              <p className="mt-3 rounded-xl border border-dashed border-neutral-300 p-6 text-sm text-neutral-500 dark:border-neutral-700 dark:text-neutral-400">
                Shows you open or save will be listed here, so you can pick up where you left off.
              </p>
            ) : (
              <ul className="mt-3 flex flex-col gap-2">
                {recent.map((show) => (
                  <RecentShowCard key={show.path} show={show} now={now} />
                ))}
              </ul>
            )}
          </section>
          <nav aria-label="Start" className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold uppercase tracking-wide text-neutral-500 dark:text-neutral-400">Start</h2>
            <Choice icon={<Sparkles size={20} />} title="New show" description="An empty show to add your props to." onClick={() => void newShow()} disabled={busy} />
            <Choice icon={<FolderOpen size={20} />} title="Open…" description="A PixelFlow show file." onClick={() => void openShow()} disabled={busy} />
            <Choice
              icon={<Upload size={20} />}
              title="Import from xLights…"
              description="Choose your xLights show folder (the one with xlights_rgbeffects.xml)."
              onClick={() => void importXlights()}
              disabled={busy}
            />
            <Choice
              icon={<PartyPopper size={20} />}
              title="Try the demo show"
              description="A sample house to explore. It opens as a copy, so nothing is changed."
              onClick={() => void openSample()}
              disabled={busy}
            />
            <Choice
              icon={<Network size={20} />}
              title="Discover my devices"
              description="Starts a new show and finds your FPP, Falcon, and WLED controllers."
              onClick={() => void discover()}
              disabled={busy}
            />
          </nav>
        </div>
      </div>
    </div>
  );
}
