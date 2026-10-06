// The assistant panel's state: the chat as shown, the proposal under review, and which provider
// and model to use. The chat itself (and the draft) live in the app; API keys never pass through
// here (Settings sends a typed key straight to the app and forgets it).

import { create } from "zustand";
import { AssistantError, type AssistantApi, type ProposalView, type ProviderId, providerName } from "../api/assistant";
import { errorMessage } from "../api/backend";
import { rowsForShow } from "../api/sequence";
import type { PreviewSet } from "../api/types";
import { fileName } from "../lib/format";
import { useLayoutEditor } from "./layoutEditor";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";

const SETTINGS_KEY = "pixelflow.ai";

/** What the chat says for the user once the song they chose has a new sequence. */
export const SONG_CHOSEN_MESSAGE = "I chose a song, and the new sequence is open. Go ahead.";

/** Provider and model per provider: not secrets, so kept in local storage. */
interface SavedSettings {
  provider: ProviderId;
  models: Partial<Record<ProviderId, string>>;
}

function loadSettings(): SavedSettings {
  try {
    const saved = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") as Partial<SavedSettings>;
    const provider = saved.provider === "openai" ? "openai" : "anthropic";
    const models: SavedSettings["models"] = {};
    for (const id of ["anthropic", "openai"] as const) {
      const model = saved.models?.[id];
      if (typeof model === "string" && model.length > 0 && model.length <= 200) models[id] = model;
    }
    return { provider, models };
  } catch {
    return { provider: "anthropic", models: {} };
  }
}

function saveSettings(settings: SavedSettings) {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    // Storage unavailable; the choice still holds for this session.
  }
}

/** Where the chat's Choose a song button is: waiting, the picker or new sequence on its way, or done. */
export interface SongChoice {
  status: "open" | "picking" | "done";
  /** The new sequence's name, once made. */
  name?: string;
}

/** One line of the chat as shown. */
export interface ChatItem {
  id: number;
  /** "proposal" marks where a proposal card sits in the chat; "chooseSong" where the Choose a song button does. */
  role: "user" | "assistant" | "error" | "proposal" | "chooseSong";
  text: string;
  proposalId?: string;
  /** For an error: the provider's own words, shown under "Details". */
  details?: string;
  /** For "chooseSong": how far the user got. */
  song?: SongChoice;
}

/** "dropped": the show (or sequence) it was made for was replaced, so it no longer applies. */
export type ProposalStatus = "open" | "applied" | "discarded" | "dropped";

interface AssistantState {
  api: AssistantApi | null;
  open: boolean;
  settingsOpen: boolean;
  provider: ProviderId;
  /** The model picked for each provider. */
  models: Partial<Record<ProviderId, string>>;
  /** Whether the current provider has a key (null until checked). */
  hasKey: boolean | null;
  items: ChatItem[];
  streaming: boolean;
  /** What the assistant is doing right now ("Looking at your props"). */
  activity: string | null;
  proposal: ProposalView | null;
  proposalStatus: ProposalStatus;
  /** The draft's pixels while previewing it. */
  preview: PreviewSet | null;
  busy: boolean;

  connect(api: AssistantApi): Promise<void>;
  setOpen(open: boolean): void;
  toggle(): void;
  setSettingsOpen(open: boolean): void;
  setProvider(provider: ProviderId): Promise<void>;
  setModel(model: string): void;
  /** Checks whether the current provider has a key (after Settings changes it). */
  refreshKey(): Promise<void>;
  send(text: string): Promise<void>;
  /** The chat's Choose a song button: the song picker, then a new unsaved sequence from it (a row
   * per prop and group, asking first about unsaved changes to the open one), then the assistant
   * carries on. The assistant itself never opens files. */
  chooseSong(itemId: number): Promise<void>;
  stop(): Promise<void>;
  newChat(): Promise<void>;
  apply(): Promise<boolean>;
  discard(): Promise<void>;
  showPreview(): Promise<void>;
  hidePreview(): void;
}

let nextItem = 1;

const saved = loadSettings();

