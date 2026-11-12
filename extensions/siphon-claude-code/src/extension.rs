//! Claude Code as an extension of Siphon's: its provider, through the person's own `claude` — found on the machine where
//! the host names none — each account's files in the folder the host gives; and what a conversation held let go of as
//! its thread is deleted.

use std::path::PathBuf;
use std::sync::Arc;

use octoii_ai::models::Provider;
use siphon::{Extension, Registrar};

use crate::account::{self, Place};
use crate::driver::ClaudeCode;

/// Claude Code, as a host adds it: the person's `claude` — the one on the machine, where none — and the extension's own
/// folder, which its `claude` reaches though the host keeps it from the threads.
#[derive(Debug, Clone)]
pub struct ClaudeCodeExtension {
    pub program: Option<PathBuf>,
    pub folder: PathBuf,
}

impl Extension for ClaudeCodeExtension {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn title(&self) -> &'static str {
        "Claude Code"
    }

    fn description(&self) -> &'static str {
        "Claude's models through Claude Code, signed in with a Claude account."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        let program = self.program.or_else(|| account::found(registrar.machine().home.as_deref()));
        let place = Place {
            kernel: registrar.kernel().clone(),
            machine: registrar.machine().clone(),
            program,
            folder: self.folder,
        };
        let _ = std::fs::create_dir_all(place.work());
        let claude = Arc::new(ClaudeCode::new(place));
        let forgetting = Arc::clone(&claude);
        registrar.on_thread_deleted(move |thread| forgetting.forget_conversation(thread));
        registrar.provider(Provider::of(claude));
    }
}
