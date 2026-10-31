#!/usr/bin/env node
// Takes the screenshots in docs/images/ again, after the UI has changed.
//
//   node scripts/screenshots.mjs                 every screenshot of the app
//   node scripts/screenshots.mjs --only wiring,test-screen
//   node scripts/screenshots.mjs --list          the screenshots, and the guides that use them
//   node scripts/screenshots.mjs --engine        also the picture of the show lit up (below)
//
// The app's screens come from the web UI's browser stand-in (`?demo`: a made-up show,
// sequence, devices, and players; nothing is sent anywhere), served by Vite's dev server from
// app/ on port 5199 and driven by Chrome without a window over the DevTools protocol. Each
// screenshot starts from a fresh page, at 1440 × 900 in the default light look, and is taken at
// twice that and scaled to at most 1600 pixels wide. The stand-in draws effects roughly
// (app/src/api/memoryRender.ts), so a shot's point is the screen, never how an effect looks.
// What a person would do by dragging on the timeline (move the playhead, pick an effect) and a
// few things the stand-in has no way to show (a test's packet counts, a USB serial port) are
// set through the page's own modules, which is why it needs the dev server.
//
// --engine draws how the show looks with PixelFlow's own engine instead: the Assistant
// harness's scripted whole-song draft on its made-up stage (docs/dev/assistant-eval.md), as a
// short animated GIF. It needs cargo and ffmpeg, and builds the harness in release mode.
//
// Needs: Node 22 or newer, the app's dependencies (`make setup`), and Google Chrome or
// Chromium (found where it usually is, or named by the CHROME environment variable). With
// ffmpeg on the PATH the PNGs are reduced to 256 colours, which halves them; without it they are
// written as they are. Uses nothing but Node itself: no package to install.
//
// Options: --out <folder> (default docs/images), --url <address> to use a UI dev server already
// running instead of starting one, --port <n> for the one it starts (default 5199),
// --chrome-port <n> to drive a Chrome already started with --remote-debugging-port=<n>, and
// --width <n> to see the screens in a narrower window than 1440 (for checking a layout; the
// guides' pictures are taken at 1440).

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, renameSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const APP = path.join(ROOT, "app");
const WIDTH = Number(option("width") ?? 1440);
const HEIGHT = 900;
/** The widest a picture is written, in pixels. */
const MAX_WIDTH = 1600;

// ---------------------------------------------------------------------------------------------
// The screenshots. Each starts on a fresh `?demo` page (or the one `query` names) and returns
// what to capture: an element's box (a selector, or a button's text), or a box of its own.

