import type { ByVerdict, LookGroup, LookVerdict } from "../api/types";

/** The verdicts, best first. */
export const LOOK_VERDICTS: LookVerdict[] = ["matches", "close", "different", "noEquivalent"];

/** What a share of the sequence with each verdict is said to do. */
const SHARE_OF: Record<LookVerdict, string> = {
  matches: "comes across the same",
  close: "looks close",
  different: "looks different",
  noEquivalent: "has no PixelFlow effect yet",
};

/** The heading over the effects with each verdict. */
export const LOOK_HEADING: Record<LookVerdict, string> = {
  matches: "Comes across the same",
  close: "Looks close",
  different: "Looks different",
  noEquivalent: "No PixelFlow effect yet",
};

/** Each verdict's share of `ms` as a whole percent that never claims all of it, or none of it,
 * when that isn't so; `null` for a verdict nothing got. */
export function lookShares(ms: ByVerdict): Record<LookVerdict, string | null> {
  const total = LOOK_VERDICTS.reduce((sum, v) => sum + ms[v], 0);
  const share = (part: number): string | null => {
    if (part <= 0 || total <= 0) return null;
    const percent = Math.round((part * 100) / total);
    if (percent === 0) return "under 1%";
    return percent === 100 && part < total ? "99%" : `${percent}%`;
  };
  return { matches: share(ms.matches), close: share(ms.close), different: share(ms.different), noEquivalent: share(ms.noEquivalent) };
}

/**
 * One line on how a sequence comes across, from how long its effects of each verdict run added
 * up: "82% of this sequence comes across the same · 12% looks close · 6% looks different". A
 * verdict nothing got isn't mentioned (but for the first); `null` for a sequence with no effects.
 * The engine's `pf_xlights::sequence::look::headline` says the same.
 */
export function lookHeadline(ms: ByVerdict): string | null {
  const shares = lookShares(ms);
  if (LOOK_VERDICTS.every((v) => shares[v] === null)) return null;
  const parts = [`${shares.matches ?? "0%"} of this sequence ${SHARE_OF.matches}`];
  for (const verdict of LOOK_VERDICTS.slice(1)) {
    if (shares[verdict]) parts.push(`${shares[verdict]} ${SHARE_OF[verdict]}`);
  }
  return parts.join(" · ");
}

/** The groups of `looks` with each verdict that has any, worst first, in the order given. */
export function looksByVerdict(looks: LookGroup[]): { verdict: LookVerdict; groups: LookGroup[] }[] {
  return [...LOOK_VERDICTS]
    .reverse()
    .map((verdict) => ({ verdict, groups: looks.filter((g) => g.verdict === verdict) }))
    .filter((section) => section.groups.length > 0);
}
