import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../../App";
import { demoDevices, demoPlayers } from "../../api/demo";
import { MemoryBackend } from "../../api/memory";
import type { Prop } from "../../api/types";
import { newProp, nodeCount } from "../../lib/shows";
import { useApp } from "../../state/store";

const WLED = "192.0.2.40";

async function openDevices() {
  const backend = new MemoryBackend();
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<App />);
  await user.click(screen.getByRole("button", { name: /^new show/i }));
  await user.click(screen.getByRole("button", { name: "Controllers" }));
  await user.click(screen.getByRole("button", { name: "Scan network" }));
  return { backend, user };
}

/** Scans and adds the demo WLED ("Porch Strip", 50 pixels on output 1) to the show. */
async function withWled() {
  const app = await openDevices();
  await app.user.click(await screen.findByRole("button", { name: "Open Porch WLED" }));
  await app.user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Add to show" }));
  return app;
}

const wledConfig = (backend: MemoryBackend) => backend.deviceNetwork.details.find((d) => d.device.address === WLED)!.config;
const show = () => useApp.getState().snapshot!.show;
const prop = (name: string) => show().props.find((p) => p.name === name)!;

async function addProp(name: string, nodes: number): Promise<Prop> {
  const p = { ...newProp("line", show()), name, shape: { source: "generator" as const, type: "line" as const, nodes, length: 5 } };
  await useApp.getState().run((b) => b.applyEdits([{ type: "addProp", prop: p }]));
  return p;
}

async function resize(name: string, nodes: number) {
  const p = structuredClone(prop(name));
  if (p.shape.source === "generator" && p.shape.type === "line") p.shape.nodes = nodes;
  await useApp.getState().run((b) => b.applyEdits([{ type: "updateProp", prop: p }]));
}

