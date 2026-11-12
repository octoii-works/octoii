//! The environment's settings shared, its defaults where none was set; a device's changed as a merge patch, bounded,
//! and let go of.

use crate::settings::{Environment, EnvironmentChange, Error, SettingsChange, device, environment, forget, set};
use crate::threads::Mode;
use beak::Beak;
use serde_json::json;
use siphon_runtime::{Changes, Options, Runtime};

#[tokio::test(flavor = "multi_thread")]
async fn the_environment_s_settings_are_shared_and_a_device_s_merged_as_a_patch() {
    let beak = Beak::new().expect("Beak");
    let runtime = Runtime::builder().start(&beak, Options::default()).expect("started");
    assert_eq!(
        environment(&runtime.snapshot().expect("a snapshot")).expect("read"),
        Environment::default()
    );
    let change = SettingsChange {
        environment: Some(EnvironmentChange {
            thread_mode: Some(Mode::Auto),
            review_model: Some(Some("a/judge".into())),
            history_retention_days: Some(None),
            ..EnvironmentChange::default()
        }),
        device: Some(json!({ "theme": "dark", "panes": { "left": 280 } })),
    };
    set(&runtime, "s1", &change).await.expect("set");
    let shared = environment(&runtime.snapshot().expect("a snapshot")).expect("read");
    assert_eq!(
        (shared.thread_mode, shared.review_model.as_deref(), shared.history_retention_days),
        (Mode::Auto, Some("a/judge"), None),
        "kept forever once the person said so"
    );
    let patch = SettingsChange {
        environment: Some(EnvironmentChange {
            review_model: Some(None),
            ..EnvironmentChange::default()
        }),
        device: Some(json!({ "theme": null, "panes": { "right": 300 } })),
    };
    set(&runtime, "s1", &patch).await.expect("set");
    let snapshot = runtime.snapshot().expect("a snapshot");
    assert_eq!(environment(&snapshot).expect("read").review_model, None);
    assert_eq!(
        device(&snapshot, "s1").expect("read"),
        json!({ "panes": { "left": 280, "right": 300 } })
    );
    assert_eq!(device(&snapshot, "s2").expect("read"), json!({}), "another device's are its own");
    let large = SettingsChange {
        device: Some(json!({ "notes": "x".repeat(70_000) })),
        ..SettingsChange::default()
    };
    assert!(matches!(set(&runtime, "s1", &large).await, Err(Error::TooLarge)));
    assert!(matches!(set(&runtime, "s1", &SettingsChange::default()).await, Err(Error::Nothing)));
    assert!(matches!(set(&runtime, "../x", &patch).await, Err(Error::InvalidSession)));
    let mut changes = Changes::new();
    forget(&snapshot, &mut changes, "s1").expect("forgotten");
    runtime.commit(changes).await.expect("committed");
    assert_eq!(device(&runtime.snapshot().expect("a snapshot"), "s1").expect("read"), json!({}));
    runtime.close().await;
}
