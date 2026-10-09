import { describe, expect, it } from "vitest";
import { MIN_TAPS, calibrateTaps, frameTarget, musicAt, nextReading, previewPosition, readingFrom, smooth, tapLag } from "./avSync";

describe("following the music between answers", () => {
  it("takes an answer as read halfway through the question, and carries it on at the speed it plays", () => {
    const reading = readingFrom(10_000, 1, true, 100, 120);
    expect(reading.atMs).toBe(110);
    expect(musicAt(reading, 110)).toBe(10_000);
    expect(musicAt(reading, 160)).toBe(10_050);
    const slowed = readingFrom(10_000, 0.5, true, 100, 120);
    expect(musicAt(slowed, 210)).toBe(10_050);
    const paused = readingFrom(10_000, 1, false, 100, 120);
    expect(musicAt(paused, 5_000)).toBe(10_000);
  });

  it("smooths a jittery answer but follows a jump, a pause, or a new speed at once", () => {
    const first = readingFrom(1_000, 1, true, 0, 0);
    // 20 ms early by the estimate: a quarter of the gap is closed.
    const jittery = nextReading(first, readingFrom(1_080, 1, true, 100, 100));
    expect(musicAt(jittery, 100)).toBe(1_095);
    const jumped = nextReading(first, readingFrom(5_000, 1, true, 100, 100));
    expect(musicAt(jumped, 100)).toBe(5_000);
    const paused = nextReading(first, readingFrom(1_090, 1, false, 100, 100));
    expect(musicAt(paused, 900)).toBe(1_090);
    const slowed = nextReading(first, readingFrom(1_090, 0.5, true, 100, 100));
    expect(musicAt(slowed, 100)).toBe(1_090);
    expect(nextReading(null, first)).toBe(first);
  });
});

describe("the preview's moment", () => {
  it("is the music moved by the offset, within the song", () => {
    expect(previewPosition(1_000, 120, 60_000)).toBe(1_120);
    expect(previewPosition(1_000, -120, 60_000)).toBe(880);
    expect(previewPosition(50, -120, 60_000)).toBe(0);
    expect(previewPosition(59_950, 120, 60_000)).toBe(60_000);
  });

  it("asks for the moment heard when the frame reaches the screen", () => {
    const reading = readingFrom(2_000, 1, true, 0, 0);
    // 30 ms to fetch, 16 ms to paint, 10 ms later than the engine counts.
    expect(frameTarget(reading, 0, 30, 16, -10, 60_000)).toBe(2_036);
    // At half speed the music moves half as far meanwhile.
    expect(frameTarget({ ...reading, speed: 0.5 }, 0, 30, 16, 0, 60_000)).toBe(2_023);
  });

  it("keeps a running average", () => {
    expect(smooth(null, 10)).toBe(10);
    expect(smooth(10, 20, 0.5)).toBe(15);
  });
});

describe("tapping along", () => {
  it("measures each tap from its nearest click", () => {
    expect(tapLag(1_030, 500)).toBe(30);
    expect(tapLag(980, 500)).toBe(-20);
    expect(tapLag(-20, 500)).toBe(-20);
    expect(tapLag(1_250, 500)).toBe(250);
  });

  it("suggests running the picture behind by how late the taps come", () => {
    const taps = [40, 545, 1_035, 1_540, 2_045, 2_540, 3_035, 3_540].map((t) => t + 2_000);
    const result = calibrateTaps(taps, 500)!;
    expect(result.lagMs).toBeCloseTo(40, 0);
    expect(result.offsetMs).toBe(-40);
    expect(result.kept).toBe(8);
    expect(result.left).toBe(0);
    expect(result.spreadMs).toBeLessThan(5);
  });

  it("and ahead when the taps come early", () => {
    const taps = [0, 500, 1_000, 1_500, 2_000, 2_500].map((t) => t - 25);
    expect(calibrateTaps(taps, 500)!.offsetMs).toBe(25);
  });

  it("leaves out slips", () => {
    const steady = [30, 530, 1_028, 1_532, 2_030, 2_529, 3_031];
    // One tap a beat late is on the next click; these two are just wrong.
    const result = calibrateTaps([...steady, 3_700, 4_150], 500)!;
    expect(result.left).toBe(2);
    expect(result.kept).toBe(7);
    expect(result.offsetMs).toBe(-30);
  });

  it("needs enough taps, and stays within the range", () => {
    expect(calibrateTaps([30, 530, 1_030], 500)).toBeNull();
    expect(MIN_TAPS).toBeGreaterThan(3);
    const late = Array.from({ length: 8 }, (_, i) => i * 1_000 + 450);
    expect(calibrateTaps(late, 1_000)!.offsetMs).toBe(-300);
  });
});
