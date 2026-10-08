import { VENDOR_AUTO_MAP, type VendorInspection, type VendorItem, type VendorMapping } from "../api/types";

/** Where `item` goes in `mapping` (none when it isn't mapped). */
export function targetsOf(mapping: VendorMapping, item: string): string[] {
  return mapping.items[item] ?? [];
}

/** How much of the sequence the mapping brings in. */
export function mappingStats(items: VendorItem[], mapping: VendorMapping) {
  const withEffects = items.filter((i) => i.effects > 0);
  const mapped = withEffects.filter((i) => targetsOf(mapping, i.name).length > 0);
  const effects = withEffects.reduce((n, i) => n + i.effects, 0);
  const mappedEffects = mapped.reduce((n, i) => n + i.effects, 0);
  return {
    mapped: mapped.length,
    total: withEffects.length,
    effects,
    mappedEffects,
    percent: effects === 0 ? 100 : Math.round((mappedEffects / effects) * 100),
  };
}

/** The mapping the suggestions make: those confident enough, and the ones saved last time. */
export function autoMapping(inspection: VendorInspection): VendorMapping {
  const items: Record<string, string[]> = {};
  for (const s of inspection.suggestions) {
    if (s.reason === "saved" || (s.confidence >= VENDOR_AUTO_MAP && s.targets.length > 0)) items[s.item] = [...s.targets];
  }
  return { items };
}

/** `mapping` with `loaded` (from an .xmap) laid over it, for the items this sequence has; how
 * many of the loaded mappings name items it doesn't. */
export function withLoaded(items: VendorItem[], mapping: VendorMapping, loaded: VendorMapping): { mapping: VendorMapping; used: number; unused: number } {
  const known = new Set(items.map((i) => i.name));
  const next: Record<string, string[]> = { ...mapping.items };
  let used = 0;
  let unused = 0;
  for (const [item, targets] of Object.entries(loaded.items)) {
    if (!known.has(item)) {
      unused++;
      continue;
    }
    next[item] = [...targets];
    used++;
  }
  return { mapping: { items: next }, used, unused };
}

/** Vendor items as the dialog lists them: each model (busiest first) followed by its submodels
 * and strands (busiest first). */
export function itemTree(items: VendorItem[]): { item: VendorItem; children: VendorItem[] }[] {
  const children = new Map<string, VendorItem[]>();
  for (const i of items) {
    if (i.parent !== null) children.set(i.parent, [...(children.get(i.parent) ?? []), i]);
  }
  const total = (i: VendorItem) => i.effects + (children.get(i.name) ?? []).reduce((n, c) => n + c.effects, 0);
  return items
    .filter((i) => i.parent === null)
    .sort((a, b) => total(b) - total(a))
    .map((item) => ({ item, children: [...(children.get(item.name) ?? [])].sort((a, b) => b.effects - a.effects) }));
}
