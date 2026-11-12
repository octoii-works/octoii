//! The person's own Claude Code, reached for real: how their account stands and the models it offers with their
//! windows, read without asking a model anything. Run by hand, on a computer where `claude` is signed in:
//! `cargo test -p siphon-claude-code --test live -- --ignored`.

use std::sync::Arc;

use octoii_ai::driver::{Call, Driver, Settings};
use octoii_ai::host::{Credential, Credentials, Unavailable};
use siphon_claude_code::{ClaudeCode, ID, Place};

struct Nothing;

impl Credentials for Nothing {
    fn read(&self, _: &str) -> Result<Option<Credential>, Unavailable> {
        Ok(None)
    }

    fn keep(&self, _: &str, _: &Credential) -> Result<(), Unavailable> {
        Ok(())
    }

    fn forget(&self, _: &str) -> Result<(), Unavailable> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "reaches the person's own Claude Code"]
async fn the_persons_own_claude_code_tells_how_it_stands_and_what_it_offers() {
    let folder = tempfile::tempdir().expect("a folder");
    let beak = beak::Beak::new().expect("Beak");
    let kernel = beak::Kernel::open(&beak, &beak::Options::default()).expect("a kernel").0;
    let home = std::env::home_dir();
    let place = Place {
        kernel,
        machine: siphon_machine::Host {
            home: home.clone(),
            ..siphon_machine::Host::default()
        },
        program: siphon_claude_code::found(home.as_deref()),
        folder: folder.path().canonicalize().expect("its real path"),
    };
    std::fs::create_dir_all(place.work()).expect("a work folder");
    let claude = ClaudeCode::new(place);
    let call = Call {
        provider: ID.into(),
        settings: Settings::default(),
        credentials: Arc::new(Nothing),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        files: None,
        now_ms: 0,
    };
    let standing = claude.status(&call).await.expect("told").expect("a standing");
    assert!(standing.account.is_some(), "signed in: {:?}", standing.guidance);
    println!("plan {:?}, limits {:?}", standing.plan, standing.limits);
    let models = claude.list(&call).await.expect("listed").expect("a listing");
    assert_ne!(models.len(), 0, "models offered");
    for model in &models {
        println!("{} {:?} {:?}", model.id, model.name, model.context_length);
    }
    assert!(models.iter().any(|model| model.context_length.is_some()), "windows told");
}
