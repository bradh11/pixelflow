import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import type { ProviderId } from "../../api/assistant";
import { demoShow } from "../../api/demo";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "../../api/demoSequence";
import { MemoryBackend } from "../../api/memory";
import { MemorySequencer } from "../../api/memorySequencer";
import { useAssistant } from "../../state/assistant";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { SEND_AUDIO_QUESTION } from "./FindLyrics";

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    width: 1000,
    height: 600,
    right: 1000,
    bottom: 600,
    toJSON: () => ({}),
  });
  useAssistant.setState({ provider: "anthropic", lyricsAudioOk: false, lyricsLanguage: "en", settingsOpen: false });
});

afterEach(() => {
  vi.restoreAllMocks();
});

/** The Sequence screen on the demo sequence (with music); `keys`: providers the assistant has keys for. */
async function openScreen(keys: ProviderId[] = []) {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  backend.nextAudioPath = DEMO_MUSIC;
  const seq = new MemorySequencer(backend);
  seq.hasAssistantKey = (p) => keys.includes(p);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "sequence" });
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  return { seq, user };
}

const trackNames = () => useSequencer.getState().doc?.timingTracks.map((t) => t.name) ?? [];
const findButton = () => screen.getByRole("button", { name: "Find lyrics" });

describe("Find lyrics", () => {
  it("waits for the assistant to be set up, saying why with a way to its settings", async () => {
    const { seq, user } = await openScreen();
    await waitFor(() => expect(findButton()).toHaveAttribute("title", "Finding lyrics needs the assistant: add your Anthropic key in Settings → AI."));
    expect(findButton()).toHaveAttribute("aria-disabled", "true");
    await user.click(findButton());
    const note = screen.getByRole("note");
    expect(note).toHaveTextContent("add your Anthropic key");
    await user.click(within(note).getByRole("button", { name: "AI settings" }));
    expect(useAssistant.getState().settingsOpen).toBe(true);
    expect(seq.calls.some((c) => c.startsWith("findLyrics"))).toBe(false);
  });

  it("with an Anthropic key adds Lyrics, words, and Vocals and says where they came from", async () => {
    const { seq, user } = await openScreen(["anthropic"]);
    await waitFor(() => expect(findButton()).toHaveAttribute("aria-disabled", "false"));
    await user.click(findButton());
    await waitFor(() => expect(trackNames()).toEqual(expect.arrayContaining(["Lyrics", "Lyrics (words)", "Vocals"])));
    // Anthropic can't hear audio: no question, and nothing is sent.
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(seq.calls).toContain("findLyrics:anthropic:false");
    expect(await screen.findByText(/Found 4 lines and 14 words\. Lyrics and line timing from LRCLIB/)).toBeInTheDocument();
    expect(screen.getByText(/14 words have rough timing/)).toBeInTheDocument();
    // One undo step takes them all away.
    await useSequencer.getState().undo();
    expect(trackNames()).not.toContain("Vocals");
  });

  it("with an OpenAI key asks before sending the audio, and can be told not to ask again", async () => {
    useAssistant.setState({ provider: "openai" });
    const { seq, user } = await openScreen(["openai"]);
    await waitFor(() => expect(findButton()).toHaveAttribute("aria-disabled", "false"));
    await user.click(findButton());
    let dialog = screen.getByRole("alertdialog", { name: "Send the song to OpenAI?" });
    expect(dialog).toHaveTextContent(SEND_AUDIO_QUESTION);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(seq.calls.some((c) => c.startsWith("findLyrics"))).toBe(false);

    await user.click(findButton());
    dialog = screen.getByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Published lyrics only" }));
    await waitFor(() => expect(seq.calls).toContain("findLyrics:openai:false"));
    await waitFor(() => expect(useSequencer.getState().findingLyrics).toBeNull());

    await user.click(findButton());
    dialog = screen.getByRole("alertdialog");
    await user.click(within(dialog).getByRole("checkbox", { name: "Don't ask again" }));
    await user.click(within(dialog).getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(seq.calls).toContain("findLyrics:openai:true"));
    expect(await screen.findByText(/word timing from OpenAI/)).toBeInTheDocument();
    expect(useAssistant.getState().lyricsAudioOk).toBe(true);
    await waitFor(() => expect(useSequencer.getState().findingLyrics).toBeNull());

    // Remembered: no question this time.
    await user.click(findButton());
    expect(screen.queryByRole("alertdialog")).toBeNull();
    await waitFor(() => expect(seq.calls.filter((c) => c === "findLyrics:openai:true")).toHaveLength(2));
  });

  it("shows each step and stops when asked, adding nothing", async () => {
    const { seq, user } = await openScreen(["anthropic"]);
    seq.lyricsStepMs = 40;
    await waitFor(() => expect(findButton()).toHaveAttribute("aria-disabled", "false"));
    await user.click(findButton());
    expect(await screen.findByText(/Looking up published lyrics|Reading the song/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Stop finding lyrics" }));
    await waitFor(() => expect(useSequencer.getState().findingLyrics).toBeNull());
    expect(trackNames()).not.toContain("Vocals");
    expect(useApp.getState().error).toBeNull();
  });

  it("says where the lyrics came from, and lets the user pick other lyrics or paste them", async () => {
    useAssistant.setState({ lyricsLanguage: "fr" });
    const { seq, user } = await openScreen(["anthropic"]);
    await waitFor(() => expect(findButton()).toHaveAttribute("aria-disabled", "false"));
    await user.click(findButton());
    expect(await screen.findByText("Lyrics: Lantern Band — Lantern Song (LRCLIB) · line timing: LRCLIB")).toBeInTheDocument();
    expect(seq.lastLyricsOptions).toEqual({ language: "fr", fresh: false });

    await user.click(screen.getByRole("button", { name: "Wrong song?" }));
    let picker = screen.getByRole("dialog", { name: "Choose the song's lyrics" });
    const options = within(picker).getAllByRole("button", { pressed: false });
    expect(within(picker).getByRole("button", { pressed: true })).toHaveTextContent("Lantern Band — Lantern Song");
    expect(options.map((o) => o.textContent)).toEqual([
      "Lantern Band — Lantern Song (Live)4:01 · English · no line times",
      "Cover Band — Lantern Song3:57 · Russian · timed lines",
    ]);
    await user.click(options[1]);
    expect(await screen.findByText("Lyrics: Cover Band — Lantern Song (LRCLIB) · line timing: LRCLIB")).toBeInTheDocument();
    expect(seq.calls).toContain("chooseLyrics:3");
    const lyrics = () => useSequencer.getState().doc?.timingTracks.find((t) => t.name === "Lyrics");
    expect(lyrics()?.marks[0].label).toBe("Привет молоко");

    // Pasted lyrics instead.
    await user.click(screen.getByRole("button", { name: "Wrong song?" }));
    picker = screen.getByRole("dialog", { name: "Choose the song's lyrics" });
    await user.type(within(picker).getByLabelText(/Or paste the lyrics/), "Candles in the window{enter}Frost upon the glass");
    await user.click(within(picker).getByRole("button", { name: "Use pasted lyrics" }));
    expect(await screen.findByText("Lyrics: pasted · line timing: pasted")).toBeInTheDocument();
    expect(lyrics()?.marks.map((m) => m.label)).toEqual(["Candles in the window", "Frost upon the glass"]);

    // Find again: without what's kept, the same way as before.
    await user.click(screen.getByRole("button", { name: "Wrong song?" }));
    picker = screen.getByRole("dialog", { name: "Choose the song's lyrics" });
    await user.click(within(picker).getByRole("button", { name: "Find again" }));
    await waitFor(() => expect(seq.lastLyricsOptions).toEqual({ language: "fr", fresh: true }));
    expect(seq.calls.filter((c) => c === "findLyrics:anthropic:false")).toHaveLength(2);
  });

  it("finds again on Shift-click", async () => {
    const { seq, user } = await openScreen(["anthropic"]);
    await waitFor(() => expect(findButton()).toHaveAttribute("aria-disabled", "false"));
    await user.keyboard("{Shift>}");
    await user.click(findButton());
    await user.keyboard("{/Shift}");
    await waitFor(() => expect(seq.lastLyricsOptions).toEqual({ language: "en", fresh: true }));
  });
});
