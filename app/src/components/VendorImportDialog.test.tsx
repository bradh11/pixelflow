import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import { DEMO_VENDOR_PATH, demoVendorPackage } from "../api/memoryVendor";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

async function startApp({ saved = false } = {}) {
  const backend = new MemoryBackend(demoShow());
  if (saved) backend.path = "/Shows/House/house.pixelflow.json";
  await useApp.getState().connect(backend);
  const seq = new MemorySequencer(backend);
  seq.vendorPackages.set(DEMO_VENDOR_PATH, demoVendorPackage());
  seq.nextXlightsSequencePath = DEMO_VENDOR_PATH;
  await useSequencer.getState().connect(seq);
  useApp.setState({ started: true, sequenceImportReport: null, vendorImport: null, error: null });
  const user = userEvent.setup();
  render(<App />);
  return { backend, seq, user };
}

async function openMapping() {
  await act(() => useApp.getState().importXlightsSequence());
  return screen.findByRole("dialog", { name: "Map Holiday Mashup onto your props" });
}

/** The vendor item's row in the dialog. */
function row(dialog: HTMLElement, label: string) {
  return within(within(dialog).getByRole("list", { name: "Their models" })).getByRole("listitem", { name: label });
}

describe("mapping a vendor's sequence onto the show", () => {
  it("opens with the suggested mapping, unmapped busy items standing out", async () => {
    const { seq } = await startApp();
    const dialog = await openMapping();
    expect(seq.calls).toContain(`inspectXlightsSequence:${DEMO_VENDOR_PATH}`);
    expect(seq.calls.some((c) => c.startsWith("importXlightsSequence"))).toBe(false);
    // Like with like: the tree to the tree, the matrix to the matrix, one arch to the arch.
    expect(within(row(dialog, "MegaTree 16x50")).getByText("Mega Tree")).toBeInTheDocument();
    expect(within(row(dialog, "P10 Matrix")).getByText("Window Matrix")).toBeInTheDocument();
    expect(within(row(dialog, "Arch 1")).getByText("Garage Arch")).toBeInTheDocument();
    expect(within(dialog).getByText("4 of 12 mapped (56% of effects)")).toBeInTheDocument();
    // The second arch isn't piled onto the same arch, but it's offered.
    const arch2 = row(dialog, "Arch 2");
    expect(within(arch2).getByText("Not mapped")).toBeInTheDocument();
    expect(within(arch2).getByRole("button", { name: "Use Garage Arch?" })).toBeInTheDocument();
    // Busy and unmapped stands out; a flood with a few effects doesn't.
    expect(row(dialog, "Whole House GRP").className).toContain("border-l-amber-500");
    expect(row(dialog, "Flood Left").className).not.toContain("border-l-amber-500");
    // Their submodels and strands are folded under their model.
    expect(within(dialog).queryByRole("listitem", { name: "Star Topper" })).not.toBeInTheDocument();
  });

  it("maps an item to several props, takes a hint, clears, and auto-maps again", async () => {
    const { user } = await startApp();
    const dialog = await openMapping();
    // Whole House to two props, found by searching.
    await user.click(within(row(dialog, "Whole House GRP")).getByRole("button", { name: "Map Whole House GRP" }));
    const search = within(dialog).getByRole("textbox", { name: "Find a prop or group for Whole House GRP" });
    await user.type(search, "star");
    const options = within(dialog).getByRole("listbox", { name: "Props for Whole House GRP" });
    await user.click(within(options).getByRole("option", { name: /Porch Star/ }));
    await user.clear(search);
    await user.type(search, "tree{Enter}");
    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    const whole = row(dialog, "Whole House GRP");
    expect(within(whole).getByText("Porch Star")).toBeInTheDocument();
    expect(within(whole).getByText("Mega Tree")).toBeInTheDocument();
    // Taking a hint; removing one.
    await user.click(within(row(dialog, "Arch 2")).getByRole("button", { name: "Use Garage Arch?" }));
    expect(within(row(dialog, "Arch 2")).getByText("Garage Arch")).toBeInTheDocument();
    await user.click(within(whole).getByRole("button", { name: "Remove Porch Star from Whole House GRP" }));
    expect(within(row(dialog, "Whole House GRP")).queryByText("Porch Star")).not.toBeInTheDocument();
    expect(within(dialog).getByText("6 of 12 mapped (78% of effects)")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: /Clear/ }));
    expect(within(dialog).getByText("0 of 12 mapped (0% of effects)")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /^Import/ })).toBeDisabled();
    await user.click(within(dialog).getByRole("button", { name: /Auto-map/ }));
    expect(within(dialog).getByText("4 of 12 mapped (56% of effects)")).toBeInTheDocument();
  });

  it("shows submodels and strands with effects under their model", async () => {
    const { user } = await startApp();
    const dialog = await openMapping();
    await user.click(within(dialog).getByRole("button", { name: "Show the parts of MegaTree 16x50" }));
    expect(row(dialog, "Star Topper")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Show the parts of Arch 1" }));
    expect(row(dialog, "Strand 1")).toBeInTheDocument();
    // Searching finds parts and opens their model.
    await user.click(within(dialog).getByRole("button", { name: "Hide the parts of Arch 1" }));
    await user.type(within(dialog).getByRole("textbox", { name: "Find a vendor model" }), "strand");
    expect(row(dialog, "Strand 1")).toBeInTheDocument();
    expect(within(dialog).queryByRole("listitem", { name: "P10 Matrix" })).not.toBeInTheDocument();
  });

  it("loads and saves xLights mappings", async () => {
    const { seq, user } = await startApp();
    seq.xmapFiles.set("/Maps/vendor.xmap", { items: { "Spinner 1": ["Porch Star"], "Somebody Else's Prop": ["Mega Tree"] } });
    seq.nextXmapPath = "/Maps/vendor.xmap";
    seq.nextXmapSavePath = "/Maps/mine.xmap";
    const dialog = await openMapping();
    await user.click(within(dialog).getByRole("button", { name: /Load .xmap/ }));
    expect(await within(dialog).findByText(/Loaded 1 mapping from vendor.xmap; left out 1 mapping for models this sequence doesn't have/)).toBeInTheDocument();
    expect(within(row(dialog, "Spinner 1")).getByText("Porch Star")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /Save .xmap/ }));
    expect(seq.xmapFiles.get("/Maps/mine.xmap")?.items["Spinner 1"]).toEqual(["Porch Star"]);
    expect(seq.xmapFiles.get("/Maps/mine.xmap")?.items["MegaTree 16x50"]).toEqual(["Mega Tree"]);
  });

  it("imports with the mapping, reports what came in, and starts there next time", async () => {
    const { seq, user } = await startApp({ saved: true });
    let dialog = await openMapping();
    expect(within(dialog).getByText(/Holiday Mashup.mp3 goes in \/Shows\/House\/music/)).toBeInTheDocument();
    await user.click(within(row(dialog, "Arch 2")).getByRole("button", { name: "Use Garage Arch?" }));
    await user.click(within(dialog).getByRole("button", { name: /^Import/ }));
    const report = await screen.findByRole("dialog", { name: "Imported Holiday Mashup" });
    expect(screen.queryByRole("dialog", { name: /Map Holiday Mashup/ })).not.toBeInTheDocument();
    expect(within(report).getByText(/weren't mapped to anything/)).toBeInTheDocument();
    expect(seq.calls).toContain(`importXlightsSequence:${DEMO_VENDOR_PATH}`);
    // Both arches layered on the garage arch; the music next to the show.
    const doc = useSequencer.getState().doc!;
    expect(doc.audio).toBe("/Shows/House/music/Holiday Mashup.mp3");
    const arch = useApp.getState().snapshot!.show.props.find((p) => p.name === "Garage Arch")!;
    const archRow = doc.rows.find((r) => "prop" in r.target && r.target.prop === arch.id)!;
    expect(archRow.layers).toHaveLength(2);
    expect(useApp.getState().screen).toBe("sequence");
    await user.click(within(report).getByRole("button", { name: "Done" }));

    // The vendor's next import starts from this mapping, skips included. (The import is the open,
    // unsaved sequence, so it asks first.)
    await act(() => useApp.getState().importXlightsSequence());
    const ask = await screen.findByRole("dialog", { name: "Unsaved changes" });
    await user.click(within(ask).getByRole("button", { name: "Don't save" }));
    dialog = await screen.findByRole("dialog", { name: "Map Holiday Mashup onto your props" });
    expect(within(row(dialog, "Arch 2")).getByText("Garage Arch")).toBeInTheDocument();
    expect(within(row(dialog, "Arch 2")).queryByRole("button", { name: /Use/ })).not.toBeInTheDocument();
    expect(within(dialog).getByText("5 of 12 mapped (64% of effects)")).toBeInTheDocument();
  });

  it("asks where the music goes while the show isn't saved, and switches between the package's sequences", async () => {
    const { backend, seq, user } = await startApp();
    backend.nextDownloadFolder = "/Users/demo/Lights";
    const dialog = await openMapping();
    expect(within(dialog).getByText(/Your show isn't saved yet, so choose a folder for Holiday Mashup.mp3/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /Choose folder/ }));
    expect(within(dialog).getByText(/Holiday Mashup.mp3 goes in \/Users\/demo\/Lights\/music/)).toBeInTheDocument();

    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Sequence" }), "Holiday Mashup (short).xsq");
    const short = await screen.findByRole("dialog", { name: "Map Holiday Mashup (short) onto your props" });
    expect(seq.calls.filter((c) => c.startsWith("inspectXlightsSequence"))).toHaveLength(2);
    expect(within(short).queryByRole("listitem", { name: "Spinner 1" })).not.toBeInTheDocument();
    await user.click(within(short).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(seq.calls.some((c) => c.startsWith("importXlightsSequence"))).toBe(false);
  });
});
