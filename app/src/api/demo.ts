import type { Show } from "./types";
import { emptyShow } from "./memory";
import { newController, newProp } from "../lib/shows";

/** A small sample show for browser-only UI development (`?demo`). */
export function demoShow(): Show {
  const show = emptyShow("Demo House");
  const arch = { ...newProp("arch", show), name: "Garage Arch" };
  const tree = { ...newProp("tree", show), name: "Mega Tree" };
  const matrix = { ...newProp("matrix", show), name: "Window Matrix", colorOrder: "GRB" as const };
  const star = { ...newProp("star", show), name: "Porch Star" };
  show.props = [arch, tree, matrix, star];
  const fpp = newController("Main FPP", "192.168.1.50", "sacn", 4);
  fpp.ports[0].slots = [arch, matrix].map((p) => ({
    prop: p.id,
    segment: null,
    nullPixels: 0,
    reverse: false,
    brightness: null,
    gamma: null,
    smartReceiver: null,
  }));
  fpp.ports[1].slots = [
    { prop: tree.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null },
  ];
  const wled = newController("Porch WLED", "192.168.1.60", "ddp", 1);
  show.controllers = [fpp, wled];
  return show;
}
