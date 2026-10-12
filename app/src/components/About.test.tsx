import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import pkg from "../../package.json";
import conf from "../../src-tauri/tauri.conf.json";
import { App } from "../App";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";
import { COPYRIGHT, LICENCE, SupportLinks } from "./About";

// The accounts are whatever app/support.json holds on the day; here each test sets its own.
const accounts = vi.hoisted(() => ({ githubSponsors: "", koFi: "", buyMeACoffee: "", payPal: "" }));
vi.mock("../../support.json", () => ({ default: accounts }));

beforeEach(() => {
  Object.assign(accounts, { githubSponsors: "", koFi: "", buyMeACoffee: "", payPal: "" });
});

async function start() {
  const backend = new MemoryBackend();
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<App />);
  return { backend, user };
}

const opened = (backend: MemoryBackend) => backend.calls.filter((call) => call.startsWith("openProjectLink"));
const panel = (name: string) => screen.queryByRole("dialog", { name });

describe("About PixelFlow", () => {
  it("says the version, whose it is, the licence, that there is no warranty, and what is owed to xLights", async () => {
    const { user } = await start();
    await user.click(screen.getByRole("button", { name: "About PixelFlow" }));
    const about = panel("About PixelFlow")!;
    expect(about).toHaveTextContent(`PixelFlow ${pkg.version}`);
    expect(about).toHaveTextContent(COPYRIGHT);
    expect(about).toHaveTextContent("Free software under the GNU General Public License, version 3.");
    expect(about).toHaveTextContent("It comes with no warranty.");
    // Both things at once: its own project, and parts of it derived, not only inspired.
    expect(about).toHaveTextContent("PixelFlow is an independent project, inspired by xLights and able to import its shows.");
    expect(about).toHaveTextContent("Parts of it are derived from xLights' source code and are used under xLights' licence, with thanks to its authors.");
    await user.keyboard("{Escape}");
    expect(panel("About PixelFlow")).not.toBeInTheDocument();
  });

  it("opens the licence, the credits, the notices, and the two projects' pages through the shell, by name", async () => {
    const { backend, user } = await start();
    await user.click(screen.getByRole("button", { name: "About PixelFlow" }));
    const about = within(panel("About PixelFlow")!);
    expect(opened(backend)).toEqual([]);
    for (const name of ["Licence", "Credits", "Third-party notices", "Source code", "xLights"]) {
      await user.click(about.getByRole("button", { name }));
    }
    expect(opened(backend)).toEqual(["licence", "credits", "notices", "source", "xlights"].map((link) => `openProjectLink:${link}`));
    expect(about.getByRole("button", { name: "xLights" })).toHaveAttribute("title", "Opens github.com/xLightsSequencer/xLights in your web browser");
  });

  it("agrees with what the built app says about itself", () => {
    expect(conf.bundle.license).toBe(pkg.license);
    expect(conf.bundle.license).toBe("GPL-3.0-only");
    expect(conf.bundle.copyright.startsWith(`${COPYRIGHT}.`)).toBe(true);
    expect(conf.bundle.copyright).toContain(LICENCE);
    expect(conf.version).toBe(pkg.version);
    // The licence and both notices travel inside the app, under the names About opens.
    expect(Object.values(conf.bundle.resources).sort()).toEqual(["LICENSE.txt", "NOTICE.txt", "THIRD-PARTY-NOTICES.txt"]);
  });

  it("is in Settings too, and comes from the menu bar", async () => {
    const { backend, user } = await start();
    await user.click(screen.getByRole("button", { name: /^new show/i }));
    useApp.getState().setScreen("settings");
    expect(await screen.findByText(COPYRIGHT)).toBeInTheDocument();
    expect(panel("About PixelFlow")).not.toBeInTheDocument();
    backend.chooseMenu({ action: "about" });
    expect(await screen.findByRole("dialog", { name: "About PixelFlow" })).toBeInTheDocument();
  });
});

describe("Support PixelFlow", () => {
  it("is nowhere while no way to give is set up", async () => {
    const { backend, user } = await start();
    // The start page, the command palette, About, and the menu bar's item (were it ever sent).
    expect(screen.queryByRole("button", { name: /support/i })).not.toBeInTheDocument();
    useApp.getState().setPaletteOpen(true);
    await screen.findByPlaceholderText("Type a command…");
    expect(screen.getByRole("option", { name: /^About PixelFlow/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /support/i })).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    backend.chooseMenu({ action: "support" });
    const about = await screen.findByRole("dialog", { name: "About PixelFlow" });
    expect(about).not.toHaveTextContent(/support/i);
    expect(panel("Support PixelFlow")).not.toBeInTheDocument();
    const { container } = render(<SupportLinks />);
    expect(container).toBeEmptyDOMElement();
    expect(opened(backend)).toEqual([]);
  });

  it("lists only the ways that are set up, says where each goes, and opens one through the shell when asked", async () => {
    Object.assign(accounts, { koFi: "made-up-name", payPal: "MadeUp_1" });
    const { backend, user } = await start();
    await user.click(screen.getByRole("button", { name: "Support PixelFlow" }));
    const support = within(panel("Support PixelFlow")!);
    const ways = within(support.getByRole("list", { name: "Ways to support PixelFlow" })).getAllByRole("button");
    expect(ways.map((button) => button.textContent?.trim())).toEqual(["Ko-fi", "PayPal"]);
    expect(ways[0]).toHaveAttribute("title", "Opens ko-fi.com/made-up-name in your web browser");
    expect(ways[1]).toHaveAttribute("title", "Opens paypal.me/MadeUp_1 in your web browser");
    // Looking opens nothing; asking opens the one asked for, by name, never by address.
    expect(opened(backend)).toEqual([]);
    await user.click(ways[0]);
    expect(opened(backend)).toEqual(["openProjectLink:koFi"]);
  });

  it("is in About and the command palette once set up, and the menu bar swaps one panel for the other", async () => {
    Object.assign(accounts, { githubSponsors: "made-up-name" });
    const { backend, user } = await start();
    useApp.getState().setPaletteOpen(true);
    await user.type(await screen.findByPlaceholderText("Type a command…"), "support");
    await user.click(screen.getByRole("option", { name: "Support PixelFlow…" }));
    expect(within(panel("Support PixelFlow")!).getByRole("button", { name: "GitHub Sponsors" })).toBeInTheDocument();
    backend.chooseMenu({ action: "about" });
    const about = within(await screen.findByRole("dialog", { name: "About PixelFlow" }));
    expect(about.getByRole("heading", { name: "Support PixelFlow" })).toBeInTheDocument();
    await user.click(about.getByRole("button", { name: "GitHub Sponsors" }));
    expect(opened(backend)).toEqual(["openProjectLink:githubSponsors"]);
    backend.chooseMenu({ action: "support" });
    expect(await screen.findByRole("dialog", { name: "Support PixelFlow" })).toBeInTheDocument();
  });

  it("never comes up by itself", async () => {
    Object.assign(accounts, { githubSponsors: "made-up-name", koFi: "made-up-name" });
    const { backend, user } = await start();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^new show/i }));
    await screen.findByRole("button", { name: /^Commands/ });
    expect(screen.queryByRole("dialog", { name: /PixelFlow/ })).not.toBeInTheDocument();
    expect(opened(backend)).toEqual([]);
  });
});