/** @type {{ name: string, file: string, usedBy: string, query?: string, run: (ui: Ui) => Promise<Clip> }[]} */
const SHOTS = [
  {
    name: "welcome",
    file: "welcome.png",
    usedBy: "first-show",
    query: "demo&start",
    async run(ui) {
      // A first start: no recent shows.
      await ui.click("Clear recent shows");
      await ui.waitGone("Clear recent shows");
      return ui.parentBox("h1");
    },
  },
  {
    name: "layout",
    file: "layout.png",
    usedBy: "README, first-show",
    async run(ui) {
      await ui.click("Window Matrix", { exact: false, within: "[aria-label=Props]" });
      await ui.settle(600);
      return ui.window();
    },
  },
  {
    name: "setup-checklist",
    file: "setup-checklist.png",
    usedBy: "first-show",
    async run(ui) {
      await ui.click("Show all steps");
      return ui.box("section[aria-label='Set up your show']", { pad: 8 });
    },
  },
  {
    name: "layout-add-prop",
    file: "layout-add-prop.png",
    usedBy: "first-show, dmx",
    async run(ui) {
      await ui.click("Add prop");
      const menu = await ui.box("[role=menu]", { pad: 0 });
      const button = await ui.box("Add prop", { pad: 0 });
      return padded({ x: menu.x - 420, y: button.y - 60, width: button.x + button.width - menu.x + 420, height: menu.y + menu.height - button.y + 60 }, 12);
    },
  },
  {
    name: "layout-color-order",
    file: "layout-color-order.png",
    usedBy: "troubleshooting",
    async run(ui) {
      await ui.click("Window Matrix", { within: "[aria-label=Props]" });
      await ui.click("More: color order and scale");
      await ui.scrollTo("Color order", "[aria-label=Properties]");
      const panel = await ui.box("[aria-label=Properties]", { pad: 0 });
      const from = await ui.box("Placement", { sel: "h3,h4,p,div", pad: 0 });
      const to = await ui.box("Scale Y", { sel: "label", pad: 0 });
      return { x: panel.x, y: from.y - 14, width: panel.width, height: to.y + to.height - from.y + 28 };
    },
  },
  {
    name: "controllers-scan",
    file: "controllers-scan.png",
    usedBy: "first-show, troubleshooting",
    async run(ui) {
      await ui.screen("Controllers");
      await ui.click("Find controllers");
      await ui.waitForText("Find controllers again", { timeoutMs: 20000 });
      await ui.settle(500);
      const top = await ui.box("Controllers", { sel: "h1,h2", pad: 0 });
      const bottom = await ui.box("isn't responding", { pad: 0 });
      return padded({ x: top.x, y: top.y, width: WIDTH - top.x - 20, height: bottom.y + bottom.height - top.y }, 16);
    },
  },
  {
    name: "fpp-page",
    file: "fpp-page.png",
    usedBy: "README, first-show, troubleshooting",
    async run(ui) {
      await ui.screen("Controllers");
      await ui.click("Find controllers");
      await ui.click("Open FPP", { exact: true, timeoutMs: 20000 });
      await ui.waitForText("Set up my show from this FPP");
      await ui.settle(800);
      return ui.window();
    },
  },
  {
    name: "send-setup",
    file: "send-setup.png",
    usedBy: "hardware-acceptance",
    async run(ui) {
      await ui.screen("Controllers");
      await ui.click("Find controllers");
      await ui.click("Send setup to this controller: Main FPP", { timeoutMs: 20000 });
      await ui.settle(1500);
      return ui.box("[role=dialog]");
    },
  },
  {
    name: "wiring",
    file: "wiring.png",
    usedBy: "README, first-show",
    pointing: true,
    async run(ui) {
      await ui.screen("Wiring");
      // Pointing at a port draws its wire on the house.
      await ui.hover("2 props · 562 px");
      return ui.window();
    },
  },
  {
    name: "wiring-slot-settings",
    file: "wiring-slot-settings.png",
    usedBy: "first-show, troubleshooting",
    async run(ui) {
      await ui.screen("Wiring");
      await ui.click("Settings for Garage Arch");
      await ui.settle(400);
      return ui.box("[aria-label='Garage Arch settings']", { pad: 12 });
    },
  },
  {
    name: "add-controller",
    file: "add-controller.png",
    usedBy: "first-show",
    async run(ui) {
      await ui.screen("Wiring");
      await ui.click("Add controller");
      await ui.type("Name", "Garage Falcon");
      await ui.choose("Controller type", "Falcon F16V5");
      await ui.type("IP address", "192.168.1.70");
      await ui.blur();
      return ui.parentBoxOf("Controller type", { up: 3, pad: 8 });
    },
  },
  {
    name: "test-screen",
    file: "test-screen.png",
    usedBy: "first-show, troubleshooting",
    async run(ui) {
      await ui.screen("Test");
      await ui.choose("Target", "Main FPP · port 1");
      await ui.choose("Pattern", "Chase");
      await ui.click("Start", { exact: true });
      await ui.sending({ frames: 1208, fps: 40, controllers: { "Main FPP": 4 } });
      const top = await ui.box("Test", { sel: "h1,h2", pad: 0 });
      const last = await ui.lastCard();
      return padded({ x: top.x, y: top.y, width: last.x + last.width - top.x, height: last.y + last.height - top.y }, 16);
    },
  },
  {
    name: "sequence",
    file: "sequence.png",
    usedBy: "README, first-show",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.seek(3400);
      await ui.selectEffect({ prop: "Mega Tree", kind: "colorWash" });
      return ui.window();
    },
  },
  {
    name: "sequence-settings",
    file: "sequence-settings.png",
    usedBy: "first-show",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.click("Sequence settings");
      return ui.boxes(["[role=dialog]", "Sequence settings"], { pad: 10 });
    },
  },
  {
    name: "new-sequence",
    file: "new-sequence.png",
    usedBy: "first-show",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.click("New sequence");
      await ui.settle(500);
      return ui.box("[role=dialog]");
    },
  },
  {
    name: "assistant",
    file: "assistant.png",
    usedBy: "README",
    async run(ui) {
      await ui.click("Assistant ⌘L");
      await ui.type("Message the assistant", "Add two arches", { enter: true });
      await ui.waitForText("Apply", { exact: true });
      await ui.settle(800);
      await ui.blur();
      return ui.window();
    },
  },
  {
    name: "assistant-sequence",
    file: "assistant-sequence.png",
    usedBy: "first-show",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.click("Assistant ⌘L");
      await ui.click("Create a compelling sequence");
      await ui.click("Choose a song…", { timeoutMs: 20000 });
      await ui.waitForText("Play preview", { timeoutMs: 30000 });
      await ui.settle(1500);
      await ui.scrollToTop("[aria-label=Assistant] *");
      return ui.box("[aria-label=Assistant]", { pad: 0 });
    },
  },
  {
    name: "play",
    file: "play.png",
    usedBy: "README, first-show",
    async run(ui) {
      await ui.screen("Play");
      await ui.seek(12600);
      return ui.window();
    },
  },
  {
    name: "play-playlist",
    file: "play-playlist.png",
    usedBy: "troubleshooting",
    async run(ui) {
      await ui.screen("Play");
      await ui.click("Playlist");
      await ui.click("Add sequence");
      await ui.settle(1500);
      const drawer = await ui.box("aside[aria-label=Playlist]", { pad: 0 });
      const music = await ui.box("section[aria-label=Music]", { pad: 0 });
      return { ...drawer, height: music.y + music.height - drawer.y + 4 };
    },
  },
  {
    name: "send-to-fpp",
    file: "send-to-fpp.png",
    usedBy: "first-show",
    async run(ui) {
      // The FPPs found on the network are offered first.
      await ui.screen("Controllers");
      await ui.click("Find controllers");
      await ui.waitForText("Find controllers again", { timeoutMs: 20000 });
      await ui.screen("Sequence");
      await ui.click("Send to FPP…");
      await ui.settle(1500);
      await ui.click("Replace it", { within: "[role=dialog]" });
      await ui.click("Use the one already on the FPP");
      return ui.box("[role=dialog]");
    },
  },
  {
    name: "export-video",
    file: "export-video.png",
    usedBy: "first-show",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.click("More ways to export");
      await ui.click("Export video…");
      await ui.settle(800);
      return ui.box("[role=dialog]");
    },
  },
  {
    name: "vendor-mapping",
    file: "vendor-mapping.png",
    usedBy: "first-show",
    async run(ui) {
      // The window says where the song's music will go, next to the show: a folder with no one's name in it.
      await ui.js(`const { useApp } = await import("/src/state/store.ts"); useApp.getState().backend.path = "/Shows/Demo House/Demo House.pixelflow.json"; return 1`);
      await ui.screen("Sequence");
      await ui.click("Import xLights sequence…");
      await ui.click("Auto-map", { timeoutMs: 20000 });
      await ui.settle(800);
      return ui.box("[role=dialog]");
    },
  },
  {
    name: "settings-preview-sync",
    file: "settings-preview-sync.png",
    usedBy: "troubleshooting",
    async run(ui) {
      await ui.click("Settings", { exact: true });
      await ui.settle(600);
      await ui.js(`find("Preview sync", { sel: "h2" })?.scrollIntoView({ block: "center" }); return 1`);
      await ui.click("Play the click");
      await ui.click("Tap along");
      await ui.settle(300);
      return ui.parentBoxOf("Preview sync", { sel: "h2", up: 2, pad: 8 });
    },
  },
  {
    name: "dmx-fixture",
    file: "dmx-fixture.png",
    usedBy: "dmx",
    async run(ui) {
      await addFlood(ui);
      await ui.click("Each channel");
      await ui.scrollTo("DMX fixture", "[aria-label=Properties]", "start");
      return ui.window();
    },
  },
  {
    name: "dmx-moving-head",
    file: "dmx-moving-head.png",
    usedBy: "dmx",
    async run(ui) {
      await addHead(ui);
      await ui.click("Pan and tilt");
      await ui.scrollTo("DMX moving head", "[aria-label=Properties]", "start");
      return ui.window();
    },
  },
  {
    name: "dmx-wiring",
    file: "dmx-wiring.png",
    usedBy: "dmx",
    async run(ui) {
      await addFlood(ui);
      await addFlood(ui, { name: "Porch Par", x: 4, y: 0.6, address: 9, layout: "4 channels: dimmer, red, green, blue" });
      await wireToDmxNode(ui, ["Lawn Flood", "Porch Par"]);
      return ui.box("[aria-label='Lawn DMX']", { pad: 10 });
    },
  },
  {
    name: "dmx-usb-port",
    file: "dmx-usb-port.png",
    usedBy: "dmx",
    async run(ui) {
      // A made-up interface on a made-up port: the stand-in lists no serial ports of its own.
      await ui.js(
        `const { useApp } = await import("/src/state/store.ts");
         useApp.getState().backend.serialPorts = [
           { name: "/dev/cu.usbserial-EN123456", label: "ENTTEC DMX USB Pro (cu.usbserial-EN123456)" },
           { name: "/dev/cu.Bluetooth-Incoming-Port", label: "cu.Bluetooth-Incoming-Port" },
         ];
         return 1`,
      );
      await ui.screen("Wiring");
      await ui.click("Add controller");
      await ui.type("Name", "Porch USB DMX");
      await ui.choose("Controller type", "USB DMX interface");
      await ui.type("Serial port", "/dev/cu.usbserial-EN123456");
      await ui.blur();
      return ui.parentBoxOf("Controller type", { up: 3, pad: 8 });
    },
  },
  {
    name: "dmx-effect",
    file: "dmx-effect.png",
    usedBy: "dmx",
    async run(ui) {
      await addFlood(ui);
      await ui.screen("Sequence");
      await ui.click("Add row");
      await ui.click("Lawn Flood", { sel: "[role=option], [role=menuitem], li button, button" });
      await ui.seek(8000);
      await ui.activeRow("Lawn Flood");
      // Enter on an effect in the palette adds it at the playhead, on the row worked on.
      await ui.js(`find("DMX effect", { exact: true }).focus(); return 1`);
      await ui.key("Enter");
      await ui.settle(800);
      await ui.choose("Channel to set", "6");
      await ui.click("Set this channel");
      await ui.type("6 · Strobe value", "180", { enter: true });
      await ui.choose("Channel to set", "1");
      await ui.click("Set this channel");
      await ui.type("1 · Dimmer value", "255", { enter: true });
      return ui.box("[aria-label='Effect settings']", { pad: 0 });
    },
  },
  {
    name: "dmx-test-channels",
    file: "dmx-test-channels.png",
    usedBy: "dmx",
    async run(ui) {
      await addFlood(ui);
      await wireToDmxNode(ui, ["Lawn Flood"]);
      await ui.screen("Test");
      await ui.choose("Target", "Prop: Lawn Flood");
      await ui.choose("Pattern", "DMX channels by hand");
      for (const [channel, value] of [["Dimmer", 255], ["Red", 255], ["Green", 96]]) await ui.slide(channel, value);
      const top = await ui.box("Test", { sel: "h1,h2", pad: 0 });
      const card = await ui.parentBoxOf("Target", { sel: "label", up: 3 });
      return padded({ x: top.x, y: top.y, width: card.x + card.width - top.x, height: card.y + card.height - top.y }, 16);
    },
  },
  {
    name: "shader-settings",
    file: "shader-settings.png",
    usedBy: "shaders",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.seek(21000);
      // With the preview above the timeline, the settings have the whole side to themselves.
      await ui.click("Preview above the timeline");
      await ui.selectEffect({ prop: "Window Matrix", kind: "shader", file: "shaders/Wavy Lines.fs" });
      return ui.settingsTop("Fades");
    },
  },
  {
    name: "shader-refused",
    file: "shader-refused.png",
    usedBy: "shaders",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.seek(25000);
      await ui.selectEffect({ prop: "Window Matrix", kind: "shader", file: "shaders/Broken Rings.fs" });
      return ui.settingsTop("Fades");
    },
  },
  {
    name: "shader-stopped",
    file: "shader-stopped.png",
    usedBy: "shaders",
    async run(ui) {
      await ui.screen("Sequence");
      await ui.seek(32500);
      await ui.selectEffect({ prop: "Window Matrix", kind: "shader", file: "shaders/Deep Zoom.fs" });
      await ui.waitForText("Stopped at", { sel: "*" });
      return ui.settingsTop("Depth");
    },
  },
];

