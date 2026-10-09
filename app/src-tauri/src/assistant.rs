//! The AI assistant's commands: API keys (write-only), the model list, chat turns streamed as
//! events, and the proposal's preview, Apply, and Discard.
//!
//! Keys go in and never come back out: no command returns one, and `ApiKey` can't even be
//! serialized. Provider calls run on a blocking thread from Rust; the engine is locked only to
//! copy the show before a turn and to apply a proposal, never during a network call.

use crate::layout::{Dims, encode_preview};
use crate::{AppState, Reply};
use pf_ai::{
    AiError, ApiKey, Applied, Cancel, ChatEvent, ChatSession, KeyLocation, KeyVault, ModelInfo, ProviderId,
    Providers, TurnReply, UiContext, Workspace, apply_proposal,
};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Runtime, State};

/// A proposal's draft sequence ready to play: its renderer and the sequence.
struct DraftPlayer {
    proposal: String,
    renderer: pf_engine::DraftRenderer,
    doc: pf_sequence::Sequence,
}

/// The event a chat turn streams on.
pub(crate) const ASSISTANT_EVENT: &str = "assistant-event";

/// The longest model id accepted.
const MAX_MODEL_ID: usize = 200;

/// The assistant's state, managed beside the engine's.
pub(crate) struct AiState {
    vault: Arc<KeyVault>,
    providers: Providers,
    session: Arc<Mutex<ChatSession>>,
    /// The turn in progress (its Stop), if any.
    running: Mutex<Option<Cancel>>,
    /// Numbers turns, so the window can tell their events apart.
    turns: Mutex<u64>,
    /// The proposal whose draft sequence is being previewed.
    player: Mutex<Option<DraftPlayer>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AiState {
    pub(crate) fn new(vault: KeyVault, providers: Providers) -> Self {
        Self {
            vault: Arc::new(vault),
            providers,
            session: Arc::default(),
            running: Mutex::default(),
            turns: Mutex::default(),
            player: Mutex::default(),
        }
    }

    /// The real thing: the OS credential store and HTTPS to the providers.
    pub(crate) fn live() -> Self {
        Self::new(
            KeyVault::os(),
            pf_ai::providers(Arc::new(pf_ai::http::UreqTransport::new())),
        )
    }

    /// The keys, for checking one is there (Find lyrics) and for requests made from Rust.
    pub(crate) fn vault(&self) -> Arc<KeyVault> {
        Arc::clone(&self.vault)
    }

    /// Lets go of the draft being previewed (its proposal was applied, discarded, or dropped).
    fn forget_player(&self) {
        *lock(&self.player) = None;
    }

    fn idle(&self) -> Reply<()> {
        if lock(&self.running).is_some() {
            return Err("The assistant is still answering. Press Stop first.".to_string());
        }
        Ok(())
    }
}

/// Marks the turn in progress as over when dropped.
struct Running<'a>(&'a Mutex<Option<Cancel>>);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        *lock(self.0) = None;
    }
}

fn text(error: AiError) -> String {
    error.to_string()
}

/// What a chat turn or the model list fails with: the plain message, and the provider's own
/// (sanitized) words for it when there are any, for the chat's "Details".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Failure {
    message: String,
    details: Option<String>,
}

impl From<AiError> for Failure {
    fn from(error: AiError) -> Self {
        Self {
            message: error.to_string(),
            details: error.details().map(str::to_string),
        }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            details: None,
        }
    }
}

async fn off_thread_failing<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| Failure::from("Something went wrong in the assistant.".to_string()))?
}

async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> Reply<T> + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Something went wrong in the assistant.".to_string())?
}

/// Where keys can be kept on this computer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyStorage {
    /// "Keychain", "Windows Credential Manager", or "system keyring".
    name: &'static str,
    available: bool,
}

#[tauri::command]
pub(crate) async fn ai_key_storage(ai: State<'_, AiState>) -> Reply<KeyStorage> {
    let vault = Arc::clone(&ai.vault);
    off_thread(move || {
        Ok(KeyStorage {
            name: pf_ai::keychain_name(),
            available: vault.keychain_available(),
        })
    })
    .await
}

/// Saves a key in the OS credential store. When there is none, the error says so and the
/// window offers [`use_api_key_for_session`].
#[tauri::command]
pub(crate) async fn set_api_key(
    ai: State<'_, AiState>,
    provider: ProviderId,
    key: ApiKey,
) -> Reply<KeyLocation> {
    let vault = Arc::clone(&ai.vault);
    off_thread(move || vault.save(provider, key).map_err(text)).await
}

