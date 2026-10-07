/** 1150 → "1,150". */
export function thousands(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

/** plural(1, "prop") → "1 prop"; plural(2, "prop") → "2 props". */
export function plural(n: number, word: string): string {
  return `${thousands(n)} ${word}${n === 1 ? "" : "s"}`;
}

/** File name from a path, for titles. */
export function fileName(path: string): string {
  return shownPath(path.split(/[\\/]/).pop() ?? path);
}

/**
 * A path for people to read. Paths from the engine are kept exactly, so a byte that isn't UTF-8
 * (possible on Linux) arrives as a NUL and two hex digits; it shows as "�".
 */
export function shownPath(path: string): string {
  // Only bytes from 0x80 up are ever marked; any other mark isn't one and is dropped.
  return path.replace(/\u0000[89a-fA-F][0-9a-fA-F]/g, "\uFFFD").replaceAll("\u0000", "");
}

/** Seconds as a clock: 75 → "1:15", 3725 → "1:02:05". */
export function clock(totalSeconds: number): string {
  const s = Math.max(0, Math.round(totalSeconds));
  const [h, m, sec] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60];
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

/** How long ago `time` (ms since the epoch) was: "just now", "5 min ago", "3 h ago", "2 days ago". */
export function ago(time: number, now = Date.now()): string {
  const minutes = Math.floor((now - time) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  return `${days} ${days === 1 ? "day" : "days"} ago`;
}

/** A sequence's name from its file ("Christmas Medley 2017.pfseq.json" is "Christmas Medley 2017"). */
export function sequenceTitle(path: string): string {
  return fileName(path).replace(/\.(pfseq\.json|fseq)$/i, "");
}

/** A file size: "24.6 GB", "8.9 MB", "12 KB". */
export function sizeText(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${Math.ceil(bytes / 1024)} KB`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** A device's "YYYY-MM-DD HH:MM" as a short date: "Oct 6, 2026" (the text as is if it isn't one). */
export function shortDate(text: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(text);
  if (!match) return text;
  return `${MONTHS[Number(match[2]) - 1] ?? "?"} ${Number(match[3])}, ${match[1]}`;
}
