import { KeyRound, RefreshCw, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { type KeyLocation, type KeyStorage, type ModelInfo, PROVIDERS, providerName } from "../../api/assistant";
import { errorMessage } from "../../api/backend";
import { useAssistant } from "../../state/assistant";
import { Button, Field, Select } from "../ui";

/**
 * Settings → AI: the provider, its API key (typed once, sent to the app's credential store, and
 * never shown or read back), and the model, picked from the provider's live list.
 */
export function AiSettings() {
  const open = useAssistant((s) => s.settingsOpen);
  const api = useAssistant((s) => s.api);
  const provider = useAssistant((s) => s.provider);
  const model = useAssistant((s) => s.models[s.provider] ?? null);
  const lyricsAudioOk = useAssistant((s) => s.lyricsAudioOk);
  const { setSettingsOpen, setProvider, setModel, setLyricsAudioOk, refreshKey } = useAssistant.getState();
  const keyRef = useRef<HTMLInputElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [storage, setStorage] = useState<KeyStorage | null>(null);
  const [location, setLocation] = useState<KeyLocation | null>(null);
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  const [loadingModels, setLoadingModels] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** The store refused the key because there is none: offer the session instead. */
  const [offerSession, setOfferSession] = useState(false);
  const [busy, setBusy] = useState(false);
  const [hasTyped, setHasTyped] = useState(false);

  const loadModels = useCallback(async () => {
    if (!api) return;
    setLoadingModels(true);
    setError(null);
    try {
      const list = await api.listModels(provider);
      setModels(list);
      const current = useAssistant.getState().models[provider];
      if (!current || !list.some((m) => m.id === current)) {
        const pick = list.find((m) => m.recommended) ?? list[0];
        if (pick) setModel(pick.id);
      }
    } catch (e) {
      setModels(null);
      setError(errorMessage(e));
    } finally {
      setLoadingModels(false);
    }
  }, [api, provider, setModel]);

  // Where the key is, and the models it can use, whenever the dialog opens or the provider changes.
  useEffect(() => {
    if (!open || !api) return;
    let current = true;
    setModels(null);
    setError(null);
    setOfferSession(false);
    void (async () => {
      try {
        const [kept, where] = await Promise.all([api.keyStorage(), api.keyLocation(provider)]);
        if (!current) return;
        setStorage(kept);
        setLocation(where);
        if (where) await loadModels();
      } catch (e) {
        if (current) setError(errorMessage(e));
      }
    })();
    return () => {
      current = false;
    };
  }, [open, api, provider, loadModels]);

  useEffect(() => {
    if (!open) return;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSettingsOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setSettingsOpen]);

  if (!open || !api) return null;
  const name = providerName(provider);
  const info = PROVIDERS.find((p) => p.id === provider)!;

  /** Sends the typed key to the app and clears the field: the window keeps no copy. */
  const saveKey = async (forSession: boolean) => {
    const input = keyRef.current;
    if (!input || busy) return;
    const typed = input.value;
    setBusy(true);
    setError(null);
    try {
      const where = forSession ? await api.useKeyForSession(provider, typed) : await api.setApiKey(provider, typed);
      input.value = "";
      setHasTyped(false);
      setOfferSession(false);
      setLocation(where);
      await refreshKey();
      await loadModels();
    } catch (e) {
      const message = errorMessage(e);
      setError(message);
      if (!forSession && message.includes("for this session only")) setOfferSession(true);
    } finally {
      setBusy(false);
    }
  };

  const removeKey = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.deleteApiKey(provider);
      setLocation(null);
      setModels(null);
      await refreshKey();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const keyStatus =
    location === "keychain"
      ? `Key saved in your ${storage?.name ?? "Keychain"}`
      : location === "session"
        ? "Key kept for this session only (forgotten when PixelFlow quits)"
        : "No key yet";

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[10vh]" onClick={() => setSettingsOpen(false)}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="ai-settings-title"
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-lg rounded-xl border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 id="ai-settings-title" className="text-base font-semibold">
              Settings → AI
            </h2>
            <p className="mt-1 text-sm text-neutral-500 dark:text-neutral-400">
              Bring your own key. PixelFlow talks to the AI company itself; this window never sees your key again.
            </p>
          </div>
          <Button ref={closeRef} variant="ghost" aria-label="Close AI settings" onClick={() => setSettingsOpen(false)}>
            <X size={16} aria-hidden />
          </Button>
        </div>

        <div className="mt-4 flex flex-col gap-4">
          <Field label="Provider">
            <Select value={provider} onChange={(e) => void setProvider(e.target.value as typeof provider)}>
              {PROVIDERS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </Select>
          </Field>

          <div className="flex flex-col gap-1 text-sm">
            <label htmlFor="ai-key" className="text-neutral-600 dark:text-neutral-400">
              {name} API key
            </label>
            <div className="flex gap-2">
              <input
                id="ai-key"
                ref={keyRef}
                type="password"
                autoComplete="off"
                spellCheck={false}
                placeholder={location ? "Paste a new key to replace it" : `Paste your key (${info.keyHint})`}
                onChange={(e) => setHasTyped(e.target.value.trim().length > 0)}
                onKeyDown={(e) => e.key === "Enter" && void saveKey(false)}
                className="min-w-0 flex-1 rounded-md border border-neutral-300 bg-white px-2 py-1.5 font-mono text-sm dark:border-neutral-700 dark:bg-neutral-950"
                aria-describedby="ai-key-status"
              />
              <Button variant="primary" disabled={busy || !hasTyped} onClick={() => void saveKey(false)}>
                <KeyRound size={14} aria-hidden /> Save key
              </Button>
            </div>
            <p id="ai-key-status" role="status" className={location ? "text-green-700 dark:text-green-400" : "text-neutral-500"}>
              {keyStatus}
            </p>
            {storage && !storage.available && !location && (
              <p className="text-amber-700 dark:text-amber-400">
                This computer has no {storage.name} PixelFlow can use, so keys can&apos;t be saved. You can use one for this session only.
              </p>
            )}
            {(offerSession || (storage && !storage.available)) && (
              <div>
                <Button disabled={busy || !hasTyped} onClick={() => void saveKey(true)}>
                  Use for this session only
                </Button>
              </div>
            )}
            {!location && <p className="text-xs text-neutral-500">Get a key at {info.keySite}.</p>}
            {location && (
              <div>
                <Button variant="danger" disabled={busy} onClick={() => void removeKey()}>
                  <Trash2 size={14} aria-hidden /> Remove key
                </Button>
              </div>
            )}
          </div>

          <div className="flex flex-col gap-1 text-sm">
            <label htmlFor="ai-model" className="text-neutral-600 dark:text-neutral-400">
              Model
            </label>
            <div className="flex gap-2">
              <Select
                id="ai-model"
                className="min-w-0 flex-1"
                value={model ?? ""}
                disabled={!models || models.length === 0}
                onChange={(e) => setModel(e.target.value)}
              >
                {!models && <option value="">{location ? (loadingModels ? "Loading models…" : "No models loaded") : "Save a key to see models"}</option>}
                {models?.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                    {m.recommended ? " (suggested)" : ""}
                  </option>
                ))}
              </Select>
              <Button aria-label="Reload models" disabled={!location || loadingModels} onClick={() => void loadModels()}>
                <RefreshCw size={14} aria-hidden className={loadingModels ? "animate-spin" : ""} />
              </Button>
            </div>
            <p className="text-xs text-neutral-500">Only models that can chat and use tools are listed.</p>
          </div>

          {error && (
            <p role="alert" className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700 dark:bg-red-950/50 dark:text-red-300">
              {error}
            </p>
          )}

          {provider === "openai" && (
            <label className="flex items-start gap-2 text-sm">
              <input type="checkbox" className="mt-0.5" checked={!lyricsAudioOk} onChange={(e) => setLyricsAudioOk(!e.target.checked)} />
              <span>Ask before Find lyrics sends a song&apos;s audio to OpenAI to hear the words</span>
            </label>
          )}

          <p className="text-xs text-neutral-500">
            When you chat, PixelFlow sends your question and the parts of your show the assistant reads to {name}. The assistant
            can&apos;t save files, start lights, or contact your controllers; it drafts changes for you to apply. Find lyrics
            sends only a song&apos;s name and length to LRCLIB, and its audio to OpenAI only if you agree.
          </p>
        </div>
      </div>
    </div>
  );
}