/** Adds an Art-Net DMX node on the Wiring screen and wires the props named to its universe, in order. */
async function wireToDmxNode(ui, props) {
  await ui.screen("Wiring");
  await ui.click("Add controller");
  await ui.type("Name", "Lawn DMX");
  await ui.choose("Controller type", "DMX node (Art-Net)");
  await ui.type("IP address", "192.168.1.80");
  await ui.type("Universes", "1");
  await ui.click("Add", { exact: true });
  for (const prop of props) {
    await ui.click("Add a prop to port 1 of Lawn DMX");
    await ui.click(prop, { sel: "[role=option], li button, button" });
  }
  await ui.settle(600);
}

/** Adds a DMX flood on the Layout screen, on the lawn, and leaves it selected. */
/** Adds a 16-bit moving head on the lawn on the Layout screen, selected. */
async function addHead(ui, { name = "Lawn Spot", x = -5, y = 0.4, address = 20 } = {}) {
  await ui.screen("Layout");
  await ui.click("Add prop");
  await ui.click("DMX moving head");
  await ui.settle(500);
  await ui.type("Name", name, { enter: true });
  await ui.type("Start address", String(address), { enter: true });
  await ui.type("Position X", String(x), { enter: true });
  await ui.type("Position Y", String(y), { enter: true });
  await ui.settle(400);
}

