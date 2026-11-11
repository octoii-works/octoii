//! Claude Code as a plugin of Siphon's: its provider, through the person's own `claude` — found on the machine where
//! the host names none — each account's files in the folder the host gives; and what a conversation held let go of as
//! its thread is deleted.

use std::path::PathBuf;
use std::sync::Arc;

use octoii_ai::models::Provider;
use siphon::{Builder, Plugin};

use crate::account::{self, Place};
use crate::driver::ClaudeCode;

/// Claude Code, as a host adds it: the person's `claude` — the one on the machine, where none — and the plugin's own
/// folder, which its `claude` reaches though the host keeps it from the threads.
#[derive(Debug, Clone)]
pub struct ClaudeCodePlugin {
    pub program: Option<PathBuf>,
    pub folder: PathBuf,
}

impl Plugin for ClaudeCodePlugin {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn build(self: Box<Self>, builder: &mut Builder) {
        let program = self.program.or_else(|| account::found(builder.machine.home.as_deref()));
        let place = Place {
            kernel: builder.kernel.clone(),
            machine: builder.machine.clone(),
            program,
            folder: self.folder,
        };
        let _ = std::fs::create_dir_all(place.work());
        let claude = Arc::new(ClaudeCode::new(place));
        let forgetting = Arc::clone(&claude);
        builder.on_thread_deleted(move |thread| forgetting.forget_conversation(thread));
        builder.provider(Provider::of(claude));
    }
}
