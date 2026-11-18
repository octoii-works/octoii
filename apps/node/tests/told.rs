//! Everything the node's model is told before a turn — its sections, then each tool's name, description and
//! parameters — kept as one snapshot, so every change of what the model reads shows in review as it reads it.

use std::fmt::Write as _;

use octoii::threads::{Config, NewThread};
use octoii_ai::vocabulary::Tool;
use octoii_node::home::Home;

/// `told` as one text: each section under its name, then each tool, its parameters as their JSON Schema.
fn shown(told: &octoii::agent::Told) -> String {
    let mut out = String::new();
    for (name, text) in &told.sections {
        let _ = write!(out, "=== section {name}\n{text}\n\n");
    }
    for tool in &told.tools {
        match tool {
            Tool::Function {
                name,
                description,
                parameters,
            } => {
                let parameters = serde_json::to_string_pretty(parameters).expect("JSON");
                let _ = write!(out, "=== tool {name}\n{description}\n--- parameters\n{parameters}\n\n");
            }
            Tool::WebSearch => out.push_str("=== tool web search\n\n"),
        }
    }
    out
}

/// `text` with what differs from one run, machine or day to the next put as a placeholder: the thread's id, the
/// test's folder — as a path and as a memory's folder spells it — the Home's temporary folder, the system's, the
/// platform and the date.
fn steady(text: &str, id: &str, folder: &std::path::Path, temporary: &std::path::Path) -> String {
    let real = folder.canonicalize().expect("its path");
    let spelt = |path: &std::path::Path| path.display().to_string().replace(['/', '\\', ':'], "-");
    text.replace(id, "<thread>")
        .replace(&temporary.display().to_string(), "<temporary>")
        .replace(&spelt(&real), "-<folder>")
        .replace(&spelt(folder), "-<folder>")
        .replace(&real.display().to_string(), "<folder>")
        .replace(&folder.display().to_string(), "<folder>")
        .replace(&std::env::temp_dir().display().to_string(), "<system temporary>/")
        .lines()
        .map(|line| {
            if line.starts_with("platform: ") {
                "platform: <platform>".to_owned()
            } else if line.starts_with("date: ") {
                "date: <date> (UTC)".to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test(flavor = "multi_thread")]
async fn what_the_model_is_told() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path().join("home"));
    home.prepare().expect("a Home");
    std::fs::write(home.config().join("AGENTS.md"), "The person's own instructions.").expect("theirs");
    let skill = home.config().join("skills/release");
    std::fs::create_dir_all(&skill).expect("a skill's folder");
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: release\ndescription: How a release is made.\n---\nThe steps.",
    )
    .expect("a skill");
    let workspace = folder.path().join("project");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    std::fs::write(workspace.join("AGENTS.md"), "The project's instructions.").expect("the project's");
    let memory = home.memory().join(workspace.display().to_string().replace(['/', '\\', ':'], "-"));
    std::fs::create_dir_all(&memory).expect("the project's memory");
    std::fs::write(memory.join("MEMORY.md"), "- [Tests](tests.md) — the suite runs with `cargo test`\n").expect("its index");
    let siphon = octoii_node::serve::siphon(&home, octoii::providers::SignInSetup::default()).expect("Octoii");
    let new = NewThread {
        actor: "person".into(),
        config: Config {
            workspace: Some(workspace.display().to_string()),
            ..Config::default()
        },
        ..NewThread::default()
    };
    let key = siphon.volume().new_id().expect("an id");
    let id = siphon.threads.create(&new, &key).await.expect("a thread");
    let told = siphon.told(&id).await.expect("told");
    assert_eq!(told.sections.first().map(|(name, _)| name.as_str()), Some("base"));
    assert_eq!(told.sections.iter().filter(|(name, _)| name == "base").count(), 1);
    let temporary = home.temporary().expect("the Home's temporary folder");
    insta::assert_snapshot!(steady(&shown(&told), &id, folder.path(), &temporary));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extension_switched_off_tells_the_model_nothing() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path().join("home"));
    home.prepare().expect("a Home");
    let siphon = octoii_node::serve::siphon(&home, octoii::providers::SignInSetup::default()).expect("Octoii");
    let new = NewThread {
        actor: "person".into(),
        config: Config {
            workspace: Some(folder.path().display().to_string()),
            ..Config::default()
        },
        ..NewThread::default()
    };
    let key = siphon.volume().new_id().expect("an id");
    let id = siphon.threads.create(&new, &key).await.expect("a thread");
    let names = |told: &octoii::agent::Told| -> Vec<String> {
        told.tools
            .iter()
            .filter_map(|tool| match tool {
                Tool::Function { name, .. } => Some(name.clone()),
                Tool::WebSearch => None,
            })
            .collect()
    };
    let before = names(&siphon.told(&id).await.expect("told"));
    assert!(before.iter().any(|name| name == "browser") && before.iter().any(|name| name == "terminal"));
    let change = octoii::settings::SettingsChange {
        environment: Some(octoii::settings::EnvironmentChange {
            disabled_extensions: Some(
                ["browser", "terminals", "files", "thread", "delegate", "question", "memory"]
                    .map(str::to_owned)
                    .to_vec(),
            ),
            ..octoii::settings::EnvironmentChange::default()
        }),
        device: None,
    };
    let mut client = octoii::testing::Client::connect(&siphon, vec![octoii::protocol::Scope::ThreadsOperate], 8);
    client.message().await;
    let call = client.call("settings.set", serde_json::to_value(change).expect("settings")).await;
    let answer = client.answer(&call).await;
    assert!(answer.get("result").is_some(), "{answer}");
    let told = siphon.told(&id).await.expect("told");
    let environment = told
        .sections
        .iter()
        .find(|(name, _)| name == "environment")
        .map(|(_, text)| text.clone());
    assert!(!told.sections.iter().any(|(name, _)| name == "memory"), "memory is told no more");
    assert!(!environment.unwrap_or_default().contains("memory folder"), "nor is its folder");
    let after = names(&told);
    assert!(
        !after.iter().any(|name| name == "browser"),
        "the browser is told no more: {after:?}"
    );
    for section in ["browser", "terminals", "files", "thread", "threads", "delegate", "question"] {
        assert!(!told.sections.iter().any(|(name, _)| name == section), "disabled section {section}");
    }
    for tool in ["browser", "terminal", "present_file", "thread", "delegate", "question"] {
        assert!(!after.iter().any(|name| name == tool), "disabled tool {tool}");
    }
    assert!(after.iter().any(|name| name == "read"));
}