async function addFlood(ui, { name = "Lawn Flood", x = -9, y = 0.6, address = 1, layout = "7 channels" } = {}) {
  await ui.screen("Layout");
  await ui.click("Add prop");
  await ui.click("DMX flood or bar");
  await ui.settle(500);
  await ui.type("Name", name, { enter: true });
  await ui.choose("Channels", layout);
  await ui.type("Start address", String(address), { enter: true });
  await ui.type("Position X", String(x), { enter: true });
  await ui.type("Position Y", String(y), { enter: true });
  await ui.settle(400);
}

// ---------------------------------------------------------------------------------------------
// Driving the page.

/** Helpers defined in the page before each step. */
const PAGE_HELPERS = String.raw`
var $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
var visible = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden"; };
var CLICKABLE = "button,[role=button],[role=tab],[role=menuitem],[role=menuitemradio],[role=menuitemcheckbox],[role=option],a,label,summary,[role=radio],[role=checkbox],[role=switch]";
var norm = (s) => (s ?? "").replace(/\s+/g, " ").trim();
var nameOf = (el) => norm(el.getAttribute("aria-label") || el.innerText || el.textContent);
var scope = (within) => (within ? (typeof within === "string" ? document.querySelector(within) : within) : document);
var find = (text, { within, exact = false, sel = CLICKABLE } = {}) => {
  const root = scope(within);
  if (!root) return undefined;
  const all = $$(sel, root).filter(visible);
  const texts = (el) => [nameOf(el), norm(el.innerText)];
  return all.find((el) => texts(el).includes(text)) ?? (exact ? undefined : all.find((el) => texts(el).some((t) => t.startsWith(text))));
};
// The smallest element on the screen whose text holds the text given.
var holding = (text, within) => $$("*", scope(within)).filter((el) => visible(el) && norm(el.innerText).toLowerCase().includes(text.toLowerCase())).sort((a, b) => box(a).width * box(a).height - box(b).width * box(b).height)[0];
var box = (el) => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
var field = (label) => {
  const lab = $$("label").filter(visible).find((l) => norm(l.innerText).startsWith(label));
  if (lab) return lab.control ?? lab.querySelector("input,select,textarea") ?? (lab.htmlFor && document.getElementById(lab.htmlFor));
  return $$("input,select,textarea").filter(visible).find((el) => el.getAttribute("aria-label") === label || el.placeholder === label);
};
`;

class Ui {
  constructor(cdp, base) {
    this.cdp = cdp;
    this.base = base;
  }

  /** Runs `code` (the body of an async function) in the page and returns its result. */
  async js(code, arg) {
    const expression = `${PAGE_HELPERS}\n;(async (arg) => { ${code} })(${JSON.stringify(arg ?? null)})`;
    const r = await this.cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  }

  async open(query) {
    const origin = new URL(this.base).origin;
    await this.cdp.send("Storage.clearDataForOrigin", { origin, storageTypes: "local_storage,session_storage,indexeddb,cache_storage" });
    await this.cdp.send("Page.navigate", { url: `${this.base}?${query}` });
    await this.waitFor(() => this.js(`return !!document.querySelector("nav[aria-label=Screens], h1")`), "the app to start");
    await this.settle(1200);
    // The first-run tip over the layout is closed: it would cover the screen in every shot.
    await this.js(`const tip = find("Got it", { exact: true }); if (tip) tip.click(); return 1`);
    await this.settle(200);
  }

