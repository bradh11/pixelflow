import { describe, expect, it } from "vitest";
import { scheduleDates, scheduleDays, scheduleRepeat, scheduleStop, scheduleTime } from "./fppSchedule";

describe("FPP schedule in words", () => {
  it("names FPP's day codes and day masks", () => {
    expect(scheduleDays(0)).toBe("Sunday");
    expect(scheduleDays(6)).toBe("Saturday");
    expect(scheduleDays(7)).toBe("Every day");
    expect(scheduleDays(13)).toBe("Fri, Sat");
    expect(scheduleDays(0x10000 | 0x4000 | 0x200 | 0x100)).toBe("Sun, Fri, Sat");
    expect(scheduleDays(0x10000 | 0x2000)).toBe("Monday");
    expect(scheduleDays(0x10000 | 0x7f00)).toBe("Every day");
  });

  it("reads clock times and sun times", () => {
    expect(scheduleTime("17:30:00")).toBe("5:30 PM");
    expect(scheduleTime("00:05:00")).toBe("12:05 AM");
    expect(scheduleTime("12:00:00")).toBe("12:00 PM");
    expect(scheduleTime("24:00:00")).toBe("Midnight");
    expect(scheduleTime("SunSet", 15)).toBe("Sunset + 15 min");
    expect(scheduleTime("Dawn", -30)).toBe("Dawn − 30 min");
    expect(scheduleTime("SunRise")).toBe("Sunrise");
  });

  it("reads date ranges, every-year dates, and holidays", () => {
    expect(scheduleDates("", "")).toBe("All year");
    expect(scheduleDates("2019-01-01", "2099-12-31")).toBe("All year");
    expect(scheduleDates("2026-11-27", "2027-01-06")).toBe("Nov 27, 2026 – Jan 6, 2027");
    expect(scheduleDates("0000-12-01", "0000-12-31")).toBe("Dec 1 – Dec 31");
    expect(scheduleDates("Thanksgiving", "")).toBe("From Thanksgiving");
  });

  it("says how an entry repeats and stops", () => {
    expect(scheduleRepeat(0)).toBeNull();
    expect(scheduleRepeat(1)).toBe("Repeats");
    expect(scheduleRepeat(1500)).toBe("Repeats every 15 min");
    expect(scheduleStop(0)).toBe("Stops gracefully");
    expect(scheduleStop(1)).toBe("Stops at once");
    expect(scheduleStop(2)).toBe("Stops after the loop");
  });
});
