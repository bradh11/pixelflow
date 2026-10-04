import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import { channelsPerPixel, newController, newProp, nodeCount, shapeLabel, uniqueName } from "./shows";

describe("show helpers", () => {
  it("counts pixels like the engine", () => {
    expect(nodeCount({ source: "generator", type: "line", nodes: 50, length: 1 })).toBe(50);
    expect(nodeCount({ source: "generator", type: "matrix", columns: 32, rows: 16, width: 1, height: 1 })).toBe(512);
    expect(
      nodeCount({ source: "generator", type: "tree", strings: 16, nodesPerString: 50, height: 1, baseRadius: 1, topRadius: 0 }),
    ).toBe(800);
    expect(nodeCount({ source: "generator", type: "customGrid", columns: 3, rows: 1, cells: [2, 0, 5] })).toBe(5);
    expect(nodeCount({ source: "measured", points: [{ x: 0, y: 0, z: 0 }], provenance: "cameraMap" })).toBe(1);
  });

  it("picks the first unused numbered name", () => {
    expect(uniqueName("Arch", [])).toBe("Arch 1");
    expect(uniqueName("Arch", ["Arch 1", "Arch 3"])).toBe("Arch 2");
  });

  it("creates props with unique names, fresh ids, and identity transforms", () => {
    const show = emptyShow("t");
    const a = newProp("arch", show);
    show.props.push(a);
    const b = newProp("arch", show);
    expect(a.name).toBe("Arch 1");
    expect(b.name).toBe("Arch 2");
    expect(a.id).not.toBe(b.id);
    expect(a.transform.scale).toEqual({ x: 1, y: 1, z: 1 });
    expect(shapeLabel(a.shape)).toBe("Arch");
    expect(channelsPerPixel(a)).toBe(3);
    expect(channelsPerPixel({ ...a, colorOrder: "GRBW" })).toBe(4);
  });

  it("creates controllers with numbered empty ports and the chosen protocol", () => {
    const ddp = newController("WLED", "10.0.0.5", "ddp", 2);
    expect(ddp.protocol).toEqual({ type: "ddp" });
    expect(ddp.ports.map((p) => p.number)).toEqual([1, 2]);
    expect(ddp.ports[0]).toEqual({ number: 1, maxPixels: null, brightness: 100, gamma: 1, slots: [] });
    const sacn = newController("FPP", "10.0.0.6", "sacn", 1);
    expect(sacn.protocol).toEqual({ type: "sacn", startUniverse: null, universeSize: 510, allowPixelStraddle: false, multicast: false });
  });
});
