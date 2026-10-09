import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { MemoryBackend } from "../../api/memory";
import { FakeAssistant } from "../../api/memoryAssistant";
import { MemorySequencer } from "../../api/memorySequencer";
import { useAssistant } from "../../state/assistant";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { AssistantError, assistantFailure, type Change, type ProposalView, modelLabel } from "../../api/assistant";
import { highlightFrame } from "./DraftPreview";
import { ProposalCard } from "./ProposalCard";

const KEY = "sk-test-not-a-key";

async function start({ setUp = true } = {}) {
  const backend = new MemoryBackend(demoShow());
  const assistant = new FakeAssistant(backend);
  if (setUp) {
    assistant.keys.set("anthropic", "keychain");
    useAssistant.getState().setModel("claude-opus-5-5");
  }
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  await useAssistant.getState().connect(assistant);
  const user = userEvent.setup();
  render(<App />);
  return { user, backend, assistant };
}

async function openPanel(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: /assistant/i, pressed: false }));
  return screen.getByRole("complementary", { name: "Assistant" });
}

describe("model names", () => {
  it("are short and readable", () => {
    expect(modelLabel("claude-opus-5-5")).toBe("Claude Opus 5.5");
    expect(modelLabel("claude-sonnet-4-5-20250929")).toBe("Claude Sonnet 4.5");
    expect(modelLabel("claude-haiku-4")).toBe("Claude Haiku 4");
    expect(modelLabel("gpt-5")).toBe("GPT-5");
    expect(modelLabel("gpt-4.1-mini")).toBe("GPT-4.1 mini");
    expect(modelLabel("my-local-model")).toBe("my-local-model");
  });
});

describe("the assistant in a narrow window", () => {
  const resize = (width: number) =>
    act(() => {
      window.innerWidth = width;
      window.dispatchEvent(new Event("resize"));
    });
  const panel = () => screen.getByRole("complementary", { name: "Assistant" });
  const sidebarRail = () => screen.getByRole("navigation", { name: "Screens" }).dataset.collapsed === "true";

  it("on a laptop (1200–1439 px) takes a narrower column, and the sidebar folds to icons while it's open", async () => {
    window.innerWidth = 1360;
    const { user } = await start();
    expect(sidebarRail()).toBe(false);
    await openPanel(user);
    expect(panel()).toHaveAttribute("data-overlay", "false");
    expect(panel()).toHaveAttribute("data-width", "compact");
    expect(sidebarRail()).toBe(true);
    await user.click(within(panel()).getByRole("button", { name: "Close assistant" }));
    expect(sidebarRail()).toBe(false);
  });

  it("floats over the screen only below 1200 px, and takes a full column from 1440", async () => {
    window.innerWidth = 1100;
    const { user } = await start();
    await openPanel(user);
    expect(panel()).toHaveAttribute("data-overlay", "true");
    resize(1600);
    expect(panel()).toHaveAttribute("data-overlay", "false");
    expect(panel()).toHaveAttribute("data-width", "full");
  });

  it("floating, Escape puts it away and gives the focus back to the Assistant button", async () => {
    window.innerWidth = 1024;
    const { user } = await start();
    await openPanel(user);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("complementary", { name: "Assistant" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Assistant/ })).toHaveFocus();
  });

  it("floating, Escape keeps a typed message: the first only leaves the box, and the text survives closing", async () => {
    window.innerWidth = 1024;
    const { user } = await start();
    await openPanel(user);
    const box = () => screen.getByRole("textbox", { name: "Message the assistant" });
    await user.type(box(), "Add arches");
    await user.keyboard("{Escape}");
    expect(panel()).toBeInTheDocument();
    expect(box()).not.toHaveFocus();
    expect(box()).toHaveValue("Add arches");
    // Mid-composition (an input method), Escape is the input method's.
    box().focus();
    fireEvent.keyDown(box(), { key: "Escape", isComposing: true });
    expect(panel()).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("complementary", { name: "Assistant" })).not.toBeInTheDocument();
    await openPanel(user);
    expect(box()).toHaveValue("Add arches");
  });
});