  async waitFor(check, what, timeoutMs = 15000) {
    const until = Date.now() + timeoutMs;
    for (;;) {
      if (await check()) return;
      if (Date.now() > until) throw new Error(`Gave up waiting for ${what}.`);
      await sleep(150);
    }
  }

  /** Waits until a button (or link, tab, menu item) with this text is on the screen. */
  async waitForText(text, opts = {}) {
    await this.waitFor(() => this.js(`return !!find(arg.text, arg.opts)`, { text, opts }), `"${text}"`, opts.timeoutMs);
  }

  async waitGone(text, opts = {}) {
    await this.waitFor(() => this.js(`return !find(arg.text, arg.opts)`, { text, opts }), `"${text}" to go`, opts.timeoutMs);
  }

  /** Clicks the button (or link, tab, menu item…) whose label or text is `text`. */
  async click(text, opts = {}) {
    await this.waitForText(text, opts);
    await this.js(
      `const el = find(arg.text, arg.opts); el.scrollIntoView({ block: "nearest" }); el.click(); return 1`,
      { text, opts },
    );
    await this.settle(350);
  }

  /** Goes to a screen from the sidebar. */
  async screen(name) {
    await this.click(name, { exact: true, within: "nav[aria-label=Screens]" });
    await this.settle(800);
  }

  /** Types into the field labelled `label` (or with that placeholder), as a person would. */
  async type(label, text, { enter = false } = {}) {
    const found = await this.js(`const el = field(arg); if (!el) return false; el.scrollIntoView({ block: "nearest" }); el.focus(); el.select?.(); return true`, label);
    if (!found) throw new Error(`No field "${label}".`);
    await this.cdp.send("Input.insertText", { text });
    if (enter) await this.key("Enter");
    await this.settle(300);
  }

  /** Moves the slider (a range input) whose label starts with `label` to `value`. */
  async slide(label, value) {
    const ok = await this.js(
      `const el = $$("input[type=range]").filter(visible).find((i) => {
         const text = norm(i.getAttribute("aria-label") || (i.id && document.querySelector("label[for='" + i.id + "']")?.innerText) || i.closest("label, div")?.innerText);
         return text.replace(/[^a-z0-9 ]/gi, " ").replace(/ +/g, " ").trim().startsWith(arg.label);
       });
       if (!el) return false;
       Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, String(arg.value));
       el.dispatchEvent(new Event("input", { bubbles: true }));
       el.dispatchEvent(new Event("change", { bubbles: true }));
       return true`,
      { label, value },
    );
    if (!ok) throw new Error(`No slider "${label}".`);
    await this.settle(200);
  }

  /** Picks an option of the <select> labelled `label`, by the option's text. */
  async choose(label, option) {
    const ok = await this.js(
      `const el = field(arg.label); if (!el) return "no field";
       const o = [...el.options].find((o) => norm(o.text).startsWith(arg.option)); if (!o) return "no option";
       const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set;
       set.call(el, o.value); el.dispatchEvent(new Event("change", { bubbles: true })); return "ok"`,
      { label, option },
    );
    if (ok !== "ok") throw new Error(`Couldn't choose "${option}" in "${label}": ${ok}.`);
    await this.settle(400);
  }

  async key(key) {
    const codes = { Enter: 13, Escape: 27, Tab: 9 };
    for (const type of ["keyDown", "keyUp"]) {
      await this.cdp.send("Input.dispatchKeyEvent", { type, key, code: key, windowsVirtualKeyCode: codes[key], text: key === "Enter" ? "\r" : undefined });
    }
    await this.settle(250);
  }

  /** The box of the first visible element matching a selector, or of a button (or `sel`) by its text. */
  async box(target, { pad = 8, within, sel } = {}) {
    const b = await this.js(
      `let el; try { el = $$(arg.target, scope(arg.within)).filter(visible)[0]; } catch {}
       el ??= find(arg.target, { within: arg.within, sel: arg.sel }) ?? find(arg.target, { within: arg.within, sel: "*", exact: true }) ?? holding(arg.target, arg.within);
       return el ? box(el) : null`,
      { target, within, sel: sel ?? undefined },
    );
    if (!b) throw new Error(`Nothing on the screen matches "${target}".`);
    return padded(b, pad);
  }

  /** The box of the element `up` levels above the one with this text (a heading, a label…). */
  async parentBoxOf(text, { sel = "*", up = 1, pad = 0 } = {}) {
    const b = await this.js(
      `let el = find(arg.text, { sel: arg.sel, exact: true }) ?? find(arg.text, { sel: arg.sel });
       for (let i = 0; el && i < arg.up; i++) el = el.parentElement;
       return el ? box(el) : null`,
      { text, sel, up },
    );
    if (!b) throw new Error(`Nothing on the screen says "${text}".`);
    return padded(b, pad);
  }

