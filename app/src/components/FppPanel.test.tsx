import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { demoDevices, demoPlayers } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";
import { FppPanel } from "./FppPanel";

describe("FppPanel", () => {
  afterEach(() => vi.useRealTimers());

  it("stops offering Stop buttons once the FPP can no longer be reached", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const backend = new MemoryBackend();
    backend.deviceNetwork = demoDevices();
    backend.fppPlayers = demoPlayers();
    useApp.setState({ backend });
    render(<FppPanel address="192.0.2.10" />);
    expect(await screen.findByRole("button", { name: "Stop now" })).toBeInTheDocument();

    delete backend.fppPlayers["192.0.2.10"];
    await vi.advanceTimersByTimeAsync(2100);
    await waitFor(() => expect(screen.queryByRole("button", { name: "Stop now" })).not.toBeInTheDocument());
    expect(screen.getByRole("alert")).toHaveTextContent("Could not reach");
  });
});
