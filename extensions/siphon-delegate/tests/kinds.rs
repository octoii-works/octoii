//! Kind is fixed at creation and checked against the thread's metadata on every continuation.

use octoii::testing::{Beak, Called, Calling};
use octoii::tools::{CallStatus, Context, Effect, Refusal, Told, Tool};
use serde::Deserialize;
use serde_json::{Value, json};
use siphon_delegate::{DelegateArgs, DelegateTool};

/// Executes the real delegate tool but lets this model-free test finish without awaiting a child's reply.
struct Dispatch;
impl Tool for Dispatch {
    const NAME: &'static str = "delegate";
    const DESCRIPTION: &'static str = "Dispatch a task";
    const REACHES_THREADS: bool = true;
    type Args = DelegateArgs;
    fn effect(&self, _: &Self::Args, _: &octoii::machine::Base) -> Effect {
        Effect::Delegates
    }
    async fn exec(&self, context: &Context, args: Self::Args) -> Result<Told, Refusal> {
        let mut told = DelegateTool.exec(context, args).await?;
        told.plan.awaits = None;
        Ok(told)
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
struct InspectArgs {
    thread: String,
}
struct Inspect;
impl Tool for Inspect {
    const NAME: &'static str = "inspect";
    const DESCRIPTION: &'static str = "Inspect a listed thread through Octoii's public interface";
    type Args = InspectArgs;
    fn effect(&self, _: &Self::Args, _: &octoii::machine::Base) -> Effect {
        Effect::Reads
    }
    fn exec(&self, context: &Context, args: Self::Args) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(
            octoii::threads::list(&context.volume())
                .map_err(|error| Refusal(error.to_string()))
                .map(|rows| {
                    let row = rows.into_iter().find(|row| row.id == args.thread).expect("the created child");
                    Told {
                        structured_content: Some(json!(row)),
                        ..Told::default()
                    }
                }),
        )
    }
}

fn calling(beak: &Beak) -> Calling {
    Calling::start(beak, |tools| tools.with(Dispatch).with(Inspect))
}

async fn called(calling: &Calling, folder: &std::path::Path, key: &str, tool: &str, args: Value) -> Called {
    calling
        .called(
            key,
            folder.to_owned(),
            Vec::new(),
            vec![(key.to_owned(), tool.to_owned(), args.to_string())],
        )
        .await
        .remove(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn creation_requires_a_kind_and_explorer_continuation_never_upgrades() {
    let beak = Beak::new().unwrap();
    let first = calling(&beak);
    let folder = tempfile::tempdir().unwrap();
    let workspace = folder.path().canonicalize().unwrap();
    let missing = called(
        &first,
        &workspace,
        "missing",
        "delegate",
        json!({ "action": "start", "task": "work" }),
    )
    .await;
    assert_eq!(missing.status, CallStatus::InvalidArguments);
    assert!(missing.text.contains("kind"), "{}", missing.text);
    let general = called(
        &first,
        &workspace,
        "general",
        "delegate",
        json!({ "action": "start", "task": "work", "kind": "general_purpose" }),
    )
    .await;
    assert_eq!(general.status, CallStatus::Ok, "{}", general.text);
    assert_eq!(general.structured_content.as_ref().unwrap()["kind"], "general_purpose");
    let started = called(
        &first,
        &workspace,
        "explorer",
        "delegate",
        json!({ "action": "start", "task": "read", "kind": "explorer" }),
    )
    .await;
    assert_eq!(started.status, CallStatus::Ok, "{}", started.text);
    let child = started.structured_content.as_ref().unwrap()["thread"].as_str().unwrap().to_owned();
    let before = called(&first, &workspace, "before", "inspect", json!({ "thread": child })).await;
    assert_eq!(before.structured_content.as_ref().unwrap()["parent"]["kind"], "explorer");
    for (key, args) in [
        ("continue", json!({ "action": "send", "task": "read again", "thread": child })),
        ("continue-again", json!({ "action": "send", "task": "read more", "thread": child })),
    ] {
        let continued = called(&first, &workspace, key, "delegate", args).await;
        assert_eq!(continued.status, CallStatus::Ok, "{}", continued.text);
        let data = continued.structured_content.unwrap();
        assert_eq!(data["type"], "continued");
        assert_eq!(data["kind"], "explorer");
    }
    let before_rejection = called(&first, &workspace, "queued", "inspect", json!({ "thread": child })).await;
    let changed = called(
        &first,
        &workspace,
        "upgrade",
        "delegate",
        json!({ "action": "send", "task": "write", "thread": child, "kind": "general_purpose" }),
    )
    .await;
    assert_eq!(changed.status, CallStatus::InvalidArguments);
    assert!(changed.text.contains("kind"), "{}", changed.text);
    assert_eq!(changed.structured_content, None);
    let after = called(&first, &workspace, "after", "inspect", json!({ "thread": child })).await;
    assert_eq!(
        after.structured_content, before_rejection.structured_content,
        "a rejected change delivers no task and changes no thread metadata"
    );
    first.close().await;
}