  /** Points at an element, as the mouse would. */
  async hover(text, opts = {}) {
    const b = await this.box(text, { ...opts, pad: 0 });
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: b.x + b.width / 2, y: b.y + b.height / 2 });
    await this.settle(500);
  }

  /** The last card on the screen (the bottom of a screen's column of cards). */
  async lastCard() {
    const b = await this.js(`const cards = $$("main section, main [class*=rounded-xl], main [class*=rounded-lg]").filter(visible).filter((e) => e.getBoundingClientRect().width > 400); const el = cards.at(-1); return el ? box(el) : null`);
    if (!b) throw new Error("No card on the screen.");
    return b;
  }

  /**
   * Puts the mouse in a corner and takes the keyboard focus out of a text box, so no tooltip,
   * caret or focus ring shows. An open menu or popover keeps its focus, and stays open.
   */
  async rest() {
    await this.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: WIDTH - 2, y: HEIGHT - 2 });
    await this.js(`const el = document.activeElement; if (el?.matches("textarea, input:not([type=checkbox]):not([type=radio]):not([type=range])")) el.blur(); return 1`);
    await this.settle(250);
  }

  async blur() {
    await this.js(`document.activeElement?.blur(); return 1`);
    await this.settle(200);
  }

  /**
   * The browser stand-in starts a test without sending anything, so its counts stay at zero.
   * This gives them what the engine reports a while into a test: `frames` sent at `fps`, to the
   * controllers named (each with how many packets it takes a frame).
   */
  async sending({ frames, fps, controllers }) {
    await this.js(
      `const { useApp } = await import("/src/state/store.ts");
       const backend = useApp.getState().backend;
       backend.output = {
         ...backend.output,
         frames: arg.frames,
         achievedFps: arg.fps,
         controllers: backend.output.controllers
           .filter((c) => c.name in arg.controllers)
           .map((c) => ({ ...c, packetsSent: arg.frames * arg.controllers[c.name] })),
       };
       return 1`,
      { frames, fps, controllers },
    );
    await this.settle(2500);
  }

  /** Scrolls the box `container` so the field (or heading) `label` sits in its middle, or at its top. */
  async scrollTo(label, container, where = "middle") {
    await this.js(
      `const box = document.querySelector(arg.container);
       const el = field(arg.label) ?? find(arg.label, { sel: "*", exact: true }) ?? holding(arg.label, arg.container);
       if (!box || !el) throw new Error("Nothing to scroll to: " + arg.label);
       const off = arg.where === "start" ? 12 : box.clientHeight / 2;
       box.scrollTop += el.getBoundingClientRect().top - box.getBoundingClientRect().top - off; return 1`,
      { label, container, where },
    );
    await this.settle(300);
  }

  /** Scrolls every scrolled box matching `selector` back to its top. */
  async scrollToTop(selector) {
    await this.js(`for (const el of $$(arg)) if (el.scrollHeight > el.clientHeight + 4) el.scrollTop = 0; return 1`, selector);
    await this.settle(300);
  }

  /** The top of the effect settings panel, down to the heading or label `until` (left out). */
  async settingsTop(until) {
    const panel = await this.box("[aria-label='Effect settings']", { pad: 0 });
    const end = await this.box(until, { within: "[aria-label='Effect settings']", sel: "h3,h4,label,p,div", pad: 0 });
    return { ...panel, height: end.y - panel.y - 4 };
  }

  /** Makes the row of the prop named `prop` the one being worked on, as clicking its name would. */
  async activeRow(prop) {
    const ok = await this.js(
      `const { useSequencer } = await import("/src/state/sequencer.ts");
       const { useApp } = await import("/src/state/store.ts");
       const p = useApp.getState().snapshot.show.props.find((x) => x.name === arg);
       const row = useSequencer.getState().doc.rows.find((r) => "prop" in r.target && r.target.prop === p?.id);
       if (!row) return false;
       useSequencer.getState().setActiveRow(row.id);
       return true`,
      prop,
    );
    if (!ok) throw new Error(`No row for ${prop}.`);
    await this.settle(300);
  }

  /**
   * Selects an effect on the open sequence's timeline, as clicking it would: the first of `kind`
   * on the row of the prop named `prop`, at or after `fromMs`.
   */
  async selectEffect({ prop, kind, fromMs = 0, file }) {
    const ok = await this.js(
      `const { useSequencer } = await import("/src/state/sequencer.ts");
       const { useApp } = await import("/src/state/store.ts");
       const show = useApp.getState().snapshot.show;
       const p = show.props.find((x) => x.name === arg.prop);
       const s = useSequencer.getState();
       for (const row of s.doc.rows) {
         if (!("prop" in row.target) || row.target.prop !== p?.id) continue;
         for (const layer of row.layers) for (const e of layer.effects) {
           if (e.params.kind === arg.kind && e.startMs >= arg.fromMs && (!arg.file || e.params.file === arg.file)) {
             s.select([e.id], row.id);
             s.reveal?.("selection");
             return true;
           }
         }
       }
       return false`,
      { prop, kind, fromMs, file: file ?? null },
    );
    if (!ok) throw new Error(`No ${kind} effect on ${prop}.`);
    await this.settle(800);
  }

  /** The smallest box around several. */
  async boxes(targets, opts) {
    const all = [];
    for (const t of targets) all.push(await this.box(t, opts));
    const x = Math.min(...all.map((b) => b.x));
    const y = Math.min(...all.map((b) => b.y));
    return { x, y, width: Math.max(...all.map((b) => b.x + b.width)) - x, height: Math.max(...all.map((b) => b.y + b.height)) - y };
  }

  /** The box of the element that holds the first match of `selector`. */
  async parentBox(selector, pad = 0) {
    const b = await this.js(`const el = $$(arg).filter(visible)[0]?.parentElement; return el ? box(el) : null`, selector);
    if (!b) throw new Error(`Nothing on the screen matches "${selector}".`);
    return padded(b, pad);
  }

  window() {
    return { x: 0, y: 0, width: WIDTH, height: HEIGHT };
  }

  async settle(ms) {
    await sleep(ms);
  }

  /** Sets the playhead (the stand-in's sequencer, as the timeline's ruler would). */
  async seek(ms) {
    await this.js(`const { useSequencer } = await import("/src/state/sequencer.ts"); await useSequencer.getState().seek(arg); return 1`, ms);
    await this.settle(700);
  }
}

