import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoDevices } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";

async function startApp() {
  const backend = new MemoryBackend();
  backend.deviceNetwork = demoDevices();
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<App />);
  return { backend, user };
}

async function openDevices() {
  const app = await startApp();
  await app.user.click(screen.getByRole("button", { name: /start fresh/i }));
  await app.user.click(screen.getByRole("button", { name: "Devices" }));
  return app;
}

describe("devices", () => {
  it("discover my devices from the welcome screen scans right away", async () => {
    const { user, backend } = await startApp();
    await user.click(screen.getByRole("button", { name: /discover my devices/i }));
    expect(await screen.findByText("Falcon_F16V5_B9F5")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Devices" })).toBeInTheDocument();
    expect(backend.calls).toContain("discoverDevices:");
  });

  it("lists controllers with how they were found, and warns about silent ones", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    const falconRow = (await screen.findByText("Falcon_F16V5_B9F5")).closest("tr")!;
    expect(within(falconRow).getByText("Falcon")).toBeInTheDocument();
    expect(within(falconRow).getByText("listed by an FPP")).toBeInTheDocument();
    expect(within(falconRow).getByText("192.0.2.20")).toBeInTheDocument();
    expect(screen.getByText(/isn't responding/)).toHaveTextContent("Falcon_F16V5_Garage (192.0.2.21) isn't responding");
    expect(screen.getByRole("button", { name: "Scan again" })).toBeInTheDocument();
  });

  it("reviews and imports a controller as one undo step", async () => {
    const { user } = await openDevices();
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await user.click(await screen.findByRole("button", { name: "Review Falcon_F16V5_B9F5" }));
    const dialog = await screen.findByRole("dialog", { name: "Import Falcon_F16V5_B9F5" });
    expect(within(dialog).getByText("Falcon Mega Tree")).toBeInTheDocument();
    expect(within(dialog).getByText("Reversed")).toBeInTheDocument();
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
    expect(within(dialog).getByText(/Import those instead/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Nothing to import" })).toBeDisabled();
    await user.keyboard("{Escape}");
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
