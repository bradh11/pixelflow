import type {
  Applied,
  AssistantApi,
  Change,
  ChatEvent,
  KeyLocation,
  KeyStorage,
  ModelInfo,
  ProposalView,
  ProviderId,
  TurnReply,
  UiContext,
} from "./assistant";
import { providerName } from "./assistant";
import { MemoryBackend } from "./memory";
import type { Edit, PreviewSet, Show } from "./types";
import { besideOthers } from "../lib/layoutEdits";
import { type PropKind, newProp } from "../lib/shows";

const MODELS: Record<ProviderId, ModelInfo[]> = {
  anthropic: [
    { id: "claude-opus-5-5", name: "Claude Opus 5.5", recommended: true },
    { id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5", recommended: false },
    { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", recommended: false },
  ],
  openai: [
    { id: "gpt-5.1", name: "gpt-5.1", recommended: true },
    { id: "gpt-5.1-mini", name: "gpt-5.1-mini", recommended: false },
  ],
};

const KINDS: { words: RegExp; kind: PropKind; label: string }[] = [
  { words: /\barch(es)?\b/i, kind: "arch", label: "arch" },
  { words: /\b(mega ?)?trees?\b/i, kind: "tree", label: "mega tree" },
  { words: /\bstars?\b/i, kind: "star", label: "star" },
  { words: /\bmatri(x|ces)\b/i, kind: "matrix", label: "matrix" },
  { words: /\b(wreaths?|circles?)\b/i, kind: "circle", label: "wreath" },
];

const NUMBERS: Record<string, number> = { a: 1, an: 1, one: 1, two: 2, three: 3, four: 4, five: 5, six: 6 };

/** How many the message asks for ("two arches", "3 stars"), between 1 and 6. */
function countIn(message: string): number {
  const match = /\b(\d+|a|an|one|two|three|four|five|six)\s+(mega |big |small )?\w+/i.exec(message);
  const n = match ? (NUMBERS[match[1].toLowerCase()] ?? Number(match[1])) : 1;
  return Math.max(1, Math.min(6, Number.isFinite(n) ? n : 1));
}

interface Pending {
  proposal: ProposalView;
  edits: Edit[];
  draft: Show;
}

/**
 * A stand-in assistant for the browser (`?demo`) and tests: keys are only remembered as "there is
 * one" (the text is dropped), models are a fixed list, and replies follow a tiny script: asking to
 * add arches, trees, stars, matrices, or wreaths drafts them (with a group); asking to rename the
 * show drafts that; anything else gets a summary of the show. Apply goes through the memory
 * backend as one undo step.
 */
export class FakeAssistant implements AssistantApi {
  keys = new Map<ProviderId, KeyLocation>();
  storage: KeyStorage = { name: "Keychain", available: true };
  /** The next send rejects with this (to show provider errors). */
  nextError: string | null = null;
  /** Milliseconds between streamed words (0 in tests). */
  delayMs = 0;
  calls: string[] = [];
  private pending: Pending | null = null;
  private stopped = false;

  constructor(private backend: MemoryBackend) {}

  async keyStorage() {
    return this.storage;
  }

  private checkKey(text: string) {
    if (text.trim().length < 8 || /\s/.test(text.trim())) throw new Error("An API key is one line of letters, digits, and dashes. Paste just the key.");
  }

  async setApiKey(provider: ProviderId, key: string) {
    this.checkKey(key);
    if (!this.storage.available) {
      throw new Error(`This computer has no ${this.storage.name} PixelFlow can use, so the key can't be saved. You can use it for this session only.`);
    }
    this.keys.set(provider, "keychain");
    return "keychain" as const;
  }

  async useKeyForSession(provider: ProviderId, key: string) {
    this.checkKey(key);
    this.keys.set(provider, "session");
    return "session" as const;
  }

  async hasApiKey(provider: ProviderId) {
    return this.keys.has(provider);
  }

  async keyLocation(provider: ProviderId) {
    return this.keys.get(provider) ?? null;
  }

  async deleteApiKey(provider: ProviderId) {
    this.keys.delete(provider);
  }

  async listModels(provider: ProviderId) {
    if (!this.keys.has(provider)) throw new Error(`Add your ${providerName(provider)} API key in Settings → AI first.`);
    return MODELS[provider];
  }

  private async stream(text: string, onEvent: (event: ChatEvent) => void) {
    for (const word of text.split(/(?<= )/)) {
      if (this.stopped) throw new Error("Stopped.");
      if (this.delayMs > 0) await new Promise((r) => setTimeout(r, this.delayMs));
      onEvent({ kind: "text", text: word });
    }
  }

  async send(provider: ProviderId, model: string, message: string, context: UiContext, onEvent: (event: ChatEvent) => void): Promise<TurnReply> {
    this.calls.push(`send:${model}`);
    this.stopped = false;
    if (!this.keys.has(provider)) throw new Error(`Add your ${providerName(provider)} API key in Settings → AI first.`);
    if (!model) throw new Error("Pick a model in Settings → AI first.");
    if (this.nextError) {
      const error = this.nextError;
      this.nextError = null;
      throw new Error(error);
    }
    // In the demo, "simulate a rate limit" (or a network error, or a bad key) shows that error.
    const simulated = /\bsimulate (?:an? )?(rate limit|network error|bad key)\b/i.exec(message)?.[1].toLowerCase();
    if (simulated) {
      const name = providerName(provider);
      throw new Error(
        simulated === "rate limit"
          ? `${name} is limiting how fast this key can send requests. Wait a minute, then try again.`
          : simulated === "network error"
            ? `Couldn't reach ${name}. Check your internet connection, then try again.`
            : `${name} didn't accept your API key. It may be mistyped or revoked: paste it again in Settings → AI.`,
      );
    }
    const show = this.backend.show;
    const kind = KINDS.find((k) => k.words.test(message));
    const rename = /\b(rename|call)\b.*?\b(show|it)\b(?:\s+(?:to|as))?\s+["“]?([^"”]+?)["”]?\s*$/i.exec(message);
    if (kind && /\b(add|put|place|make|draw|want)\b/i.test(message)) {
      onEvent({ kind: "activity", label: "Looking at your props" });
      const count = countIn(message);
      const plural = kind.label.endsWith("ch") ? `${kind.label}es` : kind.label.endsWith("x") ? kind.label.replace(/x$/, "ces") : `${kind.label}s`;
      const draft = structuredClone(show);
      const edits: Edit[] = [];
      for (let i = 0; i < count; i++) {
        const prop = besideOthers(newProp(kind.kind, draft), draft);
        draft.props.push(prop);
        edits.push({ type: "addProp", prop });
      }
      onEvent({ kind: "activity", label: "Drafting: add prop" });
      const added = edits.flatMap((e) => (e.type === "addProp" ? [e.prop] : []));
      const changes: Change[] = added.map((p) => ({ section: "prop", action: "added", name: p.name, id: p.id, details: [] }));
      if (count > 1) {
        const group = { id: crypto.randomUUID(), name: `${plural[0].toUpperCase()}${plural.slice(1)}`, members: added.map((p) => p.id) };
        draft.groups.push(group);
        edits.push({ type: "addGroup", group });
        changes.push({ section: "group", action: "added", name: group.name, id: group.id, details: [] });
        onEvent({ kind: "activity", label: "Drafting: add group" });
      }
      const words = count > 1 ? `${count} ${plural}` : `a ${kind.label}`;
      const summary = `Adds ${words} beside your other props${count > 1 ? ", grouped so you can sequence them together" : ""}.`;
      return this.propose(summary, changes, edits, draft, onEvent, `I drafted ${words}. Have a look at the preview, then Apply or Discard.`);
    }
    if (rename && rename[3].trim()) {
      const name = rename[3].trim();
      const draft = { ...structuredClone(show), name };
      const changes: Change[] = [{ section: "show", action: "changed", name, id: null, details: [`name: "${show.name}" → "${name}"`] }];
      return this.propose(`Renames the show to "${name}".`, changes, [{ type: "renameShow", name }], draft, onEvent, "Ready when you are.");
    }
    onEvent({ kind: "activity", label: "Looking at your show" });
    const selected = context.selectedProps.map((id) => show.props.find((p) => p.id === id)?.name).filter(Boolean);
    const text =
      `Your show "${show.name}" has ${show.props.length} props, ${show.groups.length} groups, and ${show.controllers.length} controllers.` +
      (selected.length > 0 ? ` You have ${selected.join(", ")} selected.` : "") +
      " Ask me to add or change something and I'll draft it for you to review.";
    await this.stream(text, onEvent);
    return { text, proposal: null };
  }

  private async propose(summary: string, changes: Change[], edits: Edit[], draft: Show, onEvent: (event: ChatEvent) => void, closing: string): Promise<TurnReply> {
    onEvent({ kind: "activity", label: "Preparing the proposal" });
    const proposal: ProposalView = {
      id: crypto.randomUUID(),
      summary,
      diff: { changes },
      changedProps: changes.filter((c) => c.section === "prop" && c.action !== "removed").flatMap((c) => (c.id ? [c.id] : [])),
      changesShow: true,
      changesSequence: false,
    };
    this.pending = { proposal, edits, draft };
    onEvent({ kind: "proposal", proposal });
    await this.stream(closing, onEvent);
    return { text: closing, proposal };
  }

  async stop() {
    this.stopped = true;
  }

  async newChat() {
    this.pending = null;
  }

  private current(id: string): Pending {
    if (!this.pending || this.pending.proposal.id !== id) throw new Error("That proposal isn't the latest one anymore.");
    return this.pending;
  }

  async apply(id: string): Promise<Applied> {
    const { edits } = this.current(id);
    this.calls.push("apply");
    const snapshot = await this.backend.applyEdits(edits);
    this.pending = null;
    return { snapshot, sequence: null };
  }

  async discard(id: string) {
    this.current(id);
    this.pending = null;
  }

  async preview(id: string): Promise<PreviewSet> {
    const { draft } = this.current(id);
    return new MemoryBackend(draft).previewProps();
  }
}
