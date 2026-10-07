import type { ScheduleEntry } from "../api/types";

const DAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DAY_CODES: Record<number, string> = {
  7: "Every day",
  8: "Weekdays",
  9: "Weekends",
  10: "Mon, Wed, Fri",
  11: "Tue, Thu",
  12: "Sun–Thu",
  13: "Fri, Sat",
  14: "Odd days",
  15: "Even days",
};
const DAY_MASK = 0x10000;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const SUN_TIMES: Record<string, string> = { sunrise: "Sunrise", sunset: "Sunset", dawn: "Dawn", dusk: "Dusk" };

/** FPP's day code in words: "Every day", "Weekends", "Saturday", "Sun, Fri, Sat". */
export function scheduleDays(day: number): string {
  if (day >= DAY_MASK) {
    // One bit per day, Sunday (0x4000) down to Saturday (0x100).
    const days = DAY_NAMES.filter((_, i) => day & (0x4000 >> i));
    if (days.length === 7) return "Every day";
    if (days.length === 0) return "No days";
    return days.length === 1 ? days[0] : days.map((d) => d.slice(0, 3)).join(", ");
  }
  return DAY_NAMES[day] ?? DAY_CODES[day] ?? "Some days";
}

/** "17:30:00" → "5:30 PM"; "SunSet" with 15 → "Sunset + 15 min". */
export function scheduleTime(time: string, offset = 0): string {
  const sun = SUN_TIMES[time.trim().toLowerCase()];
  if (sun) return offset === 0 ? sun : `${sun} ${offset > 0 ? "+" : "−"} ${Math.abs(offset)} min`;
  const match = /^(\d{1,2}):(\d{2})/.exec(time.trim());
  if (!match) return time.trim() || "—";
  const hour = Number(match[1]);
  if (hour === 24) return "Midnight";
  return `${hour % 12 || 12}:${match[2]} ${hour < 12 ? "AM" : "PM"}`;
}

function scheduleDate(date: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date.trim());
  if (!match) return date.trim();
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const md = `${MONTHS[month - 1] ?? "?"} ${day}`;
  return year === 0 ? md : `${md}, ${year}`;
}

/** The entry's date range: "All year", "Nov 25, 2026 – Jan 6, 2027", "From Dec 1". */
export function scheduleDates(start: string, end: string): string {
  // FPP fills an empty range with 2019-01-01 – 2099-12-31.
  const from = start.trim() === "" || start.trim() === "2019-01-01" ? "" : scheduleDate(start);
  const to = end.trim() === "" || end.trim() === "2099-12-31" ? "" : scheduleDate(end);
  if (!from && !to) return "All year";
  if (!to) return `From ${from}`;
  if (!from) return `Until ${to}`;
  return `${from} – ${to}`;
}

/** How the entry repeats, or null when it plays once. */
export function scheduleRepeat(repeat: number): string | null {
  if (repeat === 0) return null;
  if (repeat === 1) return "Repeats";
  return `Repeats every ${Math.round(repeat / 100)} min`;
}

/** How it stops at its end time. */
export function scheduleStop(stopType: number): string {
  return stopType === 1 ? "Stops at once" : stopType === 2 ? "Stops after the loop" : "Stops gracefully";
}

/** One line of when an entry runs: "Every day · 5:30 PM – 10:00 PM · Nov 25, 2026 – Jan 6, 2027". */
export function scheduleWhen(entry: ScheduleEntry): string {
  const times =
    entry.kind === "command" || entry.endTime === entry.startTime
      ? scheduleTime(entry.startTime, entry.startOffset)
      : `${scheduleTime(entry.startTime, entry.startOffset)} – ${scheduleTime(entry.endTime, entry.endOffset)}`;
  return [scheduleDays(entry.day), times, scheduleDates(entry.startDate, entry.endDate)].join(" · ");
}
