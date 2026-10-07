import { AlertTriangle, ArrowRight } from "lucide-react";
import type { ReactNode } from "react";
import type { Change } from "../../api/types";

/** Changes grouped as they're shown: the controller as a whole first, then port by port. */
export function groupChanges(changes: Change[]): { title: string; changes: Change[] }[] {
  const groups: { title: string; port: number | null; changes: Change[] }[] = [];
  for (const change of changes) {
    let group = groups.find((g) => g.port === change.port);
    if (!group) {
      group = { title: change.port === null ? "Receiving" : `Port ${change.port}`, port: change.port, changes: [] };
      groups.push(group);
    }
    group.changes.push(change);
  }
  return groups.sort((a, b) => (a.port ?? -1) - (b.port ?? -1));
}

function Row({ change, extra }: { change: Change; extra?: ReactNode }) {
  return (
    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
      <span className="flex flex-wrap items-baseline gap-x-2">
        {change.subject && <span className="font-medium">{change.subject}</span>}
        <span className="text-neutral-600 dark:text-neutral-300">{change.what}</span>
        <span className="ml-auto flex items-center gap-1.5 tabular-nums">
          <span className="text-neutral-500 dark:text-neutral-400">{change.before}</span>
          <ArrowRight size={12} className="shrink-0 text-neutral-400" aria-label="becomes" />
          <span className="font-medium">{change.after}</span>
        </span>
      </span>
      {change.warning && (
        <span className="flex items-start gap-1 text-xs text-amber-700 dark:text-amber-400">
          <AlertTriangle size={12} className="mt-0.5 shrink-0" aria-hidden /> {change.warning}
        </span>
      )}
      {extra}
    </span>
  );
}

/**
 * Before → after rows, grouped by port, with a warning under each change that turns something
 * off. With `picked`, each row the show can take has a checkbox (`onToggle`), and `extra` adds
 * controls under a row (choosing the prop for a new string).
 */
export function SetupChanges({
  changes,
  picked,
  onToggle,
  extra,
  label,
}: {
  changes: Change[];
  picked?: Set<string>;
  onToggle?: (id: string) => void;
  extra?: (change: Change) => ReactNode;
  label: string;
}) {
  return (
    <div role="group" aria-label={label} className="flex flex-col gap-3">
      {groupChanges(changes).map((group) => (
        <section key={group.title} aria-label={group.title} className="flex flex-col">
          <h3 className="mb-1 text-xs font-semibold tracking-wide text-neutral-500 uppercase dark:text-neutral-400">{group.title}</h3>
          <ul className="flex flex-col divide-y divide-neutral-100 rounded-md border border-neutral-200 dark:divide-neutral-800 dark:border-neutral-800">
            {group.changes.map((change) => (
              <li key={change.id} className="px-2.5 py-1.5">
                {picked ? (
                  <div className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      className="mt-1 shrink-0"
                      checked={picked.has(change.id)}
                      disabled={!change.canTake}
                      onChange={() => onToggle?.(change.id)}
                      aria-label={`Take ${[change.subject, change.what].filter(Boolean).join(" ")}: ${change.before} to ${change.after}`}
                    />
                    <Row
                      change={change}
                      extra={
                        <>
                          {!change.canTake && change.whyNot && <span className="text-xs text-neutral-500">{change.whyNot}</span>}
                          {picked.has(change.id) && extra?.(change)}
                        </>
                      }
                    />
                  </div>
                ) : (
                  <Row change={change} extra={extra?.(change)} />
                )}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
