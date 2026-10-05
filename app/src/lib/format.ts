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
  return path.split(/[\\/]/).pop() ?? path;
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
