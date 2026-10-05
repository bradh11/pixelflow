import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { MemoryBackend, emptyShow } from "../../api/memory";
import { newEffect, newRow } from "../../api/sequence";
import type { Edit, Prop, Region } from "../../api/types";
import { newProp } from "../../lib/shows";
import { LayoutScreen } from "../../screens/LayoutScreen";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";

let backend: MemoryBackend;
let edits: Edit[][];

function arch(regions: Region[] = []): Prop {
  return { ...newProp("arch", emptyShow("x")), name: "Garage Arch", regions };
}

const singer: Region = {
  id: "face-1",
  name: "Singer",
  kind: "face",
  mouths: { AI: [{ start: 0, end: 2 }], O: [{ start: 2, end: 4 }] },
  eyesOpen: [{ start: 10, end: 12 }],
  eyesClosed: [{ start: 12, end: 13 }],
  outline: [],
};

async function setup(...props: Prop[]) {
  return setupShow({ ...emptyShow("Test House"), props });
}

async function setupShow(show: ReturnType<typeof emptyShow>) {
  const props = show.props;
  backend = new MemoryBackend(show);
  edits = [];
  const applyEdits = backend.applyEdits.bind(backend);
  backend.applyEdits = async (batch: Edit[]) => {
    edits.push(batch);
    return applyEdits(batch);
  };
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<LayoutScreen />);
  act(() => useLayoutEditor.getState().select([props[0].id]));
  await screen.findByRole("heading", { name: "Submodels & faces" });
  return user;
}

/** The regions the last edit gave the prop. */
const lastRegions = () => {
  const edit = edits[edits.length - 1]?.[0];
  return edit?.type === "updateProp" ? edit.prop.regions : null;
};

const section = () => within(screen.getByRole("heading", { name: "Submodels & faces" }).closest("section")!);

