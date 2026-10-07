import type { Destination, Device, DeviceKind } from "../api/types";

/** One thing an FPP reports, in plain words, with what to do about it. */
export interface HealthProblem {
  title: string;
  advice: string;
  /** FPP's own words, when they say more than the title. */
  fppSays: string | null;
}

const KIND_NOUN: Record<DeviceKind, string> = { fpp: "the FPP", falcon: "the Falcon", wled: "the WLED" };

/** How to call a controller at `address`: "the Falcon", its name, or "the controller". */
function controllerNoun(address: string, description: string, devices: Device[]): string {
  const kind = devices.find((d) => d.address === address)?.kind;
  if (kind) return KIND_NOUN[kind];
  if (/falcon/i.test(description)) return "the Falcon";
  if (/wled/i.test(description)) return "the WLED";
  return description.trim() || "the controller";
}

const PING = /^Cannot Ping (\S+) Channel Data Target (\S+)\s*(.*)$/i;

/** FPP's warning (fppd's WarningHolder text) in plain words. `destinations` and `devices` name
 * the controller a warning is about. */
export function describeWarning(warning: string, destinations: Destination[], devices: Device[]): HealthProblem {
  const ping = PING.exec(warning.trim());
  if (ping) {
    const [, , host, description] = ping;
    const listed = destinations.find((d) => d.address === host);
    const noun = controllerNoun(host, description || listed?.description || "", devices);
    return {
      title: `Can't reach ${noun} at ${host} that this FPP sends to.`,
      advice: "Check it's powered on and plugged into the network.",
      fppSays: null,
    };
  }
  if (/reboot/i.test(warning)) {
    return { title: "This FPP needs a reboot to use changed settings.", advice: "Reboot it from FPP's web page when no show is playing.", fppSays: warning };
  }
  if (/restart/i.test(warning)) {
    return { title: "This FPP needs a restart to use changed settings.", advice: "Restart it from FPP's web page when no show is playing.", fppSays: warning };
  }
  if (/invalid interface/i.test(warning)) {
    return {
      title: "This FPP is set to send on a network connection it doesn't have.",
      advice: "Check the network interface of its outputs on FPP's Channel Outputs page.",
      fppSays: warning,
    };
  }
  return { title: warning, advice: "Open FPP's web page for details.", fppSays: null };
}
