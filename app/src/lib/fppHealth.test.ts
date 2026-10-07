import { describe, expect, it } from "vitest";
import type { Destination, Device } from "../api/types";
import { describeWarning } from "./fppHealth";

const falcon: Destination = {
  address: "10.0.0.175",
  description: "Falcon_F16V5_B9F5",
  protocol: "DDP",
  channels: 6147,
  startChannel: 1,
  startUniverse: null,
  universeSize: null,
  ddpRaw: true,
  unevenUniverses: false,
};

describe("FPP warnings in plain words", () => {
  it("names the controller it can't reach, and says what to do", () => {
    expect(describeWarning("Cannot Ping DDP Channel Data Target 10.0.0.175 Falcon_F16V5_B9F5", [falcon], [])).toEqual({
      title: "Can't reach the Falcon at 10.0.0.175 that this FPP sends to.",
      advice: "Check it's powered on and plugged into the network.",
      fppSays: null,
    });
    const wled: Device = { address: "10.0.0.60", kind: "wled", name: "Porch", model: "", firmware: "", mode: null, foundBy: [] };
    expect(describeWarning("Cannot Ping E1.31 Channel Data Target 10.0.0.60 Porch", [], [wled]).title).toBe(
      "Can't reach the WLED at 10.0.0.60 that this FPP sends to.",
    );
    expect(describeWarning("Cannot Ping DDP Channel Data Target 10.0.0.9 Garage Pixels", [], []).title).toBe(
      "Can't reach Garage Pixels at 10.0.0.9 that this FPP sends to.",
    );
    expect(describeWarning("Cannot Ping DDP Channel Data Target 10.0.0.9", [], []).title).toBe("Can't reach the controller at 10.0.0.9 that this FPP sends to.");
  });

  it("explains restarts, and passes on anything else in FPP's words", () => {
    const restart = describeWarning("FPPD restart required", [], []);
    expect(restart.title).toBe("This FPP needs a restart to use changed settings.");
    expect(restart.fppSays).toBe("FPPD restart required");
    expect(describeWarning("Something new", [], [])).toEqual({ title: "Something new", advice: "Open FPP's web page for details.", fppSays: null });
  });
});
