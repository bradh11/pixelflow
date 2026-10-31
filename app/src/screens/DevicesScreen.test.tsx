import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoDevices, demoPlayers } from "../api/demo";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Prop } from "../api/types";
import { newProp } from "../lib/shows";
import { useApp } from "../state/store";

async function startApp() {
  const backend = new MemoryBackend();
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<App />);
  return { backend, user };
}

async function openDevices() {
  const app = await startApp();
  await app.user.click(screen.getByRole("button", { name: /^new show/i }));
  await app.user.click(screen.getByRole("button", { name: "Controllers" }));
  return app;
}

describe("devices", () => {
  it("find controllers on the welcome screen scans right away", async () => {
    const { user, backend } = await startApp();
    await user.click(screen.getByRole("button", { name: /^find controllers/i }));
    expect(await screen.findByText("Falcon_F16V5_A1B2")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Controllers" })).toBeInTheDocument();
    expect(backend.calls).toContain("discoverDevices::network");
  });

  it("lists controllers with how they were found, and warns about silent ones", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    const falconRow = (await screen.findByText("Falcon_F16V5_A1B2")).closest("tr")!;
    expect(within(falconRow).getByText("Falcon")).toBeInTheDocument();
    expect(within(falconRow).getByText("listed by an FPP")).toBeInTheDocument();
    expect(within(falconRow).getByText("192.0.2.20")).toBeInTheDocument();
    expect(screen.getByText(/isn't responding/)).toHaveTextContent("Falcon_F16V5_Garage (192.0.2.21) isn't responding");
    expect(screen.getByText(/isn't responding/)).toHaveTextContent("lists it — check that it's powered on");
    expect(screen.getByRole("button", { name: "Find controllers again" })).toBeInTheDocument();
  });

  it("says which controllers asked for a password instead of leaving them out", async () => {
    const { user, backend } = await openDevices();
    backend.deviceNetwork = { details: [], silent: [], locked: ["192.0.2.50"] };
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    expect(await screen.findByText(/asks for a password/)).toHaveTextContent(
      "192.0.2.50 asks for a password, so PixelFlow can't read it.",
    );
    expect(screen.queryByText("No controllers found")).not.toBeInTheDocument();
  });

  it("reviews and imports a controller as one undo step", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_A1B2" });
    expect(within(dialog).getByText("Falcon Mega Tree")).toBeInTheDocument();
    expect(within(dialog).getByText("Reversed")).toBeInTheDocument();
    expect(within(dialog).getByText(/controller applies its own settings/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Receives:/).closest("p")).toHaveTextContent("Receives: DDP");

    await user.click(within(dialog).getByRole("button", { name: "Add to show" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Added Falcon_F16V5_A1B2: 2 props on 2 ports.");
    expect(screen.getByText("In show")).toBeInTheDocument();
    expect(screen.getByText(/2 props · 850 pixels · 1 controller/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(screen.queryByText("In show")).not.toBeInTheDocument();
  });

  it("says what was actually imported when the controller changed since it was reviewed", async () => {
    const { user, backend } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_A1B2" });
    // Someone removes port 2 on the controller while the review is open.
    const falcon = backend.deviceNetwork.details.find((d) => d.device.address === "192.0.2.20")!;
    falcon.config.ports = falcon.config.ports.slice(0, 1);
    falcon.plan.controller.ports = falcon.plan.controller.ports.slice(0, 1);
    falcon.plan.props = falcon.plan.props.slice(0, 1);

    await user.click(within(dialog).getByRole("button", { name: "Add to show" }));
    expect(screen.getByRole("status")).toHaveTextContent("Added Falcon_F16V5_A1B2: 1 prop on 1 port.");
  });

  it("opens an FPP's own page, and goes back to the list", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByText("Pi 3 Model B+ · FPP 9.3 · player"));
    expect(await screen.findByRole("heading", { name: "FPP" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(await screen.findByRole("region", { name: "Now playing" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back to controllers" }));
    expect(screen.getByRole("heading", { name: "Controllers" })).toBeInTheDocument();
    expect(screen.getByText("Porch WLED")).toBeInTheDocument();
  });

  it("sets up the show from an FPP, then fills its controller in when that controller is imported", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open FPP" }));
    await user.click(await screen.findByRole("button", { name: "Set up my show from this FPP" }));
    await user.click(await screen.findByRole("button", { name: "Add 1 controller" }));
    expect(await screen.findByText("Your show has every controller this FPP sends to.")).toBeInTheDocument();
    expect(useApp.getState().snapshot!.show.controllers.map((c) => [c.name, c.ports.length])).toEqual([["Falcon_F16V5_A1B2", 0]]);
    await user.click(screen.getByRole("button", { name: "Back to controllers" }));

    await user.click(screen.getByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    const review = await screen.findByRole("dialog");
    expect(await within(review).findByText("Fills in Falcon_F16V5_A1B2, added from your FPP's output list.")).toBeInTheDocument();
    expect(within(review).queryByText(/already in your show/)).not.toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Add to show" }));
    const controllers = useApp.getState().snapshot!.show.controllers;
    expect(controllers).toHaveLength(1);
    expect(controllers[0].ports.length).toBeGreaterThan(0);
  });

  it("clicking a device's row opens it", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByText("Porch WLED"));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Import Porch WLED" })).toBeInTheDocument();
  });

  it("remembers found controllers, refreshes them on the next scan, and marks ones that don't answer", async () => {
    const { user, backend } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    expect(await screen.findByText("Porch WLED")).toBeInTheDocument();

    // The WLED goes offline; the next scan checks every remembered controller directly.
    backend.deviceNetwork.details = backend.deviceNetwork.details.filter((d) => d.device.kind !== "wled");
    await user.click(screen.getByRole("button", { name: "Find controllers again" }));
    expect(backend.calls).toContain("discoverDevices:192.0.2.10,192.0.2.20,192.0.2.40:network");
    const row = (await screen.findByText("Porch WLED")).closest("tr")!;
    expect(within(row).getByText(/Not responding · last seen just now/)).toBeInTheDocument();

    // Still there after the app restarts.
    useApp.setState({ discovery: null });
    await useApp.getState().connect(backend);
    expect(useApp.getState().discovery!.devices.map((d) => [d.name, d.responding])).toEqual([
      ["FPP", true],
      ["Falcon_F16V5_A1B2", true],
      ["Porch WLED", false],
    ]);

    await user.click(within(screen.getByText("Porch WLED").closest("tr")!).getByRole("button", { name: "Forget Porch WLED" }));
    expect(screen.queryByText("Porch WLED")).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("checks a typed address and says when nothing answers", async () => {
    const { user, backend } = await openDevices();
    await user.type(screen.getByLabelText("Controller address"), "10.9.9.9");
    await user.click(screen.getByRole("button", { name: /check address/i }));
    expect(backend.calls).toContain("discoverDevices:10.9.9.9");
    expect(await screen.findByRole("status")).toHaveTextContent("No controller answered at 10.9.9.9.");
  });

  it("checks a typed address and says what answered, and what to do next", async () => {
    const { user } = await openDevices();
    await user.type(screen.getByLabelText("Controller address"), "192.0.2.20");
    await user.click(screen.getByRole("button", { name: /check address/i }));
    expect(await screen.findByRole("status")).toHaveTextContent("Found Falcon_F16V5_A1B2 at 192.0.2.20. Choose Open on its row to add it to your show.");
    expect(screen.getByLabelText("Controller address")).toHaveValue("");
  });

  it("offers Compare first for a controller that's already in the show, and adding a copy second", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open Porch WLED" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Add to show" }));
    await user.click(screen.getByRole("button", { name: "Open Porch WLED" }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(/already in your show/)).toBeInTheDocument();
    // Its string starts on the prop the show already has on that port.
    expect(within(dialog).getByRole("combobox", { name: "Prop for port 1 Porch Strip" })).toHaveDisplayValue(/^Porch Strip · /);
    expect(within(dialog).getByText("already on this port")).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Add to show" })).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Compare with this controller" })).toHaveClass("bg-accent-600");
    expect(within(dialog).getByRole("button", { name: "Add another copy" })).not.toHaveClass("bg-accent-600");

    await user.click(within(dialog).getByRole("button", { name: "Compare with this controller" }));
    expect(await screen.findByRole("dialog", { name: "Compare with Porch WLED" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Import Porch WLED" })).not.toBeInTheDocument();
  });

  it("starts each string on the prop of the same name, says why, and can be changed", async () => {
    const { user } = await openDevices();
    const line = (name: string, nodes: number): Prop => ({
      ...newProp("line", emptyShow("t")),
      name,
      shape: { source: "generator", type: "line", nodes, length: nodes * 0.05 },
    });
    await useApp.getState().run((b) =>
      b.applyEdits([
        { type: "addProp", prop: line("falcon  mega tree", 800) },
        { type: "addProp", prop: line("Falcon Arch", 60) },
      ]),
    );
    await user.click(screen.getByRole("button", { name: "Find controllers" }));

    await user.click(await screen.findByRole("button", { name: "Open Porch WLED" }));
    let dialog = await screen.findByRole("dialog", { name: "Import Porch WLED" });
    expect(await within(dialog).findByRole("combobox", { name: "Prop for port 1 Porch Strip" })).toHaveDisplayValue("A new prop");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await user.click(screen.getByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_A1B2" });
    const tree = await within(dialog).findByRole("combobox", { name: "Prop for port 1 Falcon Mega Tree" });
    const arch = within(dialog).getByRole("combobox", { name: "Prop for port 2 Falcon Arch" });
    expect(tree).toHaveDisplayValue("falcon  mega tree · 800 px");
    expect(arch).toHaveDisplayValue("Falcon Arch · 60 px");
    expect(within(dialog).getAllByText("same name")).toHaveLength(2);
    expect(within(dialog).getByText("pixel count differs (50 vs 60)")).toBeInTheDocument();

    await user.selectOptions(arch, "A new prop");
    expect(within(dialog).queryByText("pixel count differs (50 vs 60)")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Add to show" }));
    expect(screen.getByRole("status")).toHaveTextContent("Added Falcon_F16V5_A1B2: 1 prop and 1 of yours on 2 ports.");
  });

  it("starts a string no name matches on the one unwired prop that fits it, says why, and undoes as one step", async () => {
    const { user } = await openDevices();
    // Drawn before the Falcon was added: an 800-pixel mega tree, as the Falcon's tree string.
    await useApp.getState().run((b) => b.applyEdits([{ type: "addProp", prop: { ...newProp("tree", emptyShow("t")), name: "Mega Tree 1" } }]));
    const before = useApp.getState().snapshot!.show.props.length;
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_A1B2" });
    const tree = await within(dialog).findByRole("combobox", { name: "Prop for port 1 Falcon Mega Tree" });
    expect(tree).toHaveDisplayValue("Mega Tree 1 · 800 px");
    expect(within(dialog).getByText("unwired tree, 800 pixels")).toBeInTheDocument();
    // The arch has nothing that fits: a new prop, and the tree can't be picked for it as well.
    const arch = within(dialog).getByRole("combobox", { name: "Prop for port 2 Falcon Arch" });
    expect(arch).toHaveDisplayValue("A new prop");
    expect(within(arch).getByRole("option", { name: /^Mega Tree 1 · 800 px \(picked for port 1 Falcon Mega Tree\)$/ })).toBeDisabled();
    expect(within(dialog).getByText(/wire 1 prop you already have, and add 1 new prop\./)).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Add to show" }));
    expect(screen.getByRole("status")).toHaveTextContent("Added Falcon_F16V5_A1B2: 1 prop and 1 of yours on 2 ports.");
    const show = useApp.getState().snapshot!.show;
    expect(show.props.filter((p) => p.name.toLowerCase().includes("tree")).map((p) => p.name)).toEqual(["Mega Tree 1"]);
    expect(show.controllers[0].ports[0].slots[0].prop).toBe(show.props.find((p) => p.name === "Mega Tree 1")!.id);

    await useApp.getState().undo();
    expect(useApp.getState().snapshot!.show.controllers).toEqual([]);
    expect(useApp.getState().snapshot!.show.props).toHaveLength(before);
  });

  it("lists every unwired prop that fits first when there are several, picks none, and frees a prop given up", async () => {
    const { user } = await openDevices();
    const tree = (name: string) => ({ ...newProp("tree", emptyShow("t")), name });
    await useApp.getState().run((b) =>
      b.applyEdits([
        { type: "addProp", prop: tree("Mega Tree 2") },
        { type: "addProp", prop: tree("Mega Tree 1") },
        { type: "addProp", prop: { ...newProp("arch", emptyShow("t")), name: "Garage Arch" } },
      ]),
    );
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_A1B2" });
    const picker = await within(dialog).findByRole("combobox", { name: "Prop for port 1 Falcon Mega Tree" });
    expect(picker).toHaveDisplayValue("A new prop");
    expect(within(dialog).getByText("2 unwired props have 800 pixels: pick one if it's this string")).toBeInTheDocument();
    const fitting = within(picker).getByRole("group", { name: "Unwired, 800 pixels" });
    expect(within(fitting).getAllByRole("option").map((o) => o.textContent)).toEqual(["Mega Tree 1 · 800 px", "Mega Tree 2 · 800 px"]);

    await user.selectOptions(picker, within(fitting).getByRole("option", { name: "Mega Tree 2 · 800 px" }));
    expect(within(dialog).getByText("unwired tree, 800 pixels")).toBeInTheDocument();
    // The arch's one fit was picked for it; giving it up lets the tree's string have it.
    const arch = within(dialog).getByRole("combobox", { name: "Prop for port 2 Falcon Arch" });
    expect(arch).toHaveDisplayValue("Garage Arch · 50 px");
    expect(within(picker).getByRole("option", { name: /^Garage Arch · 50 px \(picked for port 2 Falcon Arch\)$/ })).toBeDisabled();
    await user.selectOptions(arch, "A new prop");
    expect(within(picker).getByRole("option", { name: "Garage Arch · 50 px" })).toBeEnabled();
    expect(within(arch).getByRole("option", { name: /^Mega Tree 2 · 800 px \(picked for port 1 Falcon Mega Tree\)$/ })).toBeDisabled();
  });

  it("says plainly when every string goes to a prop already in the show", async () => {
    const { user } = await openDevices();
    const line = (name: string, nodes: number): Prop => ({
      ...newProp("line", emptyShow("t")),
      name,
      shape: { source: "generator", type: "line", nodes, length: nodes * 0.05 },
    });
    await useApp.getState().run((b) =>
      b.applyEdits([
        { type: "addProp", prop: line("Falcon Mega Tree", 800) },
        { type: "addProp", prop: line("Falcon Arch", 50) },
      ]),
    );
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_A1B2" });
    expect(await within(dialog).findByText(/PixelFlow will add the controller with 2 ports and wire 2 props you already have\./)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Add to show" }));
    expect(screen.getByRole("status")).toHaveTextContent("Added Falcon_F16V5_A1B2: 2 props of yours on 2 ports.");
  });

  it("spaces the import table's columns, with numbers on the right", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    await user.click(await screen.findByRole("button", { name: "Open Falcon_F16V5_A1B2" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_A1B2" });
    const headers = within(within(dialog).getByRole("table")).getAllByRole("columnheader");
    expect(headers.map((h) => h.textContent)).toEqual(["Port", "String", "Pixels", "Order", "Nulls", "Direction", "Brightness", "Gamma"]);
    for (const header of headers) expect(header).toHaveClass("px-2");
    const right = headers.filter((h) => h.classList.contains("text-right")).map((h) => h.textContent);
    expect(right).toEqual(["Port", "Pixels", "Nulls", "Brightness", "Gamma"]);
  });

  it("empty, it scans from its own button, the only one on the screen", async () => {
    const { user, backend } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Find controllers" }));
    expect(await screen.findByText("Falcon_F16V5_A1B2")).toBeInTheDocument();
    expect(backend.calls).toContain("discoverDevices::network");
  });

  it("empty, a button goes to the Wiring screen to add a controller by hand", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: /add a controller by hand/ }));
    expect(useApp.getState().screen).toBe("wiring");
  });
});