export const useAssistant = create<AssistantState>((set, get) => {
  /** Adds to the last assistant line, or starts one. */
  function appendText(text: string) {
    const items = get().items;
    const last = items[items.length - 1];
    if (last?.role === "assistant") set({ items: [...items.slice(0, -1), { ...last, text: last.text + text }] });
    else set({ items: [...items, { id: nextItem++, role: "assistant", text }] });
  }

  function add(role: ChatItem["role"], text: string, proposalId?: string) {
    set({ items: [...get().items, { id: nextItem++, role, text, proposalId }] });
  }

  function addError(error: unknown) {
    const details = error instanceof AssistantError && error.details ? error.details : undefined;
    set({ items: [...get().items, { id: nextItem++, role: "error", text: errorMessage(error), details }] });
  }

  /** What the user is looking at, for the assistant. */
  function context() {
    const app = useApp.getState();
    const sequencer = useSequencer.getState();
    const onSequence = app.screen === "sequence" && sequencer.doc !== null;
    return {
      screen: app.screen,
      selectedProps: useLayoutEditor.getState().selected,
      selectedEffects: onSequence ? sequencer.selection : [],
      playheadMs: onSequence ? sequencer.playheadMs : null,
    };
  }

  return {
    api: null,
    open: false,
    settingsOpen: false,
    provider: saved.provider,
    models: saved.models,
    hasKey: null,
    items: [],
    streaming: false,
    activity: null,
    proposal: null,
    proposalStatus: "open",
    preview: null,
    busy: false,

    async connect(api) {
      set({ api });
      await get().refreshKey();
    },

    setOpen: (open) => set({ open }),
    toggle: () => set({ open: !get().open }),
    setSettingsOpen: (settingsOpen) => set({ settingsOpen }),

    async setProvider(provider) {
      set({ provider, hasKey: null });
      saveSettings({ provider, models: get().models });
      await get().refreshKey();
    },

    setModel(model) {
      const models = { ...get().models, [get().provider]: model };
      set({ models });
      saveSettings({ provider: get().provider, models });
    },

    async refreshKey() {
      const { api, provider } = get();
      if (!api) return;
      try {
        const hasKey = await api.hasApiKey(provider);
        if (get().provider === provider) set({ hasKey });
      } catch (e) {
        set({ hasKey: false });
        add("error", errorMessage(e));
      }
    },

    async send(text) {
      const { api, provider, models, streaming } = get();
      const message = text.trim();
      if (!api || streaming || !message) return;
      const model = models[provider];
      add("user", message);
      if (!model) {
        add("error", `Pick a ${providerName(provider)} model in Settings → AI first.`);
        return;
      }
      set({ streaming: true, activity: "Thinking" });
      let streamed = false;
      let askedForSong = false;
      const offerSong = () => {
        if (askedForSong) return;
        askedForSong = true;
        set({ items: [...get().items, { id: nextItem++, role: "chooseSong", text: "", song: { status: "open" } }] });
      };
      try {
        const reply = await api.send(provider, model, message, context(), (event) => {
          switch (event.kind) {
            case "text":
              streamed = true;
              set({ activity: null });
              appendText(event.text);
              break;
            case "activity":
              set({ activity: event.label });
              break;
            case "retrying":
              set({ activity: `${providerName(provider)} is busy; trying again in ${event.seconds} s` });
              break;
            case "proposal":
              set({ proposal: event.proposal, proposalStatus: "open", preview: null });
              add("proposal", "", event.proposal.id);
              break;
            case "chooseSong":
              offerSong();
              break;
          }
        });
        if (reply.chooseSong) offerSong();
        if (reply.proposal && get().proposal?.id !== reply.proposal.id) {
          set({ proposal: reply.proposal, proposalStatus: "open", preview: null });
          add("proposal", "", reply.proposal.id);
        }
        if (reply.text && !streamed) add("assistant", reply.text);
      } catch (e) {
        addError(e);
      } finally {
        set({ streaming: false, activity: null });
      }
    },

    async stop() {
      await get().api?.stop();
    },

    async chooseSong(itemId) {
      const backend = useApp.getState().backend;
      const item = get().items.find((i) => i.id === itemId);
      if (!backend || !item || item.song?.status !== "open" || get().streaming) return;
      const mark = (song: SongChoice) => set({ items: get().items.map((i) => (i.id === itemId ? { ...i, song } : i)) });
      const pick = async () => {
        mark({ status: "picking" });
        try {
          const path = await backend.pickAudioPath();
          if (!path) {
            mark({ status: "open" });
            return;
          }
          const waveform = await backend.audioWaveform(path, 100);
          const show = useApp.getState().snapshot?.show;
          const name = fileName(path).replace(/\.[^.]+$/, "") || "New sequence";
          const made = await useSequencer.getState().newSequence(name, waveform.durationMs, path, show ? rowsForShow(show) : []);
          if (!made) {
            mark({ status: "open" });
            return;
          }
          mark({ status: "done", name });
          // A fixed sentence: the file name, which may come from anywhere, reaches the assistant
          // only as data (the quoted "Open sequence" line of the context block).
          await get().send(SONG_CHOSEN_MESSAGE);
        } catch (e) {
          mark({ status: "open" });
          addError(e);
        }
      };
      // The sequence lives on the Sequence screen; the open one's unsaved changes are asked about first.
      useApp.getState().setScreen("sequence");
      useSequencer.getState().replaceAfterAsking(() => void pick());
    },

    async newChat() {
      const { api, streaming } = get();
      if (!api || streaming) return;
      try {
        await api.newChat();
        set({ items: [], proposal: null, proposalStatus: "open", preview: null });
      } catch (e) {
        add("error", errorMessage(e));
      }
    },

    async apply() {
      const { api, proposal } = get();
      if (!api || !proposal || get().busy) return false;
      set({ busy: true });
      let applied = false;
      // In the show's edit queue, so it lands in order with the user's own edits.
      const ok = await useApp.getState().run(async (backend) => {
        const result = await api.apply(proposal.id);
        applied = true;
        return result.snapshot ?? (await backend.getSnapshot());
      });
      if (applied) {
        set({ proposalStatus: "applied", preview: null });
        if (proposal.changesSequence) {
          await useSequencer.getState().refreshIssues();
          // The draft found the beats itself: the screen needn't offer to.
          const beats = proposal.diff.changes.some((c) => c.section === "timingTrack" && c.action === "added" && c.name === "Beats");
          if (beats) useSequencer.setState({ suggestBeats: false });
        }
      } else if (!ok) {
        add("error", useApp.getState().error ?? "The proposal couldn't be applied.");
        useApp.setState({ error: null });
      }
      set({ busy: false });
      return applied;
    },

    async discard() {
      const { api, proposal } = get();
      if (!api || !proposal) return;
      try {
        await api.discard(proposal.id);
        set({ proposalStatus: "discarded", preview: null });
      } catch (e) {
        add("error", errorMessage(e));
      }
    },

    async showPreview() {
      const { api, proposal } = get();
      if (!api || !proposal) return;
      try {
        set({ preview: await api.preview(proposal.id) });
      } catch (e) {
        add("error", errorMessage(e));
      }
    },

    hidePreview: () => set({ preview: null }),
  };
});

/** When the show or the open sequence is replaced, an open proposal made for the old one is dropped. */
function dropStaleProposal() {
  const { api, proposal, proposalStatus, streaming } = useAssistant.getState();
  if (!api || !proposal || proposalStatus !== "open" || streaming) return;
  void api.sync().then(
    (dropped) => {
      if (dropped && useAssistant.getState().proposal?.id === proposal.id) {
        useAssistant.setState({ proposalStatus: "dropped", preview: null });
      }
    },
    () => undefined,
  );
}

useApp.subscribe((state, before) => {
  if (state.snapshot !== before.snapshot) dropStaleProposal();
});
useSequencer.subscribe((state, before) => {
  if (state.docKey !== before.docKey) dropStaleProposal();
});