/// Keeps a key in memory until PixelFlow quits (never written anywhere).
#[tauri::command]
pub(crate) async fn use_api_key_for_session(
    ai: State<'_, AiState>,
    provider: ProviderId,
    key: ApiKey,
) -> Reply<KeyLocation> {
    Ok(ai.vault.use_for_session(provider, key))
}

#[tauri::command]
pub(crate) async fn has_api_key(ai: State<'_, AiState>, provider: ProviderId) -> Reply<bool> {
    let vault = Arc::clone(&ai.vault);
    off_thread(move || vault.has(provider).map_err(text)).await
}

/// Where the provider's key is kept ("keychain" or "session"), or null.
#[tauri::command]
pub(crate) async fn api_key_location(
    ai: State<'_, AiState>,
    provider: ProviderId,
) -> Reply<Option<KeyLocation>> {
    let vault = Arc::clone(&ai.vault);
    off_thread(move || vault.location(provider).map_err(text)).await
}

#[tauri::command]
pub(crate) async fn delete_api_key(ai: State<'_, AiState>, provider: ProviderId) -> Reply<()> {
    let vault = Arc::clone(&ai.vault);
    off_thread(move || vault.remove(provider).map_err(text)).await
}

/// The provider's chat models that can use tools, live from the provider, best first.
#[tauri::command]
pub(crate) async fn list_ai_models(
    ai: State<'_, AiState>,
    provider: ProviderId,
) -> Result<Vec<ModelInfo>, Failure> {
    let vault = Arc::clone(&ai.vault);
    let llm = ai.providers.get(provider);
    off_thread_failing(move || {
        let key = vault.key(provider)?;
        Ok(llm.list_models(&key, &Cancel::new())?)
    })
    .await
}

/// One streamed chat event, tagged with its turn.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssistantEvent {
    turn: u64,
    event: ChatEvent,
}

/// Sends a message to the assistant. Its reply streams as [`ASSISTANT_EVENT`] events; the
/// result is the whole reply and any proposal. The show is copied first (the engine isn't held
/// while the model works), and nothing the model does changes it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn ai_send<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
    provider: ProviderId,
    model: String,
    message: String,
    context: Option<UiContext>,
) -> Result<TurnReply, Failure> {
    let model = model.trim().to_string();
    if model.is_empty() || model.len() > MAX_MODEL_ID {
        return Err("Pick a model in Settings → AI first.".to_string().into());
    }
    let cancel = {
        let mut running = lock(&ai.running);
        if running.is_some() {
            return Err("The assistant is still answering. Wait for it, or press Stop."
                .to_string()
                .into());
        }
        let cancel = Cancel::new();
        *running = Some(cancel.clone());
        cancel
    };
    // However this ends (an error, a panic, the window going away), the assistant is free again.
    let _running = Running(&ai.running);
    let turn = {
        let mut turns = lock(&ai.turns);
        *turns += 1;
        *turns
    };
    let workspace = {
        let engine = state.engine();
        Workspace::from_engine(&engine, context.unwrap_or_default())
    };
    let vault = Arc::clone(&ai.vault);
    let llm = ai.providers.get(provider);
    let session = Arc::clone(&ai.session);
    off_thread_failing(move || {
        let key = vault.key(provider)?;
        let mut session = lock(&session);
        Ok(session.run_turn(
            llm.as_ref(),
            &key,
            &model,
            &message,
            workspace,
            &cancel,
            &mut |event| {
                // A closed window can't show the reply; the turn finishes anyway.
                let _ = app.emit(ASSISTANT_EVENT, AssistantEvent { turn, event });
            },
        )?)
    })
    .await
}

/// Stops the reply in progress (the turn ends at its next step and says "Stopped.").
#[tauri::command]
pub(crate) async fn ai_stop(ai: State<'_, AiState>) -> Reply<()> {
    if let Some(cancel) = lock(&ai.running).as_ref() {
        cancel.cancel();
    }
    Ok(())
}

/// Starts a new chat (forgetting the old one and its draft).
#[tauri::command]
pub(crate) async fn ai_new_chat(ai: State<'_, AiState>) -> Reply<()> {
    ai.idle()?;
    *lock(&ai.session) = ChatSession::new();
    ai.forget_player();
    Ok(())
}

fn current_proposal(ai: &AiState, id: &str) -> Reply<pf_ai::Proposal> {
    lock(&ai.session)
        .proposal()
        .filter(|p| p.id == id)
        .cloned()
        .ok_or_else(|| "That proposal isn't the latest one anymore.".to_string())
}