describe("opening the assistant", () => {
  it("opens from the top bar, with ⌘L, and from the command palette", async () => {
    const { user } = await start();
    const panel = await openPanel(user);
    // The model by a short name; the provider and its full id on hover.
    expect(within(panel).getByText("Claude Opus 5.5")).toHaveAttribute("title", "Anthropic · claude-opus-5-5: change the provider or model");
    await user.click(within(panel).getByRole("button", { name: "Close assistant" }));
    expect(screen.queryByRole("complementary", { name: "Assistant" })).not.toBeInTheDocument();

    await user.keyboard("{Meta>}l{/Meta}");
    expect(screen.getByRole("complementary", { name: "Assistant" })).toBeInTheDocument();
    await user.keyboard("{Meta>}l{/Meta}");
    expect(screen.queryByRole("complementary", { name: "Assistant" })).not.toBeInTheDocument();

    await user.keyboard("{Meta>}k{/Meta}");
    await user.click(await screen.findByText("Open the assistant"));
    expect(screen.getByRole("complementary", { name: "Assistant" })).toBeInTheDocument();
  });

  it("asks to connect a model when there's no key", async () => {
    const { user } = await start({ setUp: false });
    const panel = await openPanel(user);
    expect(within(panel).getByText("Connect an AI model")).toBeInTheDocument();
    expect(within(panel).getByRole("textbox", { name: "Message the assistant" })).toBeDisabled();
    await user.click(within(panel).getByRole("button", { name: /set up in settings/i }));
    expect(screen.getByRole("dialog", { name: "Settings → AI" })).toBeInTheDocument();
  });
});

describe("Settings → AI", () => {
  it("saves a key write-only, then loads models live", async () => {
    const { user, assistant } = await start({ setUp: false });
    useAssistant.getState().setSettingsOpen(true);
    const dialog = await screen.findByRole("dialog", { name: "Settings → AI" });
    expect(within(dialog).getByText("No key yet")).toBeInTheDocument();
    const field = within(dialog).getByLabelText("Anthropic API key");
    expect(field).toHaveAttribute("type", "password");
    expect(field).toHaveAttribute("autocomplete", "off");
    await user.type(field, KEY);
    await user.click(within(dialog).getByRole("button", { name: /save key/i }));

    expect(await within(dialog).findByText("Key saved in your Keychain")).toBeInTheDocument();
    // The field is cleared, and nothing in the window keeps the key.
    expect(field).toHaveValue("");
    expect(JSON.stringify(useAssistant.getState())).not.toContain(KEY);
    expect(JSON.stringify(localStorage)).not.toContain(KEY);
    expect(JSON.stringify([...assistant.keys])).not.toContain(KEY);

    const models = within(dialog).getByLabelText("Model");
    await waitFor(() => expect(models).toHaveValue("claude-opus-5-5"));
    expect(within(models).getByRole("option", { name: "Claude Opus 5.5 (suggested)" })).toBeInTheDocument();
    await user.selectOptions(models, "claude-sonnet-5-5");
    expect(useAssistant.getState().models.anthropic).toBe("claude-sonnet-5-5");
    expect(useAssistant.getState().hasKey).toBe(true);

    await user.click(within(dialog).getByRole("button", { name: /remove key/i }));
    expect(await within(dialog).findByText("No key yet")).toBeInTheDocument();
    expect(useAssistant.getState().hasKey).toBe(false);
  });

  it("offers this session only when there's no keychain", async () => {
    const { user, assistant } = await start({ setUp: false });
    assistant.storage = { name: "system keyring", available: false };
    useAssistant.getState().setSettingsOpen(true);
    const dialog = await screen.findByRole("dialog", { name: "Settings → AI" });
    expect(await within(dialog).findByText(/This computer has no system keyring PixelFlow can use/)).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Anthropic API key"), KEY);
    await user.click(within(dialog).getByRole("button", { name: /use for this session only/i }));
    expect(await within(dialog).findByText(/Key kept for this session only/)).toBeInTheDocument();
    expect(assistant.keys.get("anthropic")).toBe("session");
  });

  it("switches provider and remembers the model per provider", async () => {
    const { user, assistant } = await start();
    assistant.keys.set("openai", "keychain");
    useAssistant.getState().setSettingsOpen(true);
    const dialog = await screen.findByRole("dialog", { name: "Settings → AI" });
    await user.selectOptions(within(dialog).getByLabelText("Provider"), "openai");
    await waitFor(() => expect(within(dialog).getByLabelText("Model")).toHaveValue("gpt-5.1"));
    expect(JSON.parse(localStorage.getItem("pixelflow.ai")!)).toEqual({
      provider: "openai",
      models: { anthropic: "claude-opus-5-5", openai: "gpt-5.1" },
    });
  });
});

