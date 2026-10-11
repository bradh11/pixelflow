//! The contract between the window's TypeScript and the shell, checked in both directions with
//! files both sides read (`app/src/api/contracts`, see `docs/dev/contracts.md`):
//!
//! - `replies`: what the shell sends (replies, event payloads, rejections), written here from
//!   real commands and type-checked against the window's types by `replies.test.ts`;
//! - `commands`: what the window sends (`commandCalls.json`, recorded by
//!   `commandCalls.test.ts`), held here against the commands' own parameters.

use super::*;

mod commands;
mod replies;
pub(super) mod support;
