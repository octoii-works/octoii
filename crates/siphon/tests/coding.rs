//! The coding tools and the shell, exec'd as a call's weighing let them: what each gives the model, the tools told
//! with their schemas, and a call's arguments checked against them.

use std::path::PathBuf;
use std::time::Duration;

use beak::Beak;
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde_json::json;
use siphon::machine::Host;
use siphon::machine::paths::Base;
use siphon::tools::{Context, EditTool, Effect, GlobTool, GrepTool, ReadTool, ShellTool, Tools, WriteTool, Writes};
use siphon_runtime::{Changes, Options, Runtime};
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
    changes.mkdir(siphon::threads::thread::path("t1"));
    runtime.commit(changes).await.expect("a thread's directory");
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    let context = Context {
        thread: "t1".into(),
        process: siphon::threads::thread::path("t1"),
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
        .output
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
    let past = exec(&tools, &context, "read", json!({ "path": "notes/a.txt", "offset": 9 })).await;
    assert_eq!(past, Ok("[the file has 5 lines, all before the offset]".into()));
    exec(&tools, &context, "write", json!({ "path": "notes/empty.md", "content": "" }))
        .await
        .expect("written");
    let empty = exec(&tools, &context, "read", json!({ "path": "notes/empty.md" })).await;
    assert_eq!(empty, Ok("[the file is empty]".into()));
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
    assert_eq!(many.expect_err("many"), "`old` matches 5 places: give more of it, or set `all`");
    assert_eq!(exec(&tools, &context, "read", json!({ "path": "." })).await, Ok("notes/\n".into()));
    assert_eq!(
        exec(&tools, &context, "glob", json!({ "pattern": "**/*.txt" })).await,
        Ok("notes/a.txt\n".into())
    );
    let found = exec(&tools, &context, "grep", json!({ "pattern": "four" })).await.expect("grepped");
    assert_eq!(found, "notes/a.txt:4: line four\n");
    let missing = exec(&tools, &context, "read", json!({ "path": "nowhere.txt" })).await;
    assert!(missing.expect_err("missing").starts_with("nothing is there"));
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
        .expect("read")
        .output;
    assert_eq!(shown[0], InputContent::text("shot.png, an image of 64×32"));
    let InputContent::InputImage { file, .. } = &shown[1] else {
        panic!("an image")
    };
    assert_eq!(file.media_type, "image/png");
    let kept = siphon::threads::files::read(&context.runtime.snapshot().expect("a snapshot"), "t1", &file.reference).expect("read");
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

#[tokio::test(flavor = "multi_thread")]
async fn a_call_writes_the_memory_of_its_project_and_a_command_its_scratchpad_as_its_tmpdir() {
    let beak = Beak::new().expect("Beak");
    let (mut context, _folder) = context(&beak, Writes::Workspace).await;
    let kept = tempfile::tempdir().expect("a folder");
    let kept = kept.path().canonicalize().expect("its real path");
    context.host.memory = Some(kept.join("memory"));
    context.host.scratchpads = Some(kept.join("scratch"));
    let memory = context.host.memory_of(&context.base.workspace).expect("its memory");
    let tools = tools();
    let index = memory.join("MEMORY.md");
    let wrote = exec(
        &tools,
        &context,
        "write",
        json!({ "path": index, "content": "- [a](a.md) — one\n" }),
    )
    .await;
    assert!(wrote.is_ok(), "the first memory makes its folder: {wrote:?}");
    let line = format!("echo kept > \"$TMPDIR/note\" && echo remembered > {}/a.md", memory.display());
    let ran = exec(&tools, &context, "shell", json!({ "command": line })).await.expect("ran");
    assert_eq!(ran, "[exit 0]");
    let note = kept.join("scratch/t1/note");
    assert_eq!(std::fs::read_to_string(note).expect("in the scratchpad"), "kept\n");
    assert_eq!(std::fs::read_to_string(memory.join("a.md")).expect("in the memory"), "remembered\n");
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

#[tokio::test(flavor = "multi_thread")]
async fn a_command_a_write_and_a_read_tell_what_they_did_typed_beside_what_the_model_reads() {
    let beak = Beak::new().expect("Beak");
    let (context, _folder) = context(&beak, Writes::Workspace).await;
    let structured = |name: &'static str, arguments: serde_json::Value| {
        let context = context.clone();
        async move {
            tools()
                .exec(name, context, arguments)
                .await
                .expect("done")
                .structured
                .expect("told typed")
        }
    };
    assert_eq!(
        structured("shell", json!({ "command": "echo hi; exit 3" })).await,
        json!({ "type": "ran", "ended": { "type": "exited", "code": 3 } })
    );
    assert_eq!(
        structured("write", json!({ "path": "notes.md", "content": "one\ntwo\nthree\n" })).await,
        json!({ "type": "changed", "path": "notes.md", "created": true, "additions": 3, "deletions": 0 })
    );
    assert_eq!(
        structured("read", json!({ "path": "notes.md" })).await,
        json!({ "type": "read", "path": "notes.md", "from": 1, "to": 3, "total": 3 })
    );
    let globbed = tools()
        .exec("glob", context.clone(), json!({ "pattern": "*.md" }))
        .await
        .expect("done");
    assert_eq!(globbed.structured, None, "a search's lines are what it found, told as they are");
}
