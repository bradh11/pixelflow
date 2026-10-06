import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { decodePreview } from "./previewBytes";
import type { SequenceEditResult } from "./sequence";
import type { PreviewSet, ShowSnapshot } from "./types";

/** The AI companies a user can bring a key for. */
export type ProviderId = "anthropic" | "openai";

export const PROVIDERS: { id: ProviderId; label: string; keyHint: string; keySite: string }[] = [
  { id: "anthropic", label: "Anthropic (Claude)", keyHint: "sk-ant-…", keySite: "console.anthropic.com" },
  { id: "openai", label: "OpenAI (GPT)", keyHint: "sk-…", keySite: "platform.openai.com" },
];

export function providerName(id: ProviderId): string {
  return id === "anthropic" ? "Anthropic" : "OpenAI";
}

/** A chat model that can use tools. */
export interface ModelInfo {
  id: string;
  name: string;
  /** The one PixelFlow suggests. */
  recommended: boolean;
}

/** Where a key is kept: the OS credential store, or memory until PixelFlow quits. */
export type KeyLocation = "keychain" | "session";

/** What this computer calls its credential store, and whether there is one. */
export interface KeyStorage {
  name: string;
  available: boolean;
}

/** What the user is looking at, sent with each message. */
export interface UiContext {
  screen?: string;
  selectedProps: string[];
  selectedEffects: string[];
  playheadMs?: number | null;
}

export type DiffSection = "show" | "prop" | "group" | "controller" | "playlist" | "sequence" | "row" | "effect" | "timingTrack";
export type DiffAction = "added" | "removed" | "changed";

/** One line of a proposal's list of changes. */
export interface Change {
  section: DiffSection;
  action: DiffAction;
  name: string;
  id: string | null;
  details: string[];
  /** What deserves a careful look: where light data goes, files the assistant chose. */
  warnings: string[];
}

/** The assistant's finished draft, for the user to review. */
export interface ProposalView {
  id: string;
  summary: string;
  diff: { changes: Change[] };
  /** Props added or changed (highlighted in the preview). */
  changedProps: string[];
  changesShow: boolean;
  changesSequence: boolean;
}

/** What arrives while a reply streams in. */
export type ChatEvent =
  | { kind: "text"; text: string }
  | { kind: "activity"; label: string }
  | { kind: "retrying"; seconds: number }
  | { kind: "proposal"; proposal: ProposalView };

export interface TurnReply {
  text: string;
  proposal: ProposalView | null;
}

/** What applying did: the show (one undo step) and/or the open sequence (one sequence undo step). */
export interface Applied {
  snapshot: ShowSnapshot | null;
  sequence: SequenceEditResult | null;
}

/**
 * Everything the assistant asks of the app. Keys only ever go in: nothing returns one, and every
 * call to a provider is made by the app (in Rust), never from this window. Errors reject with a
 * plain-language message (an {@link AssistantError} with the provider's own words, when there are any).
 */
export interface AssistantApi {
  keyStorage(): Promise<KeyStorage>;
  /** Saves the key in the OS credential store. Rejects when there is none (offer the session). */
  setApiKey(provider: ProviderId, key: string): Promise<KeyLocation>;
  /** Keeps the key in memory until PixelFlow quits. */
  useKeyForSession(provider: ProviderId, key: string): Promise<KeyLocation>;
  hasApiKey(provider: ProviderId): Promise<boolean>;
  keyLocation(provider: ProviderId): Promise<KeyLocation | null>;
  deleteApiKey(provider: ProviderId): Promise<void>;
  /** The provider's models that can chat and use tools, live, best first. */
  listModels(provider: ProviderId): Promise<ModelInfo[]>;
  /** Sends a message; the reply streams to `onEvent`. Nothing in the show changes. */
  send(provider: ProviderId, model: string, message: string, context: UiContext, onEvent: (event: ChatEvent) => void): Promise<TurnReply>;
  /** Stops the reply in progress (the send rejects with "Stopped."). */
  stop(): Promise<void>;
  newChat(): Promise<void>;
  /** Applies the proposal as one undo step. Only from the user's Apply. */
  apply(id: string): Promise<Applied>;
  discard(id: string): Promise<void>;
  /** The draft show's pixels (front view), to preview without applying. */
  preview(id: string): Promise<PreviewSet>;
  /** Drops the proposal when the show or sequence it was made for isn't open anymore (call after
   * either is replaced). True when it was dropped. */
  sync(): Promise<boolean>;
}

/** A failed chat turn or model list: the plain message, and the provider's own words under "Details". */
export class AssistantError extends Error {
  constructor(
    message: string,
    readonly details: string | null = null,
  ) {
    super(message);
    this.name = "AssistantError";
  }
}

/** The app's `{ message, details }` failure as an {@link AssistantError}; anything else as it came. */
export function assistantFailure(error: unknown): unknown {
  if (error !== null && typeof error === "object" && !(error instanceof Error) && "message" in error && typeof error.message === "string") {
    const details = "details" in error && typeof error.details === "string" ? error.details : null;
    return new AssistantError(error.message, details);
  }
  return error;
}

async function failing<T>(call: Promise<T>): Promise<T> {
  try {
    return await call;
  } catch (e) {
    throw assistantFailure(e);
  }
}

/** The event the app streams replies on. */
export const ASSISTANT_EVENT = "assistant-event";

/** The assistant in the desktop app. */
export const tauriAssistant: AssistantApi = {
  keyStorage: () => invoke("ai_key_storage"),
  setApiKey: (provider, key) => invoke("set_api_key", { provider, key }),
  useKeyForSession: (provider, key) => invoke("use_api_key_for_session", { provider, key }),
  hasApiKey: (provider) => invoke("has_api_key", { provider }),
  keyLocation: (provider) => invoke("api_key_location", { provider }),
  deleteApiKey: (provider) => invoke("delete_api_key", { provider }),
  listModels: (provider) => failing(invoke("list_ai_models", { provider })),
  async send(provider, model, message, context, onEvent) {
    // One reply at a time: every event heard while this call is out belongs to it.
    const unlisten = await listen<{ turn: number; event: ChatEvent }>(ASSISTANT_EVENT, (e) => onEvent(e.payload.event));
    try {
      return await failing(invoke<TurnReply>("ai_send", { provider, model, message, context }));
    } finally {
      unlisten();
    }
  },
  stop: () => invoke("ai_stop"),
  newChat: () => invoke("ai_new_chat"),
  apply: (id) => invoke("ai_apply", { id }),
  discard: (id) => invoke("ai_discard", { id }),
  preview: async (id) => decodePreview(await invoke<ArrayBuffer | number[]>("ai_preview", { id })),
  sync: () => invoke("ai_sync"),
};
