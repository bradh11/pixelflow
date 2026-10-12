import { ExternalLink, Heart, X } from "lucide-react";
import { type ReactNode, useEffect } from "react";
import { version } from "../../package.json";
import { errorMessage } from "../api/backend";
import type { ProjectLink } from "../api/types";
import { type SupportLink, supportLinks } from "../lib/support";
import { useAbout } from "../state/about";
import { useApp } from "../state/store";
import { Button, IconButton } from "./ui";

/** Who holds the copyright in PixelFlow (the same line as `bundle.copyright` and NOTICE). */
export const COPYRIGHT = "Copyright (C) 2026 Brad Haas";
/** PixelFlow's licence, by name. */
export const LICENCE = "GNU General Public License, version 3";

/** Hands one of the project's links to the shell, which opens it outside the window. */
function open(link: ProjectLink) {
  const { backend } = useApp.getState();
  void backend?.openProjectLink(link).catch((e) => useApp.setState({ error: errorMessage(e) }));
}

function LinkButton({ link, tip, children }: { link: ProjectLink; tip: string; children: ReactNode }) {
  return (
    <Button onClick={() => open(link)} title={tip} className="px-2.5 py-1">
      {children}
    </Button>
  );
}

/**
 * The ways to support the project that are set up, each a button that opens its page in the
 * system's browser. With none set up it draws nothing at all.
 */
export function SupportLinks({ links = supportLinks() }: { links?: SupportLink[] }) {
  if (links.length === 0) return null;
  return (
    <ul aria-label="Ways to support PixelFlow" className="flex flex-wrap gap-2">
      {links.map((link) => (
        <li key={link.way}>
          <LinkButton link={link.way} tip={`Opens ${link.address} in your web browser`}>
            {link.label} <ExternalLink size={12} aria-hidden />
          </LinkButton>
        </li>
      ))}
    </ul>
  );
}

/** Why one might, and what it buys (nothing), above the ways themselves. */
function SupportBlock({ links }: { links: SupportLink[] }) {
  return (
    <div className="flex flex-col gap-2">
      <p className="text-sm text-neutral-600 dark:text-neutral-300">
        PixelFlow is free, and every feature is for everyone. If it is useful to you and you would like to help with what it costs to make, thank you: each
        of these opens in your web browser.
      </p>
      <SupportLinks links={links} />
    </div>
  );
}

/**
 * What PixelFlow is, legally and by descent: its version, who holds the copyright, its licence
 * and that there is no warranty, the credit to xLights, and where to read each in full. The ways
 * to support it follow when any is set up.
 */
export function AboutDetails({ links = supportLinks() }: { links?: SupportLink[] }) {
  return (
    <div className="flex flex-col gap-3 text-sm">
      <div>
        <p className="font-semibold">PixelFlow {version}</p>
        <p className="text-neutral-500">{COPYRIGHT}</p>
      </div>
      <p>
        Free software under the {LICENCE}. You may use it, change it, and pass it on under that licence. It comes with no warranty.
      </p>
      <p>
        PixelFlow is an independent project, inspired by xLights and able to import its shows. Parts of it are derived from xLights' source code and are
        used under xLights' licence, with thanks to its authors. Credits says which.
      </p>
      <div className="flex flex-wrap gap-2">
        <LinkButton link="licence" tip="Opens the licence that comes with PixelFlow">
          Licence
        </LinkButton>
        <LinkButton link="credits" tip="Opens PixelFlow's notice: its credit to xLights and to the other work it draws on">
          Credits
        </LinkButton>
        <LinkButton link="notices" tip="Opens the licences of everything PixelFlow is built from">
          Third-party notices
        </LinkButton>
        <LinkButton link="source" tip="Opens github.com/bradh11/pixelflow in your web browser">
          Source code <ExternalLink size={12} aria-hidden />
        </LinkButton>
        <LinkButton link="xlights" tip="Opens github.com/xLightsSequencer/xLights in your web browser">
          xLights <ExternalLink size={12} aria-hidden />
        </LinkButton>
      </div>
      {links.length > 0 && (
        <section aria-labelledby="about-support" className="flex flex-col gap-2 border-t border-neutral-200 pt-3 dark:border-neutral-800">
          <h3 id="about-support" className="flex items-center gap-1.5 font-semibold">
            <Heart size={14} aria-hidden /> Support PixelFlow
          </h3>
          <SupportBlock links={links} />
        </section>
      )}
    </div>
  );
}

/**
 * The app's own About, and its smaller sibling that lists only the ways to support the project.
 * Opened from the menu, the command palette, or the start page, and never by itself. Asked for
 * the ways to give when none is set up, it shows About.
 */
export function AboutDialog({ links = supportLinks() }: { links?: SupportLink[] }) {
  const open = useAbout((s) => s.open);
  if (!open) return null;
  return <Panel support={open === "support" && links.length > 0} links={links} />;
}

function Panel({ support, links }: { support: boolean; links: SupportLink[] }) {
  const close = useAbout.getState().close;
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    document.getElementById("about-close")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close();
    };
    // Ahead of the screens' own keys, so Escape doesn't also clear a selection behind the panel.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (opener?.isConnected) opener.focus();
    };
  }, [close]);
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[12vh]" onClick={close}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="about-title"
        onClick={(e) => e.stopPropagation()}
        className="flex max-h-[76vh] w-full max-w-md flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="flex items-center justify-between gap-3 border-b border-neutral-200 px-4 py-2.5 dark:border-neutral-800">
          <h2 id="about-title" className="font-semibold">
            {support ? "Support PixelFlow" : "About PixelFlow"}
          </h2>
          <IconButton id="about-close" label="Close" onClick={close} className="rounded-md p-1 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800">
            <X size={16} aria-hidden />
          </IconButton>
        </div>
        <div className="overflow-auto px-4 py-3">{support ? <SupportBlock links={links} /> : <AboutDetails links={links} />}</div>
      </div>
    </div>
  );
}
