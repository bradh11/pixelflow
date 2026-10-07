import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoDevices, demoPlayers } from "../api/demo";
import { MemoryBackend } from "../api/memory";
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
  await app.user.click(screen.getByRole("button", { name: "Devices" }));
  return app;
}

describe("devices", () => {
  it("discover my devices from the welcome screen scans right away", async () => {
    const { user, backend } = await startApp();
    await user.click(screen.getByRole("button", { name: /discover my devices/i }));
    expect(await screen.findByText("Falcon_F16V5_B9F5")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Devices" })).toBeInTheDocument();
    expect(backend.calls).toContain("discoverDevices::network");
  });

  it("lists controllers with how they were found, and warns about silent ones", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    const falconRow = (await screen.findByText("Falcon_F16V5_B9F5")).closest("tr")!;
    expect(within(falconRow).getByText("Falcon")).toBeInTheDocument();
    expect(within(falconRow).getByText("listed by an FPP")).toBeInTheDocument();
    expect(within(falconRow).getByText("192.0.2.20")).toBeInTheDocument();
    expect(screen.getByText(/isn't responding/)).toHaveTextContent("Falcon_F16V5_Garage (192.0.2.21) isn't responding");
    expect(screen.getByText(/isn't responding/)).toHaveTextContent("lists it — check that it's powered on");
    expect(screen.getByRole("button", { name: "Scan again" })).toBeInTheDocument();
  });

  it("reviews and imports a controller as one undo step", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByRole("button", { name: "Review Falcon_F16V5_B9F5" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_B9F5" });
    expect(within(dialog).getByText("Falcon Mega Tree")).toBeInTheDocument();
    expect(within(dialog).getByText("Reversed")).toBeInTheDocument();
    expect(within(dialog).getByText(/controller applies its own settings/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Receives:/).closest("p")).toHaveTextContent("Receives: DDP");

    await user.click(within(dialog).getByRole("button", { name: "Add to show" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Added Falcon_F16V5_B9F5: 2 props on 2 ports.");
    expect(screen.getByText("In show")).toBeInTheDocument();
    expect(screen.getByText(/2 props · 850 pixels · 1 controller/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(screen.queryByText("In show")).not.toBeInTheDocument();
  });

  it("explains when a device has nothing to import", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByRole("button", { name: "Review FPP" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Sends 6,147 channels by DDP to Falcon_F16V5_B9F5/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Add them to your show from here/)).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /import|add to show/i })).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Close" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows what an FPP is playing and lets you stop and start it", async () => {
    const { user, backend } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByRole("button", { name: "Review FPP" }));
    const dialog = await screen.findByRole("dialog");
    const player = await within(dialog).findByRole("region", { name: "Player" });
    expect(within(player).getByText("Playing Christmas Medley 2017.fseq")).toBeInTheDocument();
    expect(within(player).getByText("7:36 left")).toBeInTheDocument();
    expect(within(player).getByText(/Cannot Ping DDP Channel Data Target 192.0.2.21/)).toBeInTheDocument();
    expect(within(player).getByText(/Next: Christmas Medley 2017.fseq, Mon Oct 5 @ 06:48 PM/)).toBeInTheDocument();

    await user.click(within(player).getByRole("button", { name: "Stop now" }));
    expect(backend.calls).toContain("fppStop:192.0.2.10:now");
    expect(await within(player).findByText("Idle")).toBeInTheDocument();
    expect(within(player).queryByRole("button", { name: "Stop now" })).not.toBeInTheDocument();

    const row = within(player).getByRole("row", { name: /Christmas Medley 2017/ });
    expect(within(row).getByText("9:27")).toBeInTheDocument();
    expect(within(row).getByText("6,148")).toBeInTheDocument();
    await user.click(within(row).getByRole("button", { name: "Play Christmas Medley 2017" }));
    expect(backend.calls).toContain("fppStart:192.0.2.10:Christmas Medley 2017.fseq");
    expect(await within(player).findByText("Playing Christmas Medley 2017.fseq")).toBeInTheDocument();
  });

  it("adds a controller an FPP sends to, then fills it in when that controller is imported", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByRole("button", { name: "Review FPP" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Add Falcon_F16V5_B9F5 to show" }));
    expect(await within(dialog).findByText("In your show")).toBeInTheDocument();
    expect(useApp.getState().snapshot!.show.controllers.map((c) => [c.name, c.ports.length])).toEqual([
      ["Falcon_F16V5_B9F5", 0],
    ]);
    await user.click(within(dialog).getByRole("button", { name: "Close" }));

    await user.click(screen.getByRole("button", { name: "Review Falcon_F16V5_B9F5" }));
    const review = await screen.findByRole("dialog");
    expect(await within(review).findByText("Fills in Falcon_F16V5_B9F5, added from your FPP's output list.")).toBeInTheDocument();
    expect(within(review).queryByText(/already in your show/)).not.toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Add to show" }));
    const controllers = useApp.getState().snapshot!.show.controllers;
    expect(controllers).toHaveLength(1);
    expect(controllers[0].ports.length).toBeGreaterThan(0);
  });

  it("doesn't offer to add a destination PixelFlow can't send to", async () => {
    const { user, backend } = await openDevices();
    backend.deviceNetwork.details[0].config.destinations[0].protocol = "Art-Net";
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByRole("button", { name: "Review FPP" }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText("Not supported yet")).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /to show/ })).not.toBeInTheDocument();
  });

  it("clicking a device's row opens it", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByText("Porch WLED"));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Import Porch WLED" })).toBeInTheDocument();
  });

  it("remembers found controllers, refreshes them on the next scan, and marks ones that don't answer", async () => {
    const { user, backend } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    expect(await screen.findByText("Porch WLED")).toBeInTheDocument();

    // The WLED goes offline; the next scan checks every remembered controller directly.
    backend.deviceNetwork.details = backend.deviceNetwork.details.filter((d) => d.device.kind !== "wled");
    await user.click(screen.getByRole("button", { name: "Scan again" }));
    expect(backend.calls).toContain("discoverDevices:192.0.2.10,192.0.2.20,192.0.2.40:network");
    const row = (await screen.findByText("Porch WLED")).closest("tr")!;
    expect(within(row).getByText(/Not responding · last seen just now/)).toBeInTheDocument();

    // Still there after the app restarts.
    useApp.setState({ discovery: null });
    await useApp.getState().connect(backend);
    expect(useApp.getState().discovery!.devices.map((d) => [d.name, d.responding])).toEqual([
      ["FPP", true],
      ["Falcon_F16V5_B9F5", true],
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

  it("warns before importing a controller that's already in the show", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByRole("button", { name: "Review Porch WLED" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Add to show" }));
    await user.click(screen.getByRole("button", { name: "Review Porch WLED" }));
    expect(await within(await screen.findByRole("dialog")).findByText(/already in your show/)).toBeInTheDocument();
  });
});
