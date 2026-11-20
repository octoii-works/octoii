//! Base reads final runtime configuration even when it is registered before product extensions.
use octoii::protocol::Scope;
use octoii::testing;
use octoii::threads::{Config, Mode, NewThread};
use octoii::{Extension, Octoii, Registrar};
use siphon_base::BaseExtension;
use std::path::PathBuf;

struct WorkspaceNotes(PathBuf);
impl Extension for WorkspaceNotes {
    fn name(&self) -> &'static str {
        "notes"
    }
    fn title(&self) -> &'static str {
        "Notes"
    }
    fn description(&self) -> &'static str {
        "Test writable directory"
    }
    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.writable("notes folder", self.0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn base_reads_late_registered_folders_and_tracks_extension_settings() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().canonicalize().unwrap();
    let user = workspace.join("user");
    std::fs::create_dir(&user).unwrap();
    std::fs::write(user.join("AGENTS.md"), "USER_INSTRUCTION").unwrap();
    std::fs::write(workspace.join("AGENTS.md"), "PROJECT_INSTRUCTION").unwrap();
    let siphon = Octoii::builder(testing::setup(octoii::machine::Host::default()))
        .unwrap()
        .extension(BaseExtension { config: Some(user) })
        .extension(WorkspaceNotes(workspace.join("notes")))
        .start()
        .unwrap();
    let key = siphon.volume().new_id().unwrap();
    let id = siphon
        .threads
        .create(
            &NewThread {
                actor: "person".into(),
                project: Some(workspace.display().to_string()),
                config: Config {
                    workspace: Some(workspace.display().to_string()),
                    mode: Some(Mode::FullAccess),
                    ..Config::default()
                },
                ..NewThread::default()
            },
            &key,
        )
        .await
        .unwrap();
    let told = siphon.told(&id).await.unwrap();
    assert_eq!(
        told.sections.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
        ["base", "instructions", "environment"]
    );
    let instructions = &told.sections[1].1;
    assert!(instructions.contains("USER_INSTRUCTION"));
    assert!(instructions.contains("PROJECT_INSTRUCTION"));
    let environment = &told.sections[2].1;
    assert!(environment.contains("notes folder:"));
    assert!(environment.contains("permissions: full-access"));
    assert!(!environment.contains("git:"));
    let change = octoii::settings::SettingsChange {
        environment: Some(octoii::settings::EnvironmentChange {
            disabled_extensions: Some(vec!["notes".into()]),
            ..Default::default()
        }),
        device: None,
    };
    let mut client = testing::Client::connect(&siphon, vec![Scope::ThreadsOperate], 8);
    client.message().await;
    let answer = client.command("settings.set", serde_json::to_value(change).unwrap()).await;
    assert!(answer.get("result").is_some(), "{answer}");
    let told = siphon.told(&id).await.unwrap();
    assert!(!told.sections[2].1.contains("notes folder:"));
    siphon.close().await;
}
