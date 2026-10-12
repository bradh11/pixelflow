//! How good are the Assistant's light shows? Realistic requests on a made-up display with a
//! made-up song, each answered through the real chat loop and scored by the design review and by
//! plain measures. `docs/dev/assistant-eval.md` has the whole story.
//!
//! With no arguments every answer is scripted: no model is called and nothing is sent.
//!
//! ```text
//! cargo run --release -p pf-ai --example assistant_eval
//! cargo run --release -p pf-ai --example assistant_eval -- --list
//! cargo run --release -p pf-ai --example assistant_eval -- --provider openai --model <id>
//! PIXELFLOW_EVAL_KEY=… cargo run --release -p pf-ai --example assistant_eval -- \
//!     --provider openai --model <id> --send --max-usd 10 \
//!     --price-in <usd> --price-out <usd> --price-cached <share> \
//!     --out <a folder outside the repository>
//! ```
//!
//! A real run needs `--provider`, `--model`, `--send`, a spending ceiling (`--max-usd`), and the
//! model's prices given explicitly (plus `--price-cache-write` for Anthropic); without `--send`
//! it says what it would send and roughly what that would cost, and stops. No request goes whose
//! worst case would take the run past the ceiling. The key is read from the
//! `PIXELFLOW_EVAL_KEY` environment variable, here and nowhere else, and only then.

use pf_ai::eval::cli::{self, KEY_VARIABLE};
use pf_ai::http::UreqTransport;
use pf_ai::{ApiKey, LlmProvider, ProviderId};
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut connect = |provider: ProviderId| -> Result<(Arc<dyn LlmProvider>, ApiKey), String> {
        let key = std::env::var(KEY_VARIABLE)
            .map_err(|_| format!("Put your {provider} key in the {KEY_VARIABLE} environment variable."))?;
        let key = ApiKey::new(key).map_err(|e| format!("The key in {KEY_VARIABLE} can't be used: {e}."))?;
        Ok((
            pf_ai::providers(Arc::new(UreqTransport::new())).get(provider),
            key,
        ))
    };
    std::process::exit(cli::main(&args, &mut connect, &mut std::io::stdout()));
}
