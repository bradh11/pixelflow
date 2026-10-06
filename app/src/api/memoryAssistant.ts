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
import { AssistantError, providerName } from "./assistant";
import { MemoryBackend } from "./memory";
import { composeDemoSequence } from "./demoComposer";
import { renderSequenceFrame } from "./memoryRender";
import type { MemorySequencer } from "./memorySequencer";
import type { Sequence, SequenceEdit } from "./sequence";
import type { Edit, PreviewSet, Prop, Show } from "./types";
import { besideOthers } from "../lib/layoutEdits";
import { type PropKind, newProp, nodeCount } from "../lib/shows";

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

/** An added prop described like the app's review card does. */
function propDetails(prop: Prop): string[] {
  const kind = prop.shape.source === "generator" ? prop.shape.type : "measured points";
  const round = (n: number) => Math.round(n * 1000) / 1000;
  const p = prop.transform.position;
  return [`kind: ${kind}`, `pixels: ${nodeCount(prop.shape)}`, `position: x ${round(p.x)}, y ${round(p.y)}, z ${round(p.z)}`];
}

interface Pending {
  proposal: ProposalView;
  edits: Edit[];
  draft: Show;
  /** The show it was made for (see MemoryBackend.generation). */
  generation: number;
  /** A sequence proposal's edits and draft sequence. */
  sequenceEdits?: SequenceEdit[];
  draftSequence?: Sequence;
}

/** "Create a compelling sequence", "make me a new sequence", "build a light show". */
const CREATE_SEQUENCE = /\b(create|make|build|design|write|do)\b.*\b(sequence|light show|show to)\b|\bcompelling\b/i;
/** The app's message after the user picked a song for the new sequence. */
const SONG_CHOSEN = /\bI chose\b/i;

/**
 * A stand-in assistant for the browser (`?demo`) and tests: keys are only remembered as "there is
 * one" (the text is dropped), models are a fixed list, and replies follow a tiny script: asking to
 * add arches, trees, stars, matrices, or wreaths drafts them (with a group); asking to rename the
 * show drafts that; asking to create a sequence offers the song picker (when no empty sequence
 * is open), then analyzes the song and proposes a whole sequence for it; anything else gets a
 * summary of the show. Apply goes through the memory backend (or sequencer) as one undo step.
 */
export class FakeAssistant implements AssistantApi {
  keys = new Map<ProviderId, KeyLocation>();
  storage: KeyStorage = { name: "Keychain", available: true };
  /** The next send rejects with this (to show provider errors). */
  nextError: string | AssistantError | null = null;
  /** Milliseconds between streamed words (0 in tests). */
  delayMs = 0;
  calls: string[] = [];
  /** Every message sent, as the assistant received it. */
  sent: string[] = [];
  /** The sequencer, for sequence proposals (none: the assistant only drafts show changes). */
  sequencer: MemorySequencer | null = null;
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
    this.sent.push(message);
    this.stopped = false;
    if (!this.keys.has(provider)) throw new Error(`Add your ${providerName(provider)} API key in Settings → AI first.`);
    if (!model) throw new Error("Pick a model in Settings → AI first.");
    if (this.nextError) {
      const error = this.nextError;
      this.nextError = null;
      throw typeof error === "string" ? new Error(error) : error;
    }
    // In the demo, "simulate a rate limit" (or a network error, or a bad key) shows that error.
    const simulated = /\bsimulate (?:an? )?(rate limit|network error|bad key)\b/i.exec(message)?.[1].toLowerCase();
    if (simulated) {
      const name = providerName(provider);
      throw simulated === "rate limit"
        ? new AssistantError(
            `${name} is limiting how fast this key can send requests. Wait a minute, then try again.`,
            "HTTP 429 rate_limit_exceeded: Rate limit reached for requests. (demo)",
          )
        : simulated === "network error"
          ? new AssistantError(`Couldn't reach ${name}. Check your internet connection, then try again.`)
          : new AssistantError(
              `${name} didn't accept your API key. It may be mistyped or revoked: paste it again in Settings → AI.`,
              "HTTP 401 invalid_api_key: Incorrect API key provided: [a key]. (demo)",
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
      const changes: Change[] = added.map((p) => ({ section: "prop", action: "added", name: p.name, id: p.id, details: propDetails(p), warnings: [] }));
      if (count > 1) {
        const group = { id: crypto.randomUUID(), name: `${plural[0].toUpperCase()}${plural.slice(1)}`, members: added.map((p) => p.id) };
        draft.groups.push(group);
        edits.push({ type: "addGroup", group });
        changes.push({ section: "group", action: "added", name: group.name, id: group.id, details: [`members: ${added.map((p) => p.name).join(", ")}`], warnings: [] });
        onEvent({ kind: "activity", label: "Drafting: add group" });
      }
      const words = count > 1 ? `${count} ${plural}` : `a ${kind.label}`;
      const summary = `Adds ${words} beside your other props${count > 1 ? ", grouped so you can sequence them together" : ""}.`;
      return this.propose(summary, changes, edits, draft, onEvent, `I drafted ${words}. Have a look at the preview, then Apply or Discard.`);
    }
    if (rename && rename[3].trim()) {
      const name = rename[3].trim();
      const draft = { ...structuredClone(show), name };
      const changes: Change[] = [{ section: "show", action: "changed", name, id: null, details: [`name: "${show.name}" → "${name}"`], warnings: [] }];
      return this.propose(`Renames the show to "${name}".`, changes, [{ type: "renameShow", name }], draft, onEvent, "Ready when you are.");
    }
    const sequence = this.sequencer?.doc ?? null;
    const empty = sequence !== null && sequence.rows.every((r) => r.layers.every((l) => l.effects.length === 0));
    if (CREATE_SEQUENCE.test(message) || (SONG_CHOSEN.test(message) && sequence)) {
      if (!sequence || (!empty && !SONG_CHOSEN.test(message))) {
        const ask = sequence
          ? "Your open sequence already has effects, so let's start a new one. Choose a song and I'll build a light show to it."
          : "You don't have a sequence open yet. Choose a song and I'll build a light show to it.";
        onEvent({ kind: "activity", label: "Asking for a song" });
        await this.stream(ask, onEvent);
        onEvent({ kind: "chooseSong" });
        return { text: ask, proposal: null, chooseSong: true };
      }
      return this.composeSequence(sequence, onEvent);
    }
    onEvent({ kind: "activity", label: "Looking at your show" });
    const selected = context.selectedProps.map((id) => show.props.find((p) => p.id === id)?.name).filter(Boolean);
    const text =
      `Your show "${show.name}" has ${show.props.length} props, ${show.groups.length} groups, and ${show.controllers.length} controllers.` +
      (selected.length > 0 ? ` You have ${selected.join(", ")} selected.` : "") +
      " Ask me to add or change something and I'll draft it for you to review.";
    await this.stream(text, onEvent);
    return { text, proposal: null, chooseSong: false };
  }

