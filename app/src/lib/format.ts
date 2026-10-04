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
