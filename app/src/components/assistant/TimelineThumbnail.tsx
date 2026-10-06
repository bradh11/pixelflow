import type { TimelineView } from "../../api/assistant";

/** Height of one row in the thumbnail (SVG units). */
const ROW = 6;
const WIDTH = 320;

/**
 * A sequence proposal's draft drawn small: one line per row with its effects in their first
 * color, and the song's sections marked across the top.
 */
export function TimelineThumbnail({ timeline }: { timeline: TimelineView }) {
  const { durationMs, rows, sections, moreRows } = timeline;
  const x = (ms: number) => (Math.min(Math.max(ms, 0), durationMs) / Math.max(durationMs, 1)) * WIDTH;
  const top = 10;
  const height = top + Math.max(rows.length, 1) * ROW;
  const effects = rows.reduce((n, r) => n + r.effects.length, 0);
  return (
    <figure className="mt-2">
      <svg
        role="img"
        aria-label={`Timeline of the draft: ${rows.length} rows, ${effects} effects`}
        viewBox={`0 0 ${WIDTH} ${height}`}
        preserveAspectRatio="none"
        className="h-auto w-full rounded border border-neutral-200 bg-neutral-950 dark:border-neutral-800"
        style={{ maxHeight: "10rem" }}
      >
        {sections.map((s, i) => (
          <g key={`${s.label}-${i}`}>
            {i > 0 && <line x1={x(s.startMs)} x2={x(s.startMs)} y1={0} y2={height} stroke="#a3a3a3" strokeWidth={0.5} strokeDasharray="2 2" />}
            <text x={x(s.startMs) + 2} y={7} fontSize={6} fill="#d4d4d4">
              {s.label}
            </text>
          </g>
        ))}
        {rows.map((row, r) =>
          row.effects.map((e, i) => (
            <rect
              key={`${r}-${i}`}
              x={x(e.startMs)}
              y={top + r * ROW + 0.5}
              width={Math.max(0.6, x(e.endMs) - x(e.startMs) - 0.3)}
              height={ROW - 1}
              fill={e.color}
              rx={0.8}
            >
              <title>{row.name}</title>
            </rect>
          )),
        )}
      </svg>
      {moreRows > 0 && <figcaption className="mt-0.5 text-xs text-neutral-500">…and {moreRows} more rows.</figcaption>}
    </figure>
  );
}
