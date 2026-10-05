import { describe, expect, it } from "vitest";
import { decodePreview } from "./previewBytes";

/** The same bytes the shell's `preview_positions_are_packed_for_the_window` test checks. */
function packed(): ArrayBuffer {
  const data = new ArrayBuffer(16 + 48 + 16);
  const view = new DataView(data);
  view.setUint32(0, 1, true);
  view.setUint32(4, 1, true);
  view.setFloat64(8, 7, true);
  new Uint8Array(data, 16, 36).set(new TextEncoder().encode("11111111-0000-4000-8000-000000000001"));
  view.setUint32(52, 6, true);
  view.setUint32(56, 3, true);
  view.setUint32(60, 2, true);
  [1.5, -2, 3, 4.25].forEach((v, i) => view.setFloat32(64 + i * 4, v, true));
  return data;
}

describe("raw pixel positions", () => {
  it("reads each prop's id, colors, and positions, and the show revision", () => {
    const { revision, props } = decodePreview(packed());
    expect(revision).toBe(7);
    expect(props).toHaveLength(1);
    expect(props[0]).toMatchObject({ prop: "11111111-0000-4000-8000-000000000001", frameOffset: 6, channelsPerPixel: 3 });
    expect(Array.from(props[0].points)).toEqual([1.5, -2, 3, 4.25]);
  });

  it("reads the bytes however Tauri hands them over: a buffer, a view into one, or plain numbers", () => {
    const bytes = new Uint8Array(packed());
    const padded = new Uint8Array(bytes.length + 3);
    padded.set(bytes, 3);
    for (const data of [bytes, padded.subarray(3), Array.from(bytes)]) {
      const { revision, props } = decodePreview(data);
      expect(revision).toBe(7);
      expect(props[0].prop).toBe("11111111-0000-4000-8000-000000000001");
      expect(Array.from(props[0].points)).toEqual([1.5, -2, 3, 4.25]);
    }
    expect(() => decodePreview({} as ArrayBuffer)).toThrow(/damaged/);
  });

  it("reads a show with no props", () => {
    const data = new ArrayBuffer(16);
    new DataView(data).setUint32(0, 1, true);
    expect(decodePreview(data)).toEqual({ revision: 0, props: [] });
  });

  it("refuses bytes that don't add up", () => {
    expect(() => decodePreview(new ArrayBuffer(4))).toThrow(/damaged/);
    expect(() => decodePreview(packed().slice(0, 70))).toThrow(/damaged/);
  });
});