#[tokio::test(flavor = "multi_thread")]
async fn product_context_does_not_grant_subthread_tools() {
    let folder = tempfile::tempdir().unwrap();
    let home = Home::at(folder.path().join("home"));
    home.prepare().unwrap();
    let siphon = octoii_node::serve::siphon(&home, octoii::providers::SignInSetup::default()).unwrap();
    let key = siphon.volume().new_id().unwrap();
    let parent = siphon.threads.create(&NewThread::default(), &key).await.unwrap();
    for kind in [
        octoii::threads::SubThreadKind::Explorer,
        octoii::threads::SubThreadKind::GeneralPurpose,
    ] {
        let key = siphon.volume().new_id().unwrap();
        let child = siphon
            .threads
            .create(
                &NewThread {
                    parent: Some(octoii::threads::Parent {
                        thread: parent.clone(),
                        call: key.clone(),
                        kind,
                    }),
                    ..NewThread::default()
                },
                &key,
            )
            .await
            .unwrap();
        let told = siphon.told(&child).await.unwrap();
        let names: Vec<_> = told
            .tools
            .iter()
            .filter_map(|tool| match tool {
                Tool::Function { name, .. } => Some(name.as_str()),
                Tool::WebSearch => None,
            })
            .collect();
        assert!(!names.contains(&"thread") && !names.contains(&"delegate"));
        if kind == octoii::threads::SubThreadKind::Explorer {
            assert!(names.contains(&"read") && names.contains(&"execute"));
            assert!(names.iter().all(|name| ["read", "glob", "grep", "execute"].contains(name)));
        }
    }
}