describe("chatting", () => {
  it("drafts a proposal, previews it, and applies it as one undo step", async () => {
    const { user, backend } = await start();
    const panel = await openPanel(user);
    const before = backend.show.props.length;
    await user.type(within(panel).getByRole("textbox", { name: "Message the assistant" }), "Add two arches beside the garage{Enter}");

    const card = await within(panel).findByRole("region", { name: "Proposed changes" });
    expect(within(card).getByText(/Adds 2 arches beside your other props/)).toBeInTheDocument();
    // Two props and their group.
    expect(within(card).getByText("3 to add")).toBeInTheDocument();
    expect(within(card).getByText("Props")).toBeInTheDocument();
    expect(within(card).getByText("Groups")).toBeInTheDocument();
    expect(await within(panel).findByText(/I drafted 2 arches/)).toBeInTheDocument();
    // Nothing changed yet.
    expect(backend.show.props.length).toBe(before);

    await user.click(within(card).getByRole("button", { name: "Preview" }));
    const preview = await screen.findByRole("dialog", { name: "Preview: not applied yet" });
    expect(backend.show.props.length).toBe(before);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Preview: not applied yet" })).not.toBeInTheDocument();
    expect(preview).not.toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Apply" }));
    expect(await within(card).findByText(/Applied as one step/)).toBeInTheDocument();
    expect(backend.show.props.length).toBe(before + 2);
    expect(backend.show.groups.some((g) => g.name === "Arches")).toBe(true);
    expect(useApp.getState().snapshot!.show.props.length).toBe(before + 2);

    await act(() => useApp.getState().undo());
    expect(backend.show.props.length).toBe(before);
    expect(backend.show.groups.some((g) => g.name === "Arches")).toBe(false);
  });

  it("discarding changes nothing", async () => {
    const { user, backend } = await start();
    const panel = await openPanel(user);
    const before = structuredClone(backend.show);
    await user.click(within(panel).getByRole("button", { name: "Rename the show to Christmas 2026" }));
    const card = await within(panel).findByRole("region", { name: "Proposed changes" });
    expect(within(card).getByText(/name: ".*" → "Christmas 2026"/)).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Discard" }));
    expect(await within(card).findByText("Discarded. Nothing was changed.")).toBeInTheDocument();
    expect(backend.show).toEqual(before);
  });

  it("answers questions with what the user has selected", async () => {
    const { user, backend } = await start();
    useLayoutEditor.setState({ selected: [backend.show.props[0].id] });
    const panel = await openPanel(user);
    await user.click(within(panel).getByRole("button", { name: "What's in my show?" }));
    expect(await within(panel).findByText(new RegExp(`You have ${backend.show.props[0].name} selected`))).toBeInTheDocument();
    expect(within(panel).queryByRole("region", { name: "Proposed changes" })).not.toBeInTheDocument();
  });

  it("shows provider errors in plain language", async () => {
    const { user, assistant } = await start();
    assistant.nextError = "Anthropic is limiting how fast this key can send requests. Wait a minute, then try again.";
    const panel = await openPanel(user);
    await user.type(within(panel).getByRole("textbox", { name: "Message the assistant" }), "hello{Enter}");
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Wait a minute, then try again.");
  });

  it("keeps the provider's own words under Details", async () => {
    const { user, assistant } = await start();
    assistant.nextError = new AssistantError(
      'The model "gpt-x" doesn\'t accept the "reasoning.effort" setting PixelFlow sends. Pick another model in Settings → AI.',
      "HTTP 400 unsupported_parameter (reasoning.effort): Unsupported parameter: 'reasoning.effort' is not supported with this model.",
    );
    const panel = await openPanel(user);
    await user.type(within(panel).getByRole("textbox", { name: "Message the assistant" }), "hello{Enter}");
    const alert = await within(panel).findByRole("alert");
    expect(alert).toHaveTextContent('doesn\'t accept the "reasoning.effort" setting');
    const details = within(alert).getByText("Details");
    expect(within(alert).getByText(/HTTP 400 unsupported_parameter/)).not.toBeVisible();
    await user.click(details);
    expect(within(alert).getByText(/HTTP 400 unsupported_parameter/)).toBeVisible();
  });

  it("turns an app error with details into a message and details", () => {
    const error = assistantFailure({ message: "OpenAI is busy right now. Try again in a moment.", details: "HTTP 503 server_is_overloaded: busy" });
    expect(error).toBeInstanceOf(AssistantError);
    expect((error as AssistantError).message).toBe("OpenAI is busy right now. Try again in a moment.");
    expect((error as AssistantError).details).toBe("HTTP 503 server_is_overloaded: busy");
    expect(assistantFailure("plain")).toBe("plain");
  });

  it("Stop ends a reply in progress", async () => {
    const { user, assistant } = await start();
    assistant.delayMs = 30;
    const panel = await openPanel(user);
    await user.type(within(panel).getByRole("textbox", { name: "Message the assistant" }), "What's in my show?{Enter}");
    await user.click(await within(panel).findByRole("button", { name: "Stop" }));
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Stopped.");
    expect(within(panel).getByRole("button", { name: "Send" })).toBeInTheDocument();
  });

  it("New chat clears the conversation", async () => {
    const { user } = await start();
    const panel = await openPanel(user);
    await user.click(within(panel).getByRole("button", { name: "What's in my show?" }));
    await within(panel).findByText(/Your show/);
    await user.click(within(panel).getByRole("button", { name: "New chat" }));
    expect(within(panel).queryByText(/Your show/)).not.toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "What's in my show?" })).toBeInTheDocument();
  });
});

