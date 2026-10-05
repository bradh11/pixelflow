// Reads the pixel positions the desktop shell sends as raw bytes (see `encode_preview` in
// src-tauri/src/layout.rs). A big show's positions as JSON would be megabytes of text to parse
// after every edit; these are read in place.

import type { PreviewProp, PreviewProp3d, PreviewSet, PreviewSet3d } from "./types";

const HEADER = 16;
const ENTRY = 48;
const ID_LENGTH = 36;

const littleEndian = new Uint8Array(new Uint16Array([1]).buffer)[0] === 1;

/**
 * The bytes as an ArrayBuffer of their own. Tauri hands raw replies over as an ArrayBuffer, but
 * as a plain array of numbers when its fast channel is unavailable.
 */
function ownBuffer(data: ArrayBuffer | ArrayBufferView | number[]): ArrayBuffer | null {
  if (data instanceof ArrayBuffer) return data;
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength).slice().buffer;
  if (Array.isArray(data)) return Uint8Array.from(data).buffer;
  return null;
}

interface Entry {
  prop: string;
  frameOffset: number;
  channelsPerPixel: number;
}

type Bytes = ArrayBuffer | ArrayBufferView | number[];

/**
 * Each prop's entry and coordinates from `bytes` in `format` (1: x, y pairs; 2: x, y, z triples),
 * which the shell packed little-endian.
 */
function decode(bytes: Bytes, format: 1 | 2): { revision: number; entries: (Entry & { coords: Float32Array })[] } {
  const damaged = () => new Error("The props' positions came back damaged. Try again.");
  const per = format === 1 ? 2 : 3;
  const data = ownBuffer(bytes);
  if (!data || data.byteLength < HEADER) throw damaged();
  const view = new DataView(data);
  if (view.getUint32(0, true) !== format) throw damaged();
  const count = view.getUint32(4, true);
  const revision = view.getFloat64(8, true);
  const floatsAt = HEADER + ENTRY * count;
  if (data.byteLength < floatsAt) throw damaged();

  const ascii = new TextDecoder("ascii");
  const entries: (Entry & { pixels: number })[] = [];
  let pixels = 0;
  for (let i = 0; i < count; i++) {
    const at = HEADER + ENTRY * i;
    const entry = {
      prop: ascii.decode(new Uint8Array(data, at, ID_LENGTH)),
      frameOffset: view.getUint32(at + ID_LENGTH, true),
      channelsPerPixel: view.getUint32(at + ID_LENGTH + 4, true),
      pixels: view.getUint32(at + ID_LENGTH + 8, true),
    };
    entries.push(entry);
    pixels += entry.pixels;
  }
  if (data.byteLength !== floatsAt + pixels * per * 4) throw damaged();

  // Read in place on little-endian machines (all of them, in practice); copied otherwise.
  let floats: Float32Array;
  if (littleEndian) {
    floats = new Float32Array(data, floatsAt, pixels * per);
  } else {
    floats = new Float32Array(pixels * per);
    for (let i = 0; i < floats.length; i++) floats[i] = view.getFloat32(floatsAt + i * 4, true);
  }
  let next = 0;
  return {
    revision,
    entries: entries.map(({ pixels: n, ...entry }) => {
      const coords = floats.subarray(next, next + n * per);
      next += n * per;
      return { ...entry, coords };
    }),
  };
}

/** The props' pixel positions (front view) from `bytes`, which the shell packed little-endian. */
export function decodePreview(bytes: Bytes): PreviewSet {
  const { revision, entries } = decode(bytes, 1);
  const props: PreviewProp[] = entries.map(({ coords, ...entry }) => ({ ...entry, points: coords }));
  return { revision, props };
}

/** The props' pixel positions in depth (x, y, z) from `bytes`, which the shell packed little-endian. */
export function decodePreview3d(bytes: Bytes): PreviewSet3d {
  const { revision, entries } = decode(bytes, 2);
  const props: PreviewProp3d[] = entries.map(({ coords, ...entry }) => ({ ...entry, xyz: coords }));
  return { revision, props };
}
