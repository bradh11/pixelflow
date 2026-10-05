// A stand-in for the desktop app's engine in tests: the in-memory backend, but with the
// engine's own revision numbers (a real engine has counted every show opened and every edit
// since it started) and pixel positions that arrive as raw bytes, packed exactly like
// `encode_preview` in src-tauri/src/layout.rs and read back with the desktop's `decodePreview`.

import type { Edit, PreviewSet, PreviewSet3d, ShowSnapshot } from "../api/types";
import { MemoryBackend } from "../api/memory";
import { decodePreview } from "../api/previewBytes";

/** The bytes the shell's `encode_preview` sends: header, one 48-byte entry per prop, then f32 x, y pairs. */
export function encodePreview({ revision, props }: PreviewSet): ArrayBuffer {
  const floats = props.reduce((sum, p) => sum + p.points.length, 0);
  const data = new ArrayBuffer(16 + 48 * props.length + 4 * floats);
  const view = new DataView(data);
  view.setUint32(0, 1, true);
  view.setUint32(4, props.length, true);
  view.setFloat64(8, revision, true);
  props.forEach((p, i) => {
    const at = 16 + 48 * i;
    new Uint8Array(data, at, 36).set(new TextEncoder().encode(p.prop));
    view.setUint32(at + 36, p.frameOffset, true);
    view.setUint32(at + 40, p.channelsPerPixel, true);
    view.setUint32(at + 44, p.points.length / 2, true);
  });
  let at = 16 + 48 * props.length;
  for (const p of props) {
    for (let i = 0; i < p.points.length; i++, at += 4) view.setFloat32(at, p.points[i], true);
  }
  return data;
}

/** How far ahead of the in-memory backend's revisions the "engine" is. */
const ENGINE_REVISION_OFFSET = 1041;

export class DesktopLikeBackend extends MemoryBackend {
  private engine = (s: ShowSnapshot): ShowSnapshot => ({ ...s, revision: s.revision + ENGINE_REVISION_OFFSET });

  override async getSnapshot() {
    return this.engine(await super.getSnapshot());
  }

  override async applyEdits(edits: Edit[]) {
    return this.engine(await super.applyEdits(edits));
  }

  override async undo() {
    return this.engine(await super.undo());
  }

  override async redo() {
    return this.engine(await super.redo());
  }

  override async previewProps(): Promise<PreviewSet> {
    const memory = await super.previewProps();
    return decodePreview(encodePreview({ ...memory, revision: memory.revision + ENGINE_REVISION_OFFSET }));
  }

  override async previewProps3d(): Promise<PreviewSet3d> {
    const memory = await super.previewProps3d();
    return { ...memory, revision: memory.revision + ENGINE_REVISION_OFFSET };
  }
}