describe("compare with this device", () => {
  it("lists differences by port and takes the picked ones into the show as one undo step", async () => {
    const { user, backend } = await withWled();
    // On the WLED's own page: output 1 grows to 60 pixels and becomes BGR.
    wledConfig(backend).ports[0].strings[0].pixels = 60;
    wledConfig(backend).ports[0].strings[0].colorOrder = "BGR";

    await user.click(screen.getByRole("button", { name: "Compare with this device: Porch WLED" }));
    const dialog = await screen.findByRole("dialog", { name: "Compare with Porch WLED" });
    const port = await within(dialog).findByRole("region", { name: "Port 1" });
    const pixels = within(port).getByRole("checkbox", { name: "Take String 1 · Porch Strip Pixels: 50 to 60" });
    const order = within(port).getByRole("checkbox", { name: "Take String 1 · Porch Strip Color order: GRB to BGR" });
    expect(pixels).toBeChecked();
    expect(order).toBeChecked();
    await user.click(order);
    expect(backend.calls.some((c) => c.startsWith("takeFromDevice"))).toBe(false);

    await user.click(within(dialog).getByRole("button", { name: "Take 1 change into my show" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(nodeCount(prop("Porch Strip").shape)).toBe(60);
    expect(show().controllers[0].ports[0].slots[0].controllerColorOrder).toBe("GRB");
    expect(screen.getByText("Took 1 change from Porch WLED into your show.")).toBeInTheDocument();
    // Nothing was sent to the WLED.
    expect(backend.calls.filter((c) => c.startsWith("sendDeviceSetup"))).toEqual([]);

    await user.click(within(screen.getByText("Took 1 change from Porch WLED into your show.").parentElement!).getByRole("button", { name: "Undo" }));
    expect(nodeCount(prop("Porch Strip").shape)).toBe(50);
  });

  it("wires a new string to a prop already in the show instead of a new one", async () => {
    const { user, backend } = await withWled();
    const arch = await addProp("Garage Arch", 50);
    wledConfig(backend).ports.push({ number: 2, strings: [{ ...wledConfig(backend).ports[0].strings[0], name: null, pixels: 40 }], maxPixels: null });

    await user.click(screen.getByRole("button", { name: "Compare with this device: Porch WLED" }));
    const dialog = await screen.findByRole("dialog", { name: "Compare with Porch WLED" });
    const port = await within(dialog).findByRole("region", { name: "Port 2" });
    expect(within(port).getByText("40 pixels, GRB")).toBeInTheDocument();
    await user.selectOptions(within(port).getByRole("combobox", { name: "Prop for String 1 on port 2" }), arch.id);
    expect(within(port).getByText("Garage Arch has 50 pixels; this string has 40.")).toBeInTheDocument();
    // A prop that reorders its colors on a string the controller reorders too is warned about.
    const old = await addProp("Old Arch", 40);
    await useApp.getState().run((b) => b.applyEdits([{ type: "updateProp", prop: { ...prop("Old Arch"), colorOrder: "BGR" } }]));
    await user.selectOptions(within(port).getByRole("combobox", { name: "Prop for String 1 on port 2" }), old.id);
    expect(within(port).getByText(/colors are swapped twice/)).toBeInTheDocument();
    // One with another channel width can't be wired at all.
    const white = await addProp("White Arch", 40);
    await useApp.getState().run((b) => b.applyEdits([{ type: "updateProp", prop: { ...prop("White Arch"), colorOrder: "RGBW" } }]));
    await user.selectOptions(within(port).getByRole("combobox", { name: "Prop for String 1 on port 2" }), white.id);
    expect(within(port).getByRole("alert")).toHaveTextContent("White Arch can't be wired here");
    await user.selectOptions(within(port).getByRole("combobox", { name: "Prop for String 1 on port 2" }), arch.id);
    await user.click(within(dialog).getByRole("button", { name: "Take 1 change into my show" }));

    const wled = show().controllers[0];
    expect(wled.ports.map((p) => p.number)).toEqual([1, 2]);
    expect(wled.ports[1].slots[0].prop).toBe(arch.id);
    expect(show().props.map((p) => p.name)).toEqual(["Porch Strip", "Garage Arch", "Old Arch", "White Arch"]);
  });

  it("says when the show and the controller match", async () => {
    const { user } = await withWled();
    await user.click(screen.getByRole("button", { name: "Compare with this device: Porch WLED" }));
    expect(await screen.findByText("Your show and Porch WLED match.")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("send setup to this device", () => {
  it("shows before → after with warnings, and sends only on the Send click", async () => {
    const { user, backend } = await withWled();
    await resize("Porch Strip", 40);

    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    const dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    const port = await within(dialog).findByRole("region", { name: "Port 1" });
    expect(within(port).getByText("Pixels")).toBeInTheDocument();
    expect(within(port).getByText("50")).toBeInTheDocument();
    expect(within(port).getByText("40")).toBeInTheDocument();
    expect(within(port).getByText("10 pixels fewer: the last 10 on this string go dark.")).toBeInTheDocument();
    expect(within(dialog).getByText(/1 of them turns pixels off or moves them/)).toBeInTheDocument();
    expect(backend.calls.filter((c) => c.startsWith("sendDeviceSetup"))).toEqual([]);

    await user.click(within(dialog).getByRole("button", { name: "Send to Porch WLED" }));
    expect(await within(dialog).findByText("Sent. Reading it back, the controller matches your show.")).toBeInTheDocument();
    expect(backend.calls).toContain("sendDeviceSetup:192.0.2.40:port1/string1/pixels");
    expect(wledConfig(backend).ports[0].strings[0].pixels).toBe(40);
    // The copy from before sending stays, in case the lights look wrong.
    expect(within(dialog).getByRole("button", { name: "Put back the previous setup…" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps the copy for Put back when reopened, shows what Put back changes before it writes, and lets a failed one be tried again", async () => {
    const { user, backend } = await withWled();
    await resize("Porch Strip", 40);
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    let dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    await user.click(await within(dialog).findByRole("button", { name: "Send to Porch WLED" }));
    await within(dialog).findByText("Sent. Reading it back, the controller matches your show.");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));

    // Reopened: the copy from before the send is still offered.
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    expect(await within(dialog).findByText(/A copy of Porch WLED's setup from .*, before PixelFlow changed it, is kept\./)).toBeInTheDocument();

    // Put back shows its rows first; nothing is written until it's confirmed.
    await user.click(within(dialog).getByRole("button", { name: "Put back the previous setup…" }));
    const rows = await within(dialog).findByRole("group", { name: "What Put back changes" });
    expect(within(rows).getByText("40")).toBeInTheDocument();
    expect(within(rows).getByText("50")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.startsWith("restoreDeviceSetup"))).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Don't put it back" }));
    expect(wledConfig(backend).ports[0].strings[0].pixels).toBe(40);

    backend.restoreFailure = true;
    await user.click(within(dialog).getByRole("button", { name: "Put back the previous setup…" }));
    await user.click(await within(dialog).findByRole("button", { name: "Put back on Porch WLED" }));
    expect(await within(dialog).findByText(/Putting the previous setup back failed/)).toHaveTextContent("You can try again.");
    expect(wledConfig(backend).ports[0].strings[0].pixels).toBe(40);
    backend.restoreFailure = false;
    await user.click(within(dialog).getByRole("button", { name: "Put back the previous setup…" }));
    await user.click(await within(dialog).findByRole("button", { name: "Put back on Porch WLED" }));
    expect(await within(dialog).findByText("The previous setup is back on the controller.")).toBeInTheDocument();
    expect(wledConfig(backend).ports[0].strings[0].pixels).toBe(50);
    // Once it's back, the copy is let go.
    expect(within(dialog).queryByRole("button", { name: /Put back/ })).not.toBeInTheDocument();
  });

  it("keeps the oldest copy across later sends, and forgets it on request", async () => {
    const { user, backend } = await withWled();
    for (const pixels of [40, 30]) {
      await resize("Porch Strip", pixels);
      await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
      const dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
      await user.click(await within(dialog).findByRole("button", { name: "Send to Porch WLED" }));
      await within(dialog).findByRole("status");
      await user.click(within(dialog).getByRole("button", { name: "Close" }));
    }
    expect(backend.setupCopies.get("wled-Porch WLED")!.config.ports[0].strings[0].pixels).toBe(50);
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    const dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    await user.click(await within(dialog).findByRole("button", { name: "Forget this copy" }));
    expect(within(dialog).queryByRole("button", { name: /Put back/ })).not.toBeInTheDocument();
    expect(backend.calls).toContain("forgetDeviceSetupCopy:wled-Porch WLED");
  });

  it("won't put a copy back on another controller that now answers at the address", async () => {
    const { user, backend } = await withWled();
    await resize("Porch Strip", 40);
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    const dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    await user.click(await within(dialog).findByRole("button", { name: "Send to Porch WLED" }));
    await within(dialog).findByRole("status");
    await user.click(within(dialog).getByRole("button", { name: "Put back the previous setup…" }));
    await within(dialog).findByRole("group", { name: "What Put back changes" });
    // Another WLED takes the address before Put back is confirmed.
    backend.deviceNetwork.details.find((d) => d.device.address === WLED)!.device.name = "Garage WLED";
    await user.click(within(dialog).getByRole("button", { name: "Put back on Porch WLED" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("is now Garage WLED");
    expect(wledConfig(backend).ports[0].strings[0].pixels).toBe(40);
  });

  it("won't send a setup the controller wouldn't load, and says why", async () => {
    const { user, backend } = await withWled();
    // Pretend the WLED is an FPP, whose strings stop at 1,600 pixels.
    backend.deviceNetwork.details.find((d) => d.device.address === WLED)!.device.kind = "fpp";
    await resize("Porch Strip", 2000);
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    const dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("an FPP string drives at most 1,600 pixels, and this one would have 2,000");
    expect(within(dialog).queryByRole("button", { name: /^Send to/ })).not.toBeInTheDocument();
  });

  it("offers to put the previous setup back when sending fails partway", async () => {
    const { user, backend } = await withWled();
    await resize("Porch Strip", 30);
    backend.setupSendFailure = "fail";
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    const dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    await user.click(await within(dialog).findByRole("button", { name: "Send to Porch WLED" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("It may have been only partly saved.");
    expect(wledConfig(backend).ports[0].strings[0].pixels).toBe(30);

    await user.click(within(dialog).getByRole("button", { name: "Put back the previous setup…" }));
    await user.click(await within(dialog).findByRole("button", { name: "Put back on Porch WLED" }));
    expect(await within(dialog).findByText("The previous setup is back on the controller.")).toBeInTheDocument();
    expect(wledConfig(backend).ports[0].strings[0].pixels).toBe(50);
    expect(within(dialog).queryByRole("button", { name: /Put back/ })).not.toBeInTheDocument();
  });

  it("reports a controller that doesn't match after sending", async () => {
    const { user, backend } = await withWled();
    await resize("Porch Strip", 30);
    backend.setupSendFailure = "mismatch";
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Porch WLED…" }));
    const dialog = await screen.findByRole("dialog", { name: "Send setup to Porch WLED" });
    await user.click(await within(dialog).findByRole("button", { name: "Send to Porch WLED" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("doesn't match your show");
    expect(within(dialog).getByRole("group", { name: "Still different after sending" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Put back the previous setup…" })).toBeInTheDocument();
  });

  it("can't send to a Falcon yet, and says so", async () => {
    const { user } = await openDevices();
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_B9F5" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Add to show" }));
    await user.click(screen.getByRole("button", { name: "Send setup to this device: Falcon_F16V5_B9F5…" }));
    const dialog = await screen.findByRole("dialog", { name: "Send setup to Falcon_F16V5_B9F5" });
    expect(await within(dialog).findByText(/can't send a setup to Falcon controllers yet/)).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /^Send to/ })).not.toBeInTheDocument();
  });
});

describe("import review", () => {
  it("wires a device's strings to props already in the show", async () => {
    const { user } = await openDevices();
    const arch = await addProp("Garage Arch", 50);
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_B9F5" }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(/one new prop per string/)).toBeInTheDocument();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Prop for port 2 Falcon Arch" }), arch.id);
    expect(within(dialog).getByText(/wire 1 prop you already have, and add 1 new prop/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Add to show" }));

    expect(screen.getByRole("status")).toHaveTextContent("Added Falcon_F16V5_B9F5: 1 props and 1 of yours on 2 ports.");
    const falcon = show().controllers[0];
    expect(falcon.ports[1].slots[0].prop).toBe(arch.id);
    expect(show().props.map((p) => p.name).sort()).toEqual(["Falcon Mega Tree", "Garage Arch"]);
  });
});
