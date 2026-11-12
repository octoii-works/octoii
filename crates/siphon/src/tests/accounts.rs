//! Accounts added of the host's providers, each another instance of one, renamed and removed; a provider's quota used
//! up kept on its directory, an account's or the host's own.

use crate::accounts::{Error, UsedUp, accounts, add, keep_used_up, provider_of, providers, remove, rename, used_up};
use beak::Beak;
use octoii_ai::drivers::{OLLAMA, OPENAI};
use octoii_ai::models::Provider;
use siphon_runtime::{Changes, Options, Runtime};

#[tokio::test(flavor = "multi_thread")]
async fn an_account_is_another_instance_of_a_host_s_provider_and_a_quota_used_up_stands_on_its_directory() {
    let beak = Beak::new().expect("Beak");
    let runtime = Runtime::builder().start(&beak, Options::default()).expect("started");
    let host = [Provider::of(std::sync::Arc::new(OPENAI)), Provider::of(std::sync::Arc::new(OLLAMA))];
    let work = add(&runtime, &host, "openai", "  Work  ", "add-1").await.expect("added");
    assert_eq!(
        add(&runtime, &host, "openai", "Other", "add-1").await.expect("added"),
        work,
        "added once for its key"
    );
    assert!(matches!(
        add(&runtime, &host, "ollama", "Home", "add-2").await,
        Err(Error::TakesNoAccounts)
    ));
    assert!(matches!(
        add(&runtime, &host, "nobody", "Home", "add-3").await,
        Err(Error::UnknownProvider(_))
    ));
    assert!(matches!(
        add(&runtime, &host, "openai", "   ", "add-4").await,
        Err(Error::InvalidName)
    ));
    let added = accounts(&runtime.snapshot().expect("a snapshot")).expect("listed");
    assert_eq!(added.len(), 1);
    assert_eq!((added[0].provider.as_str(), added[0].name.as_str()), ("openai", "Work"));
    let reached = providers(&host, &added);
    let instance = reached.iter().find(|provider| provider.id == work).expect("the account's provider");
    assert_eq!((instance.name.as_str(), instance.catalogue.as_str()), ("Work", "openai"));
    rename(&runtime, &work, "Office").await.expect("renamed");
    assert_eq!(
        accounts(&runtime.snapshot().expect("a snapshot")).expect("listed")[0].name,
        "Office"
    );

    let spent = UsedUp {
        reset_at_ms: runtime.now_ms() + 3_600_000,
        window_minutes: Some(300),
    };
    for provider in ["openai", work.as_str()] {
        let mut changes = Changes::new();
        keep_used_up(&runtime.snapshot().expect("a snapshot"), &mut changes, provider, spent).expect("kept");
        runtime.commit(changes).await.expect("committed");
    }
    let snapshot = runtime.snapshot().expect("a snapshot");
    assert_eq!(used_up(&snapshot, "openai").expect("read"), Some(spent));
    assert_eq!(used_up(&snapshot, &work).expect("read"), Some(spent));
    assert_eq!(used_up(&snapshot, "ollama").expect("read"), None);
    let listed = accounts(&snapshot).expect("listed");
    assert_eq!(listed.len(), 1, "the host's own provider is no account, the account still one");
    assert_eq!(listed[0].provider, "openai", "its label kept as its quota was used up");
    remove(&runtime, &work).await.expect("removed");
    assert_eq!(accounts(&runtime.snapshot().expect("a snapshot")).expect("listed"), [], "removed");
    assert!(
        matches!(remove(&runtime, "openai").await, Err(Error::NoSuchAccount)),
        "the host's own is no account"
    );
    assert_eq!(provider_of("openai/gpt-6"), "openai");
    runtime.close().await;
}