  private async pause(ms: number) {
    if (this.delayMs > 0) await new Promise((r) => setTimeout(r, ms));
    if (this.stopped) throw new Error("Stopped.");
  }

  /** The scripted sequence: listen to the song, add its timing, place effects, propose. */
  private async composeSequence(doc: Sequence, onEvent: (event: ChatEvent) => void): Promise<TurnReply> {
    const sequencer = this.sequencer!;
    onEvent({ kind: "activity", label: "Listening to the song" });
    await this.pause(900);
    const analysis = await sequencer.analyzeAudio(doc.audio ?? "");
    onEvent({ kind: "activity", label: "Looking at your props" });
    await this.pause(400);
    onEvent({ kind: "activity", label: "Drafting: song timing" });
    await this.pause(400);
    const composed = composeDemoSequence(doc, this.backend.show, analysis);
    for (const section of composed.sections) {
      onEvent({ kind: "activity", label: `Drafting: place effects (${section.label})` });
      await this.pause(350);
    }
    onEvent({ kind: "activity", label: "Checking the draft" });
    await this.pause(300);
    onEvent({ kind: "activity", label: "Preparing the proposal" });
    const proposal: ProposalView = {
      id: crypto.randomUUID(),
      summary: composed.summary,
      diff: { changes: composed.changes },
      changedProps: [],
      changesShow: false,
      changesSequence: true,
      sections: composed.sections,
      timeline: composed.timeline,
    };
    this.pending = {
      proposal,
      edits: [],
      draft: this.backend.show,
      generation: this.backend.generation,
      sequenceEdits: composed.edits,
      draftSequence: composed.draft,
    };
    onEvent({ kind: "proposal", proposal });
    const closing = "Here's a first pass. Play the preview, then Apply or Discard.";
    await this.stream(closing, onEvent);
    return { text: closing, proposal, chooseSong: false };
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
      sections: [],
      timeline: null,
    };
    this.pending = { proposal, edits, draft, generation: this.backend.generation };
    onEvent({ kind: "proposal", proposal });
    await this.stream(closing, onEvent);
    return { text: closing, proposal, chooseSong: false };
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

  async sync() {
    if (this.pending && this.pending.generation !== this.backend.generation) {
      this.pending = null;
      return true;
    }
    return false;
  }

  async apply(id: string): Promise<Applied> {
    const { edits, generation, sequenceEdits } = this.current(id);
    if (generation !== this.backend.generation) {
      throw new Error("A different show is open now, so this suggestion no longer applies. Ask again.");
    }
    this.calls.push("apply");
    if (sequenceEdits) {
      if (!this.sequencer?.doc) throw new Error("The sequence this suggestion changes isn't open anymore. Open it again and ask again.");
      const sequence = await this.sequencer.editSequence(sequenceEdits);
      this.pending = null;
      return { snapshot: null, sequence };
    }
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

  async previewFrame(id: string, positionMs: number): Promise<Uint8Array> {
    const { draft, draftSequence } = this.current(id);
    if (!draftSequence) throw new Error("This suggestion doesn't change the sequence.");
    return renderSequenceFrame(draftSequence, draft, positionMs);
  }
}
