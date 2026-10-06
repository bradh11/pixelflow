/**
 * `name` as an FPP file name ending in `.ext`, as the desktop shell names files it sends (only
 * characters FPP keeps, no `..`, never empty). Used to match local files with ones on an FPP.
 */
export function fppFileName(name: string, ext: string): string {
  const stem = name
    .replace(/[/\\]/g, " ")
    .replace(/[^A-Za-z0-9_ \-~,;[\]().]/g, "")
    .replace(/\.{2,}/g, ".")
    .replace(/ {2,}/g, " ")
    .replace(/^[ .]+|[ .]+$/g, "");
  return `${stem || "Sequence"}.${ext.toLowerCase()}`;
}
