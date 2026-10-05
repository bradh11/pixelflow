// Reads the pixel positions the desktop shell sends as raw bytes (see `encode_preview` in
// src-tauri/src/layout.rs). A big show's positions as JSON would be megabytes of text to parse
// after every edit; these are read in place.

import type { PreviewProp, PreviewSet } from "./types";

const HEADER = 16;
const ENTRY = 48;
const ID_LENGTH = 36;

const littleEndian = new Uint8Array(new Uint16Array([1]).buffer)[0] === 1;

/** The props' pixel positions from `data`, which the shell packed little-endian. */
export function decodePreview(data: ArrayBuffer): PreviewSet {
  const damaged = () => new Error("The props' positions came back damaged. Try again.");
  if (data.byteLength < HEADER) throw damaged();
  const view = new DataView(data);
  if (view.getUint32(0, true) !== 1) throw damaged();
  const count = view.getUint32(4, true);
  const revision = view.getFloat64(8, true);
  const floatsAt = HEADER + ENTRY * count;
  if (data.byteLength < floatsAt) throw damaged();

  const ascii = new TextDecoder("ascii");
  const entries: { prop: string; frameOffset: number; channelsPerPixel: number; pixels: number }[] = [];
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
  if (data.byteLength !== floatsAt + pixels * 8) throw damaged();

  // Read in place on little-endian machines (all of them, in practice); copied otherwise.
  let floats: Float32Array;
  if (littleEndian) {
    floats = new Float32Array(data, floatsAt, pixels * 2);
  } else {
    floats = new Float32Array(pixels * 2);
    for (let i = 0; i < floats.length; i++) floats[i] = view.getFloat32(floatsAt + i * 4, true);
  }
  const props: PreviewProp[] = [];
  let next = 0;
  for (const { pixels: n, ...entry } of entries) {
    props.push({ ...entry, points: floats.subarray(next, next + n * 2) });
    next += n * 2;
  }
  return { revision, props };
}