/// Applies the proposal: the show changes as one undo step (and the open sequence as one
/// sequence undo step). Only ever called from the user's Apply.
#[tauri::command]
pub(crate) async fn ai_apply(
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
    id: String,
) -> Reply<Applied> {
    ai.idle()?;
    let proposal = current_proposal(&ai, &id)?;
    let applied = apply_proposal(&mut state.engine(), &proposal)?;
    lock(&ai.session).applied();
    ai.forget_player();
    Ok(applied)
}

/// Drops the draft and proposal when the show (or sequence document) they were made for isn't
/// open anymore; the window calls this after the show or sequence is replaced. True when
/// something was dropped. While the assistant is answering, nothing is dropped yet (Apply
/// checks again, and the next message starts over).
#[tauri::command]
pub(crate) async fn ai_sync(state: State<'_, AppState>, ai: State<'_, AiState>) -> Reply<bool> {
    if ai.idle().is_err() {
        return Ok(false);
    }
    let (generation, document) = {
        let engine = state.engine();
        (engine.show_generation(), engine.sequence_doc_id())
    };
    let dropped = lock(&ai.session).sync_to(generation, document);
    if dropped {
        ai.forget_player();
    }
    Ok(dropped)
}

/// Throws the proposal and its draft away.
#[tauri::command]
pub(crate) async fn ai_discard(ai: State<'_, AiState>, id: String) -> Reply<()> {
    ai.idle()?;
    current_proposal(&ai, &id)?;
    lock(&ai.session).discarded();
    ai.forget_player();
    Ok(())
}

/// The proposal's show as pixel positions (front view), raw like `preview_props`, for showing
/// the draft without applying it.
#[tauri::command]
pub(crate) async fn ai_preview(ai: State<'_, AiState>, id: String) -> Reply<Response> {
    // (The chat is busy for the whole turn; waiting on it here would hold up other commands.)
    ai.idle()?;
    let proposal = current_proposal(&ai, &id)?;
    Ok(Response::new(encode_preview(
        0,
        &pf_engine::preview_props_of(&proposal.draft_show),
        Dims::Flat,
    )))
}

