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