describe("creating a sequence", () => {
  async function startWithSequencer() {
    const backend = new MemoryBackend(demoShow());
    backend.nextAudioPath = "/Music/Jingle Bell Rock.mp3";
    const sequencer = new MemorySequencer(backend);
    const assistant = new FakeAssistant(backend);
    assistant.sequencer = sequencer;
    assistant.keys.set("anthropic", "keychain");
    useAssistant.getState().setModel("claude-opus-5-5");
    await useApp.getState().connect(backend);
    useApp.setState({ started: true, screen: "sequence" });
    await useSequencer.getState().connect(sequencer);
    await useAssistant.getState().connect(assistant);
    const user = userEvent.setup();
    render(<App />);
    return { user, backend, sequencer, assistant };
  }

  it("says the open sequence closes, and keeps the chat (not the window) scrolling", async () => {
    const { user, sequencer } = await startWithSequencer();
    await act(() => useSequencer.getState().newSequence("Carol of the Bells", 30_000, null, []));
    expect(sequencer.doc?.name).toBe("Carol of the Bells");
    const panel = await openPanel(user);
    await user.type(within(panel).getByRole("textbox", { name: "Message the assistant" }), "Make me a new sequence{Enter}");
    const offer = await within(panel).findByRole("region", { name: "Choose a song" });
    expect(offer).toHaveTextContent('This closes "Carol of the Bells"');
    // Positioned scroll containers keep hidden labels inside them, so a tall card scrolls the
    // chat, never the window.
    expect(panel.querySelector("[aria-live]")).toHaveClass("relative");
  });

  it("asks for a song, makes the sequence, and proposes a whole show that plays before Apply", async () => {
    const { user, sequencer, assistant } = await startWithSequencer();
    const panel = await openPanel(user);
    await user.click(within(panel).getByRole("button", { name: "Create a compelling sequence" }));
    expect(await within(panel).findByText(/You don't have a sequence open yet/)).toBeInTheDocument();
    const offer = await within(panel).findByRole("region", { name: "Choose a song" });
    expect(sequencer.doc).toBeNull();

    // The user picks the song; a new, unsaved sequence with a row per prop and group is made, and
    // the assistant carries on by itself.
    await user.click(within(offer).getByRole("button", { name: /choose a song/i }));
    expect(await within(panel).findByText("New sequence: Jingle Bell Rock")).toBeInTheDocument();
    expect(sequencer.doc?.name).toBe("Jingle Bell Rock");
    expect(sequencer.doc?.rows.length).toBeGreaterThan(0);
    expect(useSequencer.getState().dirty).toBe(false);
    // The assistant finds the beats itself: the screen doesn't offer to while it works.
    expect(useSequencer.getState().suggestBeats).toBe(false);
    const card = await within(panel).findByRole("region", { name: "Proposed changes" });
    // The file name reaches the assistant only as data (the context block), never as the user's own words.
    expect(within(panel).getByText("I chose a song, and the new sequence is open. Go ahead.")).toBeInTheDocument();
    expect(assistant.sent.at(-1)).toBe("I chose a song, and the new sequence is open. Go ahead.");
    expect(within(card).getByText("By section")).toBeInTheDocument();
    expect(within(card).getByText(/Locked \d+ edges to the music/)).toBeInTheDocument();
    expect(within(card).getByText("Intro", { selector: "span" })).toBeInTheDocument();
    expect(within(card).getByRole("img", { name: /Timeline of the draft/ })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: /Show all \d+/ })).toBeInTheDocument();
    // Nothing is in the sequence until Apply.
    expect(sequencer.doc!.rows.every((r) => r.layers.every((l) => l.effects.length === 0))).toBe(true);

    const frames = vi.spyOn(assistant, "previewFrame");
    await user.click(within(card).getByRole("button", { name: "Play preview" }));
    const preview = await screen.findByRole("dialog", { name: "Preview: not applied yet" });
    await waitFor(() => expect(frames).toHaveBeenCalled());
    expect(within(preview).getByRole("slider", { name: "Position" })).toBeInTheDocument();
    await user.click(within(preview).getByRole("button", { name: "Pause" }));
    expect(within(preview).getByRole("button", { name: "Play" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(sequencer.doc!.timingTracks).toEqual([]);

    await user.click(within(card).getByRole("button", { name: "Apply" }));
    expect(await within(card).findByText(/Applied as one step/)).toBeInTheDocument();
    const effects = sequencer.doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects));
    expect(effects.length).toBeGreaterThan(20);
    expect(sequencer.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars", "Sections"]);
    expect(useSequencer.getState().suggestBeats).toBe(false);
    await waitFor(() => expect(useSequencer.getState().doc?.timingTracks.length).toBe(3));
    await act(() => useSequencer.getState().undo());
    expect(sequencer.doc!.timingTracks).toEqual([]);
    expect(sequencer.doc!.rows.every((r) => r.layers.every((l) => l.effects.length === 0))).toBe(true);
  });
});

describe("the review card", () => {
  const proposal = (changes: Change[]): ProposalView => ({
    id: "p1",
    summary: "Adds a controller.",
    diff: { changes },
    changedProps: [],
    changesShow: true,
    changesSequence: false,
    sections: [],
    timeline: null,
    lockedEdges: 0,
  });
  const falcon: Change = {
    section: "controller",
    action: "added",
    name: "Falcon 2",
    id: "c1",
    details: ["address: 203.0.113.9", "protocol: DDP", "port 1: Roofline", "port 2: Arch 1", "port 3: Arch 2", "port 4: Tree", "port 5: Star"],
    warnings: ["Sends light data to a new address: 203.0.113.9"],
  };

  it("never folds away a change that carries a warning", async () => {
    await start();
    const renames: Change[] = Array.from({ length: 12 }, (_, i) => ({
      section: "controller",
      action: "changed",
      name: `Controller ${i + 1}`,
      id: `c${i}`,
      details: [`name: "Old ${i}" → "Controller ${i + 1}"`],
      warnings: [],
    }));
    const effects: Change[] = Array.from({ length: 20 }, (_, i) => ({
      section: "effect",
      action: "added",
      name: `Twinkle ${i + 1}`,
      id: `e${i}`,
      details: [],
      warnings: [],
    }));
    render(<ProposalCard proposal={proposal([...renames, { ...falcon, action: "changed" }, ...effects])} current />);
    const card = screen.getByRole("region", { name: "Proposed changes" });
    // All 13 controller changes show, warning included, without expanding anything.
    expect(within(card).getByText("Sends light data to a new address: 203.0.113.9")).toBeVisible();
    expect(within(card).getByText("Controller 12")).toBeInTheDocument();
    // Only effects (and rows, timing tracks) fold.
    expect(within(card).queryByText("Twinkle 13")).not.toBeInTheDocument();
    expect(within(card).getAllByRole("button", { name: /Show all/ })).toHaveLength(1);
    expect(within(card).getByRole("button", { name: "Show all 20" })).toBeInTheDocument();
  });

  it("shows a few details, then all of them on request, and always the warnings", async () => {
    const { user } = await start();
    render(<ProposalCard proposal={proposal([falcon])} current />);
    const card = screen.getByRole("region", { name: "Proposed changes" });
    // Nothing was locked to the music (a show change): no line about it.
    expect(within(card).queryByText(/to the music/)).not.toBeInTheDocument();
    expect(within(card).getByText("Sends light data to a new address: 203.0.113.9")).toBeInTheDocument();
    expect(within(card).getByText("address: 203.0.113.9")).toBeInTheDocument();
    expect(within(card).queryByText("port 5: Star")).not.toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Show 2 more" }));
    expect(within(card).getByText("port 5: Star")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Show fewer" }));
    expect(within(card).queryByText("port 5: Star")).not.toBeInTheDocument();
  });

  it("warns when lights are running and the proposal changes controllers", async () => {
    const { backend } = await start();
    backend.output = { ...backend.output, running: true };
    render(<ProposalCard proposal={proposal([falcon])} current />);
    expect(await screen.findByText(/Your lights are running/)).toBeInTheDocument();
  });

  it("drops a proposal when another show is opened", async () => {
    const { user } = await start();
    const panel = await openPanel(user);
    await user.click(within(panel).getByRole("button", { name: "Add two arches beside the garage" }));
    const card = await within(panel).findByRole("region", { name: "Proposed changes" });
    await act(() => useApp.getState().newShow());
    expect(await within(card).findByText("A different show is open now, so this suggestion was dropped.")).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Apply" })).not.toBeInTheDocument();
  });
});

describe("the preview frame for a very large show", () => {
  it("doesn't run out of arguments", () => {
    const props = Array.from({ length: 300_000 }, (_, i) => ({ prop: `p${i}`, frameOffset: i * 3, channelsPerPixel: 3, points: [0, 0] }));
    expect(highlightFrame(props, []).length).toBe(900_000);
  });
});

describe("the preview frame", () => {
  it("highlights changed props and dims the rest", () => {
    const frame = highlightFrame(
      [
        { prop: "a", frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 1, 0] },
        { prop: "b", frameOffset: 6, channelsPerPixel: 4, points: [0, 0] },
      ],
      ["b"],
    );
    expect([...frame]).toEqual([55, 55, 66, 55, 55, 66, 255, 176, 32, 0]);
  });
});
