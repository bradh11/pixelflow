import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../../App";
import { demoShow } from "../../api/demo";
import { MemoryBackend } from "../../api/memory";
import { FakeAssistant } from "../../api/memoryAssistant";
import { useAssistant } from "../../state/assistant";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { type Change, type ProposalView, modelLabel } from "../../api/assistant";
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

  it("floating, Escape puts it away", async () => {
    window.innerWidth = 1024;
    const { user } = await start();
    await openPanel(user);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("complementary", { name: "Assistant" })).not.toBeInTheDocument();
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

describe("the review card", () => {
  const proposal = (changes: Change[]): ProposalView => ({
    id: "p1",
    summary: "Adds a controller.",
    diff: { changes },
    changedProps: [],
    changesShow: true,
    changesSequence: false,
  });
  const falcon: Change = {
    section: "controller",
    action: "added",
    name: "Falcon 2",
    id: "c1",
    details: ["address: 203.0.113.9", "protocol: DDP", "port 1: Roofline", "port 2: Arch 1", "port 3: Arch 2", "port 4: Tree", "port 5: Star"],
    warnings: ["Sends light data to a new address: 203.0.113.9"],
  };

  it("shows a few details, then all of them on request, and always the warnings", async () => {
    const { user } = await start();
    render(<ProposalCard proposal={proposal([falcon])} current />);
    const card = screen.getByRole("region", { name: "Proposed changes" });
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