/// The proposal's draft sequence at `position_ms`, drawn on its draft show (show frame bytes,
/// raw, laid out like [`ai_preview`]'s pixels): the preview plays the draft without applying it.
/// Nothing is sent to the controllers.
#[tauri::command]
pub(crate) async fn ai_preview_frame(
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
    id: String,
    position_ms: u64,
) -> Reply<Response> {
    ai.idle()?;
    let current = lock(&ai.session).proposal().is_some_and(|p| p.id == id);
    if !current {
        ai.forget_player();
        return Err("That proposal isn't the latest one anymore.".to_string());
    }
    let cached = lock(&ai.player).take().filter(|p| p.proposal == id);
    let mut player = match cached {
        Some(player) => player,
        None => {
            let proposal = current_proposal(&ai, &id)?;
            let doc = proposal
                .draft_sequence
                .filter(|_| !proposal.sequence_edits.is_empty())
                .ok_or_else(|| "This suggestion doesn't change the sequence.".to_string())?;
            let mut renderer = pf_engine::DraftRenderer::new(&proposal.draft_show);
            // The draft follows the open sequence's music.
            renderer.set_audio(state.engine().sequence_audio());
            DraftPlayer {
                proposal: id,
                renderer,
                doc,
            }
        }
    };
    // Rendering is real work: off the async workers.
    let (player, frame) = tauri::async_runtime::spawn_blocking(move || {
        let frame = player.renderer.frame(&player.doc, position_ms);
        (player, frame)
    })
    .await
    .map_err(|_| "Something went wrong drawing the preview.".to_string())?;
    // Kept for the next frame, unless the proposal was applied, discarded, or replaced meanwhile.
    if lock(&ai.session)
        .proposal()
        .is_some_and(|p| p.id == player.proposal)
    {
        *lock(&ai.player) = Some(player);
    }
    Ok(Response::new(frame))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::DeviceAccess;
    use crate::{context, with_commands};
    use pf_ai::MemoryStore;
    use pf_ai::http::RetryPolicy;
    use pf_ai::testing::{FAKE_KEY, FakeTransport, Reply};
    use pf_engine::Engine;
    use serde_json::{Value, json};
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;
    use tauri::{App, Listener, Manager, WebviewWindow, WebviewWindowBuilder};

    struct TestApp {
        app: App<MockRuntime>,
        webview: WebviewWindow<MockRuntime>,
        anthropic: Arc<FakeTransport>,
        /// Find lyrics' recorded LRCLIB and OpenAI replies.
        lrclib: Arc<FakeTransport>,
        whisper: Arc<FakeTransport>,
        _dir: tempfile::TempDir,
    }

    /// The app with a fake credential store and recorded provider replies: no keychain, no
    /// network.
    fn app_with(store: MemoryStore) -> TestApp {
        let dir = tempfile::tempdir().unwrap();
        let (transport, _) = pf_output::RecordingTransport::new();
        let engine = Engine::new(dir.path())
            .with_transport(move || Ok(Box::new(transport.clone()) as Box<dyn pf_output::Transport>));
        let anthropic = Arc::new(FakeTransport::default());
        let openai = Arc::new(FakeTransport::default());
        let lrclib = Arc::new(FakeTransport::default());
        let whisper = Arc::new(FakeTransport::default());
        let providers = Providers {
            anthropic: Arc::new(
                pf_ai::anthropic::Anthropic::new(anthropic.clone()).with_retry(RetryPolicy::immediate()),
            ),
            openai: Arc::new(pf_ai::openai::OpenAi::new(openai).with_retry(RetryPolicy::immediate())),
        };
        let app = with_commands(mock_builder())
            .manage(AppState {
                engine: Mutex::new(engine),
                devices: DeviceAccess::fake(pf_devices::testing::network()),
                waveforms: Mutex::default(),
                photos: Default::default(),
                models: Default::default(),
                export_cancels: Default::default(),
                send_cancels: Default::default(),
                download_cancels: Default::default(),
                download_folders: Default::default(),
                checking_files: Default::default(),
                recent: Arc::new(crate::recent::RecentShows::in_memory()),
                last_folders: crate::pickers::LastFolders::new(None),
                dialog: Default::default(),
                imported_from: Mutex::default(),
                vendor_mappings: Arc::new(crate::vendor::SavedMappings::new(None)),
            })
            .manage(AiState::new(KeyVault::new(Box::new(store)), providers))
            .manage(crate::lyrics::LyricsState::new(
                pf_ai::lyrics::Services {
                    lrclib: pf_ai::lyrics::lrclib::Lrclib::new(lrclib.clone()),
                    transcriber: pf_ai::lyrics::transcribe::Transcriber::new(whisper.clone())
                        .with_retry(RetryPolicy::immediate()),
                    voice: Box::new(|_, _| Ok(pf_analysis::VocalActivity::default())),
                    tags: Box::new(|path| pf_audio::read_tags(path).ok()),
                },
                None,
            ))
            .build(context())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        TestApp {
            app,
            webview,
            anthropic,
            lrclib,
            whisper,
            _dir: dir,
        }
    }

    fn request(
        webview: &WebviewWindow<MockRuntime>,
        cmd: &str,
        args: Value,
    ) -> Result<InvokeResponseBody, Value> {
        get_ipc_response(
            webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: webview.url().unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
    }

    /// Calls a command like the window does. No reply may ever contain the key.
    fn call(t: &TestApp, cmd: &str, args: Value) -> Result<Value, Value> {
        let reply = request(&t.webview, cmd, args).map(|body| body.deserialize::<Value>().unwrap());
        let shown = format!("{reply:?}");
        assert!(!shown.contains(FAKE_KEY), "{cmd} returned the key: {shown}");
        reply
    }

    /// One streamed Anthropic reply: optional text, then optional tool calls.
    fn sse(text: &str, tools: &[(&str, Value)]) -> String {
        let mut out =
            String::from("event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n");
        let mut index = 0;
        if !text.is_empty() {
            out += &format!(
                "event: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n",
                json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
                json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": text } }),
            );
            index += 1;
        }
        for (i, (name, input)) in tools.iter().enumerate() {
            out += &format!(
                "event: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {}\n\n",
                json!({ "type": "content_block_start", "index": index, "content_block": { "type": "tool_use", "id": format!("toolu_{i}"), "name": name, "input": {} } }),
                json!({ "type": "content_block_delta", "index": index, "delta": { "type": "input_json_delta", "partial_json": input.to_string() } }),
                json!({ "type": "content_block_stop", "index": index }),
            );
            index += 1;
        }
        let stop = if tools.is_empty() { "end_turn" } else { "tool_use" };
        out += &format!(
            "event: message_delta\ndata: {}\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n",
            json!({ "type": "message_delta", "delta": { "stop_reason": stop } })
        );
        out
    }

    #[test]
    fn keys_go_in_but_never_come_back_out() {
        let t = app_with(MemoryStore::new());
        let storage = call(&t, "ai_key_storage", json!({})).unwrap();
        assert_eq!(storage["available"], true);
        assert_eq!(
            call(&t, "has_api_key", json!({ "provider": "anthropic" })).unwrap(),
            false
        );
        assert_eq!(
            call(
                &t,
                "set_api_key",
                json!({ "provider": "anthropic", "key": FAKE_KEY })
            )
            .unwrap(),
            "keychain"
        );
        assert_eq!(
            call(&t, "has_api_key", json!({ "provider": "anthropic" })).unwrap(),
            true
        );
        assert_eq!(
            call(&t, "has_api_key", json!({ "provider": "openai" })).unwrap(),
            false
        );
        assert_eq!(
            call(&t, "api_key_location", json!({ "provider": "anthropic" })).unwrap(),
            "keychain"
        );
        call(&t, "delete_api_key", json!({ "provider": "anthropic" })).unwrap();
        assert_eq!(
            call(&t, "has_api_key", json!({ "provider": "anthropic" })).unwrap(),
            false
        );

        // A key that isn't one is refused without echoing it.
        let err = call(
            &t,
            "set_api_key",
            json!({ "provider": "openai", "key": "sk bad key" }),
        )
        .unwrap_err();
        assert!(!err.to_string().contains("sk bad key"), "{err}");
    }

    #[test]
    fn without_a_credential_store_keys_can_be_used_for_the_session() {
        let t = app_with(MemoryStore::unavailable());
        assert_eq!(call(&t, "ai_key_storage", json!({})).unwrap()["available"], false);
        let err = call(
            &t,
            "set_api_key",
            json!({ "provider": "openai", "key": FAKE_KEY }),
        )
        .unwrap_err();
        assert!(err.as_str().unwrap().contains("for this session only"), "{err}");
        assert_eq!(
            call(
                &t,
                "use_api_key_for_session",
                json!({ "provider": "openai", "key": FAKE_KEY })
            )
            .unwrap(),
            "session"
        );
        assert_eq!(
            call(&t, "has_api_key", json!({ "provider": "openai" })).unwrap(),
            true
        );
        assert_eq!(
            call(&t, "api_key_location", json!({ "provider": "openai" })).unwrap(),
            "session"
        );
    }

    #[test]
    fn a_chat_turn_drafts_previews_and_applies_as_one_undo_step() {
        let t = app_with(MemoryStore::new());
        let send = json!({ "provider": "anthropic", "model": "claude-opus-5-5", "message": "Call the show Christmas" });
        assert_eq!(
            call(&t, "ai_send", send.clone()).unwrap_err(),
            json!({ "message": "Add your Anthropic API key in Settings → AI first.", "details": null })
        );
        call(
            &t,
            "set_api_key",
            json!({ "provider": "anthropic", "key": FAKE_KEY }),
        )
        .unwrap();
        let original = call(&t, "get_snapshot", json!({})).unwrap();

        t.anthropic.push(Reply::ok(sse(
            "Renaming it.",
            &[("show_rename_show", json!({ "name": "Christmas" }))],
        )));
        t.anthropic.push(Reply::ok(sse(
            "",
            &[(
                "propose_changes",
                json!({ "summary": "Renames the show to Christmas." }),
            )],
        )));
        t.anthropic.push(Reply::ok(sse("Take a look.", &[])));
        let events = Arc::new(Mutex::new(Vec::new()));
        let heard = Arc::clone(&events);
        t.app.listen_any(ASSISTANT_EVENT, move |event| {
            lock(&heard).push(serde_json::from_str::<Value>(event.payload()).unwrap());
        });
        let reply = call(&t, "ai_send", send).unwrap();
        assert_eq!(reply["text"], "Renaming it.\n\nTake a look.");
        let proposal = &reply["proposal"];
        assert_eq!(proposal["summary"], "Renames the show to Christmas.");
        assert_eq!(
            proposal["diff"]["changes"][0]["details"][0],
            "name: \"Untitled Show\" → \"Christmas\""
        );
        let heard = lock(&events).clone();
        // (Turn 2: the send without a key was turn 1.)
        assert_eq!(
            heard[0],
            json!({ "turn": 2, "event": { "kind": "text", "text": "Renaming it." } })
        );
        assert!(heard.iter().any(|e| e["event"]["kind"] == "proposal"));

        // The key went only in the header, and the show is unchanged until Apply.
        let requests = t.anthropic.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].header("x-api-key").unwrap(), FAKE_KEY);
        assert!(requests.iter().all(|r| {
            !r.body
                .as_ref()
                .and_then(|b| b.text())
                .unwrap_or("")
                .contains(FAKE_KEY)
        }));
        assert_eq!(call(&t, "get_snapshot", json!({})).unwrap(), original);

        let id = proposal["id"].as_str().unwrap();
        let preview = request(&t.webview, "ai_preview", json!({ "id": id })).unwrap();
        assert!(matches!(preview, InvokeResponseBody::Raw(_)));
        assert_eq!(
            call(&t, "ai_apply", json!({ "id": "not-the-one" })).unwrap_err(),
            "That proposal isn't the latest one anymore."
        );
        let applied = call(&t, "ai_apply", json!({ "id": id })).unwrap();
        assert_eq!(applied["snapshot"]["show"]["name"], "Christmas");
        assert_eq!(applied["snapshot"]["canUndo"], true);
        assert_eq!(applied["sequence"], Value::Null);
        // Applied once: the proposal is gone.
        assert!(call(&t, "ai_apply", json!({ "id": id })).is_err());
        let undone = call(&t, "undo", json!({})).unwrap();
        assert_eq!(undone["show"], original["show"]);
        assert_eq!(undone["canUndo"], false);
    }

    #[test]
    fn a_sequence_proposal_plays_in_the_preview_without_applying() {
        let t = app_with(MemoryStore::new());
        call(
            &t,
            "set_api_key",
            json!({ "provider": "anthropic", "key": FAKE_KEY }),
        )
        .unwrap();
        // No sequence open: the assistant offers the song picker.
        t.anthropic.push(Reply::ok(sse(
            "You don't have a sequence open yet.",
            &[("ask_for_song", json!({}))],
        )));
        t.anthropic
            .push(Reply::ok(sse("Pick a song and I'll build it.", &[])));
        let send =
            json!({ "provider": "anthropic", "model": "claude-opus-5-5", "message": "Make me a sequence" });
        let reply = call(&t, "ai_send", send).unwrap();
        assert_eq!(reply["chooseSong"], true);

        // The user picked a song; the app made the sequence. Now the draft fills it.
        call(
            &t,
            "new_sequence_doc",
            json!({ "name": "Jingle", "durationMs": 10_000, "audio": null }),
        )
        .unwrap();
        let track = pf_sequence::TimingTrack::new("Beats", pf_sequence::TimingKind::Beats, vec![]);
        t.anthropic.push(Reply::ok(sse(
            "",
            &[(
                "sequence_add_timing_track",
                json!({ "track": serde_json::to_value(&track).unwrap() }),
            )],
        )));
        t.anthropic.push(Reply::ok(sse("Here it is.", &[])));
        let reply = call(
            &t,
            "ai_send",
            json!({ "provider": "anthropic", "model": "claude-opus-5-5", "message": "I chose a song." }),
        )
        .unwrap();
        let proposal = &reply["proposal"];
        assert_eq!(proposal["changesSequence"], true);
        assert_eq!(proposal["timeline"]["durationMs"], 10_000);
        assert_eq!(proposal["sections"][0]["label"], "Whole sequence");
        let id = proposal["id"].as_str().unwrap();
        let frame = request(
            &t.webview,
            "ai_preview_frame",
            json!({ "id": id, "positionMs": 500 }),
        )
        .unwrap();
        assert!(matches!(frame, InvokeResponseBody::Raw(_)));
        assert!(
            request(
                &t.webview,
                "ai_preview_frame",
                json!({ "id": "another", "positionMs": 0 })
            )
            .is_err()
        );
        let doc = call(&t, "get_sequence_doc", json!({})).unwrap();
        assert_eq!(
            doc["sequence"]["timingTracks"],
            json!([]),
            "previewing applies nothing"
        );

        // Once discarded, the draft no longer plays (nothing is kept for it).
        call(&t, "ai_discard", json!({ "id": id })).unwrap();
        assert!(
            request(
                &t.webview,
                "ai_preview_frame",
                json!({ "id": id, "positionMs": 500 })
            )
            .is_err()
        );
        let ai = t.app.state::<AiState>();
        assert!(lock(&ai.player).is_none());
    }

    #[test]
    fn opening_another_show_drops_the_proposal() {
        let t = app_with(MemoryStore::new());
        call(
            &t,
            "set_api_key",
            json!({ "provider": "anthropic", "key": FAKE_KEY }),
        )
        .unwrap();
        t.anthropic.push(Reply::ok(sse(
            "",
            &[("show_rename_show", json!({ "name": "Christmas" }))],
        )));
        t.anthropic.push(Reply::ok(sse("Renamed.", &[])));
        let reply = call(
            &t,
            "ai_send",
            json!({ "provider": "anthropic", "model": "claude-opus-5-5", "message": "rename" }),
        )
        .unwrap();
        let id = reply["proposal"]["id"].as_str().unwrap().to_string();
        assert_eq!(call(&t, "ai_sync", json!({})).unwrap(), false, "same show: kept");
        call(&t, "new_show", json!({ "name": "Show B" })).unwrap();
        assert_eq!(
            call(&t, "ai_sync", json!({})).unwrap(),
            true,
            "another show: dropped"
        );
        assert_eq!(
            call(&t, "ai_apply", json!({ "id": id })).unwrap_err(),
            "That proposal isn't the latest one anymore."
        );
        assert_eq!(
            call(&t, "get_snapshot", json!({})).unwrap()["show"]["name"],
            "Show B"
        );
    }

    #[test]
    fn provider_errors_are_plain_and_the_chat_can_start_over() {
        let t = app_with(MemoryStore::new());
        call(
            &t,
            "set_api_key",
            json!({ "provider": "anthropic", "key": FAKE_KEY }),
        )
        .unwrap();
        t.anthropic.push(Reply::status(
            401,
            json!({ "type": "error", "error": { "type": "authentication_error", "message": "invalid x-api-key" } }).to_string(),
        ));
        let err = call(
            &t,
            "ai_send",
            json!({ "provider": "anthropic", "model": "claude-opus-5-5", "message": "hi" }),
        )
        .unwrap_err();
        assert!(
            err["message"]
                .as_str()
                .unwrap()
                .starts_with("Anthropic didn't accept your API key."),
            "{err}"
        );
        // The provider's own words, for the chat's Details.
        assert_eq!(err["details"], "HTTP 401 authentication_error: invalid x-api-key");
        assert_eq!(
            call(
                &t,
                "ai_send",
                json!({ "provider": "anthropic", "model": " ", "message": "hi" })
            )
            .unwrap_err()["message"],
            "Pick a model in Settings → AI first."
        );
        call(&t, "ai_stop", json!({})).unwrap();
        call(&t, "ai_new_chat", json!({})).unwrap();

        t.anthropic.push(Reply::ok(
            json!({ "data": [{ "type": "model", "id": "claude-opus-5-5", "display_name": "Claude Opus 5.5" }], "has_more": false })
                .to_string(),
        ));
        let models = call(&t, "list_ai_models", json!({ "provider": "anthropic" })).unwrap();
        assert_eq!(
            models,
            json!([{ "id": "claude-opus-5-5", "name": "Claude Opus 5.5", "recommended": true }])
        );
        assert!(t.app.try_state::<AiState>().is_some());
    }

    /// Made-up lyrics, as LRCLIB answers a search.
    fn lrclib_reply() -> String {
        json!([{
            "id": 1, "trackName": "Lantern Song", "artistName": "Lantern Band", "albumName": "Made Up",
            "duration": 4.0, "instrumental": false,
            "plainLyrics": "Paper lanterns glowing\nSnowy rooftops shine",
            "syncedLyrics": "[00:00.50]Paper lanterns glowing\n[00:02.00]Snowy rooftops shine\n[00:03.50]",
        }])
        .to_string()
    }

    /// A new sequence whose music is a silent 4 s WAV named like the song.
    fn sequence_with_song(t: &TestApp) {
        let song = t._dir.path().join("01 - Lantern Song.wav");
        std::fs::write(&song, pf_audio::wav_bytes(&vec![0.0; 64_000], 16_000)).unwrap();
        call(
            t,
            "new_sequence_doc",
            json!({ "name": "Lantern Song", "durationMs": 4_000, "audio": song.to_string_lossy() }),
        )
        .unwrap();
    }

    fn track_names(t: &TestApp) -> Vec<String> {
        let doc = call(t, "get_sequence_doc", json!({})).unwrap();
        doc["sequence"]["timingTracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn lyrics_wait_for_the_assistant_to_be_set_up() {
        let t = app_with(MemoryStore::new());
        sequence_with_song(&t);
        let gate = call(&t, "lyrics_gate", json!({ "provider": null })).unwrap();
        assert_eq!(gate["ready"], false);
        let gate = call(&t, "lyrics_gate", json!({ "provider": "anthropic" })).unwrap();
        assert_eq!(gate["ready"], false);
        assert!(gate["reason"].as_str().unwrap().contains("Settings → AI"));
        let err = call(
            &t,
            "find_lyrics",
            json!({ "provider": "anthropic", "upload": true }),
        )
        .unwrap_err();
        assert!(err.as_str().unwrap().contains("Settings → AI"), "{err}");
        // Nothing was asked of anyone.
        assert!(t.lrclib.requests().is_empty() && t.whisper.requests().is_empty());
    }

    #[test]
    fn with_anthropic_published_lyrics_become_timing_tracks_in_one_undo_step() {
        let t = app_with(MemoryStore::new());
        call(
            &t,
            "set_api_key",
            json!({ "provider": "anthropic", "key": FAKE_KEY }),
        )
        .unwrap();
        sequence_with_song(&t);
        assert_eq!(
            call(&t, "lyrics_gate", json!({ "provider": "anthropic" })).unwrap(),
            json!({ "ready": true, "reason": null, "recognizer": false })
        );
        t.lrclib.push(Reply::ok(lrclib_reply()));
        let found = call(
            &t,
            "find_lyrics",
            json!({ "provider": "anthropic", "upload": true }),
        )
        .unwrap();
        assert_eq!(
            found["summary"],
            "Lyrics and line timing from LRCLIB; words are spread over each line."
        );
        assert_eq!(
            (found["lines"].as_u64(), found["words"].as_u64()),
            (Some(2), Some(6))
        );
        assert_eq!(
            track_names(&t),
            [
                "Lyrics",
                "Lyrics (words)",
                "Lyrics (syllables)",
                "Lyrics (phonemes)",
                "Vocals"
            ]
        );
        // Only the song's name and length went out; no audio (Anthropic can't hear it).
        let requests = t.lrclib.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "https://lrclib.net/api/search?q=Lantern%20Song");
        assert!(t.whisper.requests().is_empty());
        // Syllables and mouth shapes made again from the words: the same tracks, updated.
        let doc = call(&t, "get_sequence_doc", json!({})).unwrap();
        let tracks = doc["sequence"]["timingTracks"].as_array().unwrap().clone();
        let id = |name: &str| tracks.iter().find(|t| t["name"] == name).unwrap()["id"].clone();
        call(
            &t,
            "syllables_from_words",
            json!({ "track": id("Lyrics (words)") }),
        )
        .unwrap();
        let again = call(&t, "get_sequence_doc", json!({})).unwrap();
        let syllables = &again["sequence"]["timingTracks"][2];
        assert_eq!(syllables["id"], id("Lyrics (syllables)"));
        assert_eq!(syllables["marks"][0]["label"], tracks[2]["marks"][0]["label"]);
        let err = call(&t, "syllables_from_words", json!({ "track": id("Lyrics") })).unwrap_err();
        assert_eq!(err, "That isn't a words track.");
        // Made again from unchanged words, nothing changed: one undo step takes all that Find
        // lyrics added away.
        call(&t, "undo_sequence", json!({})).unwrap();
        assert!(track_names(&t).is_empty());
    }

    #[test]
    fn with_openai_the_audio_goes_only_when_the_user_agreed() {
        let t = app_with(MemoryStore::new());
        call(
            &t,
            "set_api_key",
            json!({ "provider": "openai", "key": FAKE_KEY }),
        )
        .unwrap();
        sequence_with_song(&t);
        assert_eq!(
            call(&t, "lyrics_gate", json!({ "provider": "openai" })).unwrap()["recognizer"],
            true
        );
        t.lrclib.push(Reply::ok(lrclib_reply()));
        call(
            &t,
            "find_lyrics",
            json!({ "provider": "openai", "upload": false }),
        )
        .unwrap();
        assert!(t.whisper.requests().is_empty());

        t.lrclib.push(Reply::ok(lrclib_reply()));
        t.whisper.push(Reply::ok(
            json!({ "text": "paper lanterns glowing", "words": [
                { "word": "paper", "start": 0.6, "end": 0.9 },
                { "word": "lanterns", "start": 0.9, "end": 1.4 },
                { "word": "glowing", "start": 1.4, "end": 1.9 },
            ] })
            .to_string(),
        ));
        let found = call(&t, "find_lyrics", json!({ "provider": "openai", "upload": true })).unwrap();
        assert_eq!(found["summary"], "Lyrics from LRCLIB, word timing from OpenAI.");
        let requests = t.whisper.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].header("authorization").unwrap(),
            format!("Bearer {FAKE_KEY}")
        );
        // Found again: the same tracks updated, not copies.
        assert_eq!(
            track_names(&t),
            [
                "Lyrics",
                "Lyrics (words)",
                "Lyrics (syllables)",
                "Lyrics (phonemes)",
                "Vocals"
            ]
        );
        call(&t, "cancel_lyrics", json!({})).unwrap();
    }
}
