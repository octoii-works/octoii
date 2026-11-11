//! The coding tools and the shell, exec'd as a call's weighing let them: what each gives the model, the tools told
//! with their schemas, and a call's arguments checked against them.

use std::path::PathBuf;
use std::time::Duration;

use beak::Beak;
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde_json::json;
use siphon_machine::Host;
use siphon_machine::paths::Base;
use siphon_runtime::{Changes, Options, Runtime};
use siphon_tools::{Context, EditTool, Effect, GlobTool, GrepTool, ReadTool, ShellTool, Tools, WriteTool, Writes};
use tokio_util::sync::CancellationToken;

fn tools() -> Tools {
    Tools::new()
        .with(ReadTool)
        .with(WriteTool)
        .with(EditTool)
        .with(GlobTool)
        .with(GrepTool)
        .with(ShellTool)
}

/// A context working in a fresh folder for the thread `t1`, writing as `writes` says; the folder held.
async fn context(beak: &Beak, writes: Writes) -> (Context, tempfile::TempDir) {
    let runtime = Runtime::builder().start(beak, Options::default()).expect("started");
    let mut changes = Changes::new();
    changes.mkdir(siphon_threads::thread::path("t1"));
    runtime.commit(changes).await.expect("a thread's directory");
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    let context = Context {
        thread: "t1".into(),
        process: siphon_threads::thread::path("t1"),
        call: "c1".into(),
        base: Base { workspace, home: None },
        writes,
        kernel: runtime.kernel().clone(),
        host: Host::default(),
        runtime,
        cancelled: CancellationToken::new(),
    };
    (context, folder)
}

async fn exec(tools: &Tools, context: &Context, name: &str, arguments: serde_json::Value) -> Result<String, String> {
    tools.effect(name, &arguments, &context.base).map_err(|refusal| refusal.0)?;
    let given = tools.exec(name, context.clone(), arguments).await.map_err(|refusal| refusal.0)?;
    Ok(given
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[tokio::test(flavor = "multi_thread")]
async fn files_are_written_edited_read_a_page_at_a_time_and_found() {
    let beak = Beak::new().expect("Beak");
    let (context, _folder) = context(&beak, Writes::Workspace).await;
    let tools = tools();
    let lines = "line 1\nline 2\nline 3\nline 4\nline 5\n";
    assert_eq!(
        exec(&tools, &context, "write", json!({ "path": "notes/a.txt", "content": lines })).await,
        Ok("made notes/a.txt: +5 -0".into())
    );
    let page = exec(&tools, &context, "read", json!({ "path": "notes/a.txt", "offset": 2, "limit": 2 }))
        .await
        .expect("read");
    assert_eq!(page, "line 2\nline 3\n[lines 2-3; more from offset 4]");
    let edited = exec(
        &tools,
        &context,
        "edit",
        json!({ "path": "notes/a.txt", "old": "line 4", "new": "line four" }),
    )
    .await
    .expect("edited");
    assert!(edited.contains("-line 4\n+line four"), "{edited}");
    let many = exec(
        &tools,
        &context,
        "edit",
        json!({ "path": "notes/a.txt", "old": "line", "new": "row" }),
    )
    .await;
    assert!(many.expect_err("many").starts_with("many: 5 places match"));
    assert_eq!(exec(&tools, &context, "read", json!({ "path": "." })).await, Ok("notes/\n".into()));
    assert_eq!(
        exec(&tools, &context, "glob", json!({ "pattern": "**/*.txt" })).await,
        Ok("notes/a.txt\n".into())
    );
    let found = exec(&tools, &context, "grep", json!({ "pattern": "four" })).await.expect("grepped");
    assert_eq!(found, "notes/a.txt:4: line four\n");
    let missing = exec(&tools, &context, "read", json!({ "path": "nowhere.txt" })).await;
    assert!(missing.expect_err("missing").starts_with("not-found"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_image_read_is_shown_as_an_image_kept_in_its_thread() {
    let beak = Beak::new().expect("Beak");
    let (context, _folder) = context(&beak, Writes::Workspace).await;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(64, 32, image::Rgb([200, 10, 10]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a PNG");
    std::fs::write(context.base.workspace.join("shot.png"), &png).expect("written");
    let shown = tools()
        .exec("read", context.clone(), json!({ "path": "shot.png" }))
        .await
        .expect("read");
    assert_eq!(shown[0], InputContent::text("shot.png, an image of 64×32"));
    let InputContent::InputImage { file, .. } = &shown[1] else {
        panic!("an image")
    };
    assert_eq!(file.media_type, "image/png");
    let kept = siphon_threads::files::read(&context.runtime.snapshot().expect("a snapshot"), "t1", &file.reference).expect("read");
    assert_eq!(kept, Some(Bytes::from(png)), "kept by its content in the thread");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_line_runs_in_the_workspace_held_there_and_tells_how_it_ended() {
    let beak = Beak::new().expect("Beak");
    let (context, _folder) = context(&beak, Writes::Workspace).await;
    let tools = tools();
    let ran = exec(
        &tools,
        &context,
        "shell",
        json!({ "command": "echo out; echo err >&2; pwd; exit 3" }),
    )
    .await
    .expect("ran");
    let workspace = context.base.workspace.display().to_string();
    assert_eq!(ran, format!("out\nerr\n{workspace}\n[exit 3]"));
    let slow = exec(&tools, &context, "shell", json!({ "command": "sleep 5", "timeout_seconds": 1 }))
        .await
        .expect("ran");
    assert_eq!(slow, "[its 1 s ran out; it was ended]");
    if beak::machine_confines() {
        let outside = tempfile::tempdir().expect("a folder");
        let target = outside.path().canonicalize().expect("its real path").join("x");
        let held = exec(
            &tools,
            &context,
            "shell",
            json!({ "command": format!("echo no > {}", target.display()) }),
        )
        .await
        .expect("ran");
        assert!(!held.ends_with("[exit 0]"), "{held}");
        assert!(!target.exists());
    }
    let effect = tools.effect(
        "shell",
        &json!({ "command": "brew install jq", "escalate": true, "justification": "to install jq" }),
        &context.base,
    );
    assert_eq!(
        effect,
        Ok(Effect::Runs {
            line: "brew install jq".into(),
            outside_sandbox: true,
            justification: Some("to install jq".into()),
        })
    );
    let _ = Duration::ZERO;
}

#[test]
fn the_tools_are_told_with_their_schemas_and_a_call_s_arguments_checked_against_them() {
    let tools = tools();
    let names: Vec<String> = tools.declared().into_iter().map(|declared| declared.name).collect();
    assert_eq!(names, ["edit", "glob", "grep", "read", "shell", "write"]);
    let read = tools.declared().into_iter().find(|declared| declared.name == "read").expect("read");
    assert_eq!(read.parameters["required"], json!(["path"]));
    let base = Base {
        workspace: PathBuf::from("/work"),
        home: None,
    };
    let wrong = tools.effect("read", &json!({ "offset": 0 }), &base).expect_err("refused");
    assert!(wrong.0.starts_with("invalid arguments"), "{}", wrong.0);
    assert!(tools.effect("nonesuch", &json!({}), &base).is_err());
    assert_eq!(
        tools.effect("write", &json!({ "path": "a.txt", "content": "" }), &base),
        Ok(Effect::Edits(vec![PathBuf::from("/work/a.txt")]))
    );
}