function padded(b, pad) {
  const x = Math.max(0, b.x - pad);
  const y = Math.max(0, b.y - pad);
  return { x, y, width: Math.min(WIDTH, b.x + b.width + pad) - x, height: Math.min(HEIGHT, b.y + b.height + pad) - y };
}

// ---------------------------------------------------------------------------------------------
// Chrome, over the DevTools protocol.

function findChrome() {
  if (process.env.CHROME) {
    if (existsSync(process.env.CHROME)) return process.env.CHROME;
    fail(`CHROME is set to ${process.env.CHROME}, but there's nothing there.`);
  }
  const candidates = [];
  if (process.platform === "darwin") {
    for (const app of ["Google Chrome", "Google Chrome Canary", "Chromium"]) {
      candidates.push(`/Applications/${app}.app/Contents/MacOS/${app}`, path.join(process.env.HOME ?? "", `Applications/${app}.app/Contents/MacOS/${app}`));
    }
  } else if (process.platform === "win32") {
    for (const base of [process.env.PROGRAMFILES, process.env["PROGRAMFILES(X86)"], process.env.LOCALAPPDATA].filter(Boolean)) {
      candidates.push(path.join(base, "Google/Chrome/Application/chrome.exe"), path.join(base, "Chromium/Application/chrome.exe"));
    }
  } else {
    for (const dir of (process.env.PATH ?? "").split(path.delimiter).filter(Boolean)) {
      for (const name of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) candidates.push(path.join(dir, name));
    }
  }
  const found = candidates.find((c) => existsSync(c));
  if (!found) fail("Couldn't find Google Chrome or Chromium. Install one, or set CHROME to the browser's program.");
  return found;
}

async function startChrome() {
  const profile = mkdtempSync(path.join(tmpdir(), "pixelflow-screenshots-"));
  const chrome = spawn(
    findChrome(),
    ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check", "--hide-scrollbars", "--mute-audio", `--window-size=${WIDTH},${HEIGHT}`, "about:blank"],
    { stdio: "ignore" },
  );
  // Chrome says which port it took in a file in its profile.
  const portFile = path.join(profile, "DevToolsActivePort");
  const until = Date.now() + 20000;
  while (!existsSync(portFile) || !readFileSync(portFile, "utf8").includes("\n")) {
    if (chrome.exitCode !== null || Date.now() > until) {
      stopChrome(chrome, profile);
      fail("Chrome didn't start.");
    }
    await sleep(100);
  }
  const port = Number(readFileSync(portFile, "utf8").split("\n")[0]);
  return { port, stop: () => stopChrome(chrome, profile) };
}

function stopChrome(chrome, profile) {
  chrome.kill();
  try {
    rmSync(profile, { recursive: true, force: true });
  } catch {
    // Chrome may still hold a file for a moment; the folder is in the system's temporary folder.
  }
}

class Cdp {
  static async connect(port) {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const page = targets.find((t) => t.type === "page");
    const cdp = new Cdp(new WebSocket(page.webSocketDebuggerUrl));
    await new Promise((resolve, reject) => {
      cdp.ws.addEventListener("open", resolve, { once: true });
      cdp.ws.addEventListener("error", () => reject(new Error("Couldn't talk to Chrome.")), { once: true });
    });
    return cdp;
  }

  constructor(ws) {
    this.ws = ws;
    this.next = 0;
    this.waiting = new Map();
    ws.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const done = this.waiting.get(message.id);
      if (!done) return;
      this.waiting.delete(message.id);
      if (message.error) done.reject(new Error(`${message.error.message} (${done.method})`));
      else done.resolve(message.result);
    });
  }

  send(method, params = {}) {
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { resolve, reject, method });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    this.ws.close();
  }
}

// ---------------------------------------------------------------------------------------------
// The UI server.

async function startUi(port) {
  const vite = path.join(APP, "node_modules/vite/bin/vite.js");
  if (!existsSync(vite)) fail("The app's dependencies aren't installed: run `make setup` (or `pnpm install` in app/) first.");
  const server = spawn(process.execPath, [vite, "--port", String(port), "--strictPort"], { cwd: APP, stdio: ["ignore", "pipe", "pipe"] });
  let said = "";
  server.stdout.on("data", (d) => (said += d));
  server.stderr.on("data", (d) => (said += d));
  const url = `http://localhost:${port}/`;
  const until = Date.now() + 60000;
  for (;;) {
    if (server.exitCode !== null) fail(`The UI server stopped:\n${said}`);
    if (Date.now() > until) {
      server.kill();
      fail(`The UI server didn't answer at ${url}:\n${said}`);
    }
    try {
      if ((await fetch(url)).ok) break;
    } catch {
      // Not up yet.
    }
    await sleep(250);
  }
  return { url, stop: () => server.kill() };
}

// ---------------------------------------------------------------------------------------------
// Pictures.

function hasFfmpeg() {
  return spawnSync("ffmpeg", ["-version"], { stdio: "ignore" }).status === 0;
}