describe("Submodels & faces", () => {
  beforeEach(() => {
    edits = [];
  });

  it("adds a submodel, edits its lines and style, and deletes it, each as one prop edit", async () => {
    const user = await setup(arch());
    expect(section().getByText(/No submodels yet/)).toBeInTheDocument();
    await user.click(section().getByRole("button", { name: "Add submodel" }));
    await waitFor(() => expect(lastRegions()).toMatchObject([{ name: "Submodel 1", kind: "nodes", lines: [[]] }]));
    const id = lastRegions()![0].id;
    expect(useLayoutEditor.getState().highlight).toEqual({ prop: backend.show.props[0].id, region: id, phoneme: null });

    const line = section().getByLabelText("Line 1 pixels");
    await user.type(line, "1-10, 15{Enter}");
    await waitFor(() =>
      expect(lastRegions()![0]).toMatchObject({
        lines: [
          [
            { first: 0, last: 9 },
            { first: 14, last: 14 },
          ],
        ],
      }),
    );
    expect(section().getByRole("button", { name: /^Submodel 1/ })).toHaveTextContent("1 line · 11 pixels");

    const before = edits.length;
    await user.clear(section().getByLabelText("Line 1 pixels"));
    await user.type(section().getByLabelText("Line 1 pixels"), "1-x{Enter}");
    expect(section().getByText('"1-x" isn\'t a pixel number or range; use numbers from 1, like 1-10 or 15.')).toBeInTheDocument();
    expect(edits.length).toBe(before);

    await user.click(section().getByRole("button", { name: "Add line" }));
    await waitFor(() => expect(lastRegions()![0]).toMatchObject({ lines: [[{ first: 0, last: 9 }, { first: 14, last: 14 }], []] }));
    await user.click(section().getByRole("button", { name: "Remove line 2" }));
    await waitFor(() => expect((lastRegions()![0] as { lines: unknown[] }).lines).toHaveLength(1));

    await user.selectOptions(section().getByLabelText("Effects see"), "stackedStrands");
    await waitFor(() => expect(lastRegions()![0]).toMatchObject({ buffer: "stackedStrands" }));
    await user.selectOptions(section().getByLabelText("Lines are"), "vertical");
    await waitFor(() => expect(lastRegions()![0]).toMatchObject({ layout: "vertical" }));

    await user.click(section().getByRole("button", { name: "Delete submodel" }));
    await waitFor(() => expect(lastRegions()).toEqual([]));
    expect(useLayoutEditor.getState().highlight).toBeNull();
  });

  it("refuses a name another submodel or face on the prop already has", async () => {
    const left: Region = { id: "left", name: "Left", kind: "nodes", lines: [[{ first: 0, last: 24 }]], layout: "horizontal", buffer: "default" };
    const user = await setup(arch([left, singer]));
    await user.click(section().getByRole("button", { name: /^Left/ }));
    const name = section().getByLabelText("Name");
    await user.clear(name);
    await user.type(name, "singer{Enter}");
    expect(section().getByText('"singer" is taken on Garage Arch; names must be different on each prop.')).toBeInTheDocument();
    expect(edits).toEqual([]);
    await user.clear(name);
    await user.type(name, "Left side{Enter}");
    await waitFor(() => expect(lastRegions()!.map((r) => r.name)).toEqual(["Left side", "Singer"]));
  });

  it("says what uses a submodel before deleting it", async () => {
    const left: Region = { id: "left", name: "Left", kind: "nodes", lines: [[{ first: 0, last: 24 }]], layout: "horizontal", buffer: "default" };
    const prop = arch([left]);
    const show = { ...emptyShow("Test House"), props: [prop], groups: [{ id: "g", name: "Halves", members: [{ prop: prop.id, region: "left" }] }] };
    const user = await setupShow(show);
    const row = newRow({ region: { prop: prop.id, region: "left" } });
    act(() => useSequencer.setState({ doc: { schemaVersion: 2, name: "Song", audio: null, durationMs: 1000, frameMs: 25, timingTracks: [], rows: [row] } }));
    await user.click(section().getByRole("button", { name: /^Left/ }));
    await user.click(section().getByRole("button", { name: "Delete submodel" }));
    expect(section().getByText("Left is in the group Halves and has 1 row in the open sequence. Deleting it takes it out of the group, and the row will light nothing.")).toBeInTheDocument();
    expect(edits).toEqual([]);
    await user.click(section().getByRole("button", { name: "Keep it" }));
    expect(section().queryByText(/Deleting it takes it out/)).toBeNull();
    await user.click(section().getByRole("button", { name: "Delete submodel" }));
    await user.click(section().getByRole("button", { name: "Delete anyway" }));
    await waitFor(() => expect(lastRegions()).toEqual([]));
    expect(backend.show.groups[0].members).toEqual([]);
  });

  it("says which Faces effects use a face before deleting it, and doesn't offer to rename it", async () => {
    const user = await setup(arch([singer]));
    const prop = backend.show.props[0];
    const row = newRow({ prop: prop.id });
    const sing = newEffect("faces", 0, 500);
    sing.params = { ...sing.params, face: "singer" } as typeof sing.params;
    row.layers[0].effects.push(sing, newEffect("faces", 500, 1000));
    act(() => useSequencer.setState({ doc: { schemaVersion: 2, name: "Song", audio: null, durationMs: 1000, frameMs: 25, timingTracks: [], rows: [row] } }));
    await user.click(section().getByRole("button", { name: /^Singer/ }));
    expect(section().queryByLabelText("Name")).toBeNull();
    expect(section().getByText(/Faces effects find a face by its name/)).toBeInTheDocument();
    await user.click(section().getByRole("button", { name: "Delete face" }));
    expect(section().getByText("Singer sings in 2 Faces effects in the open sequence. Deleting it leaves them with no face.")).toBeInTheDocument();
  });

  it("says when a line lists pixels past the end of the prop", async () => {
    const left: Region = { id: "left", name: "Left", kind: "nodes", lines: [[{ first: 0, last: 4 }]], layout: "horizontal", buffer: "default" };
    const user = await setup(arch([left]));
    await user.click(section().getByRole("button", { name: /^Left/ }));
    const line = section().getByLabelText("Line 1 pixels");
    await user.clear(line);
    await user.type(line, "1-5, 120{Enter}");
    expect(section().getByText("Pixel 120 is past the end of Garage Arch, which has 50 pixels.")).toBeInTheDocument();
    expect(edits).toEqual([]);
    await user.clear(line);
    await user.type(line, "5000000000{Enter}");
    expect(section().getByText("Pixel 5000000000 is past the end of Garage Arch, which has 50 pixels.")).toBeInTheDocument();
    expect(edits).toEqual([]);
  });

  it("lists a face's pixels and shows a mouth shape on the canvas", async () => {
    const user = await setup(arch([singer]));
    const propId = backend.show.props[0].id;
    await user.click(section().getByRole("button", { name: /^Singer/ }));
    expect(section().getByRole("button", { name: /^Singer/ })).toHaveTextContent("Face · 7 pixels");
    expect(useLayoutEditor.getState().highlight).toEqual({ prop: propId, region: "face-1", phoneme: null });
    expect(section().getByText("Mouth AI").nextSibling).toHaveTextContent("1-2");
    expect(section().getByText("Eyes open").nextSibling).toHaveTextContent("11-12");
    expect(section().getByText("Outline").nextSibling).toHaveTextContent("none");
    await user.click(within(section().getByRole("group", { name: "Mouth shape preview" })).getByRole("button", { name: "O" }));
    expect(useLayoutEditor.getState().highlight).toEqual({ prop: propId, region: "face-1", phoneme: "O" });
    // Picking another prop puts the highlight away.
    act(() => useLayoutEditor.getState().select([]));
    expect(useLayoutEditor.getState().highlight).toBeNull();
  });
});