/** Captures `clip` (CSS pixels) at twice its size, but no wider than MAX_WIDTH. */
async function capture(cdp, clip, file) {
  const x = Math.round(clip.x);
  const y = Math.round(clip.y);
  const width = Math.round(clip.width);
  const height = Math.round(clip.height);
  const scale = Math.min(1, MAX_WIDTH / (width * 2));
  // The app never scrolls as a whole: anything a step scrolled into view is put back first.
  await cdp.send("Runtime.evaluate", { expression: "document.scrollingElement.scrollTo(0, 0)" });
  const { data } = await cdp.send("Page.captureScreenshot", { format: "png", clip: { x, y, width, height, scale }, captureBeyondViewport: false });
  writeFileSync(file, Buffer.from(data, "base64"));
}

/** Reduces a PNG to 256 colours with ffmpeg, when that makes it smaller. */
function reduce(file) {
  const out = `${file}.reduced.png`;
  const r = spawnSync(
    "ffmpeg",
    ["-v", "error", "-y", "-i", file, "-vf", "split[a][b];[a]palettegen=max_colors=256:reserve_transparent=0[p];[b][p]paletteuse=dither=sierra2_4a", out],
    { stdio: "inherit" },
  );
  if (r.status === 0 && statSync(out).size < statSync(file).size) renameSync(out, file);
  else rmSync(out, { force: true });
}

// ---------------------------------------------------------------------------------------------
// The engine's picture: the made-up stage lit up, as an animated GIF.

function engineShot(outDir) {
  if (!hasFfmpeg()) fail("--engine needs ffmpeg on the PATH.");
  const work = mkdtempSync(path.join(tmpdir(), "pixelflow-stage-"));
  try {
    console.log("Drawing the Assistant harness's scripted whole-song draft with the engine (a release build)...");
    const run = spawnSync(
      "cargo",
      ["run", "--release", "--locked", "-p", "pf-ai", "--example", "assistant_eval", "--", "--scenario", "whole-song", "--out", work],
      { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] },
    );
    if (run.status !== 0) fail("The harness didn't draw the stage.");
    const video = path.join(work, "videos", "whole-song.mp4");
    // From the end of the first verse, through the build, into the first chorus.
    const gif = path.join(outDir, "show-lit-up.gif");
    const filter = "fps=10,crop=1280:376:0:36,scale=880:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle";
    const r = spawnSync("ffmpeg", ["-v", "error", "-y", "-ss", "21", "-t", "7.5", "-i", video, "-vf", filter, "-loop", "0", gif], { stdio: "inherit" });
    if (r.status !== 0) fail("ffmpeg couldn't make the GIF.");
    console.log(`  show-lit-up.gif  ${Math.round(statSync(gif).size / 1024)} KB`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------------------------

/** A reason to stop, said plainly (anything else is a bug here, and keeps its stack). */
class Failure extends Error {}

function fail(message) {
  throw new Failure(message);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function option(name) {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : undefined;
}

async function main() {
  if (process.argv.includes("--list")) {
    for (const s of SHOTS) console.log(`${s.name.padEnd(22)} ${s.file.padEnd(28)} ${s.usedBy}`);
    console.log(`${"(--engine)".padEnd(22)} ${"show-lit-up.gif".padEnd(28)} README`);
    return;
  }
  if (typeof WebSocket === "undefined") fail("This needs Node 22 or newer (for its built-in WebSocket).");
  const outDir = path.resolve(ROOT, option("out") ?? "docs/images");
  mkdirSync(outDir, { recursive: true });
  const only = option("only")?.split(",");
  const shots = only ? SHOTS.filter((s) => only.includes(s.name)) : SHOTS;
  if (only && shots.length !== only.length) fail(`No such screenshot: ${only.filter((n) => !SHOTS.some((s) => s.name === n)).join(", ")}. See --list.`);

  const debugPort = option("chrome-port");
  if (!debugPort) findChrome();
  const reducing = hasFfmpeg();
  if (!reducing) console.log("ffmpeg isn't on the PATH: the PNGs are written without reducing them.");
  let ui;
  let chrome;
  let cdp;
  try {
    ui = option("url") ? { url: option("url"), stop() {} } : await startUi(Number(option("port") ?? 5199));
    chrome = debugPort ? { port: Number(debugPort), stop() {} } : await startChrome();
    cdp = await Cdp.connect(chrome.port);
    await cdp.send("Page.enable");
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: WIDTH, height: HEIGHT, deviceScaleFactor: 2, mobile: false });
    // The app follows the computer's light or dark setting by default: these show it light.
    await cdp.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "light" }, { name: "prefers-reduced-motion", value: "reduce" }] });
    const page = new Ui(cdp, ui.url);
    for (const shot of shots) {
      let clip;
      try {
        await page.open(shot.query ?? "demo");
        clip = await shot.run(page);
        if (!shot.pointing) await page.rest();
      } catch (error) {
        // Most often the screen has changed: a button renamed or moved.
        fail(`${shot.name}: ${error.message.split("\n")[0]}`);
      }
      const file = path.join(outDir, shot.file);
      await capture(cdp, clip, file);
      if (reducing) reduce(file);
      console.log(`  ${shot.file.padEnd(28)} ${Math.round(statSync(file).size / 1024)} KB`);
    }
  } finally {
    cdp?.close();
    chrome?.stop();
    ui?.stop();
  }
  if (process.argv.includes("--engine")) engineShot(outDir);
}

await main().catch((error) => {
  console.error(error instanceof Failure ? `screenshots: ${error.message}` : error);
  process.exitCode = 1;
});
