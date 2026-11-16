//! Files presented as real model calls: a saved resource link, a separate display title, result replay after the
//! source disappears, and failed calls that never return a resource.

use octoii_ai::vocabulary::InputContent;
use serde_json::json;
use siphon::machine::Base;
use siphon::testing::{Beak, Called, Calling};
use siphon::threads::files;
use siphon::tools::{CallStatus, Effect, Tool, Tools};
use siphon_files::{PresentFileArgs, PresentFileTool};

async fn called(calling: &Calling, key: &str, workspace: &std::path::Path, arguments: Vec<serde_json::Value>) -> Vec<Called> {
    let calls = arguments
        .into_iter()
        .enumerate()
        .map(|(at, args)| (format!("file-{at}"), PresentFileTool::NAME.to_owned(), args.to_string()))
        .collect();
    calling
        .called(key, workspace.to_owned(), vec![PresentFileTool::NAME.to_owned()], calls)
        .await
}

#[test]
fn file_presentation_is_an_action_and_its_schema_requires_a_path() {
    let base = Base {
        workspace: "/work".into(),
        home: None,
    };
    assert_eq!(
        PresentFileTool.effect(
            &PresentFileArgs {
                path: "report.pdf".into(),
                title: None,
            },
            &base,
        ),
        Effect::Acts {
            tool: "present_file".into(),
        }
    );
    let tools = Tools::new().with(PresentFileTool);
    assert!(tools.effect("present_file", &json!({}), &base).is_err());
    assert!(tools.effect("present_file", &json!({ "path": "" }), &base).is_err());
    assert_eq!(PresentFileTool::output_schema(), None, "the resource is held once in content");
}

#[tokio::test(flavor = "multi_thread")]
async fn presented_files_keep_original_bytes_and_committed_results_remain_readable_without_the_sources() {
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    let originals = [
        ("report.pdf", b"%PDF-1.7\noriginal\n%%EOF\n".as_slice(), "application/pdf"),
        ("shot.png", b"\x89PNG\r\n\x1a\n\0original image bytes".as_slice(), "image/png"),
        ("notes.txt", b"\xef\xbb\xbfone\r\ntwo\r\n".as_slice(), "text/plain"),
        ("archive.zip", b"PK\x03\x04\0\0original archive bytes".as_slice(), "application/zip"),
    ];
    for (name, bytes, _) in &originals {
        std::fs::write(workspace.join(name), bytes).expect("written");
    }
    let beak = Beak::new().expect("Beak");
    let calling = Calling::start(&beak, |tools| tools.with(PresentFileTool));
    let arguments: Vec<_> = originals
        .iter()
        .map(|(name, _, _)| json!({ "path": name, "title": "Verification result" }))
        .collect();
    let presented = called(&calling, "presented", &workspace, arguments).await;
    for ((name, bytes, media_type), result) in originals.iter().zip(&presented) {
        assert_eq!(result.status, CallStatus::Ok, "{}", result.text);
        assert_eq!(result.metadata, Some(json!({ "title": "Verification result" })));
        assert_eq!(result.structured_content, None);
        assert_eq!(result.images, 0, "the original image is a link, not model image input");
        let [InputContent::InputText { text }, InputContent::ResourceLink { file, size }] = result.content.as_slice() else {
            panic!("a short explanation and resource link: {:?}", result.content)
        };
        assert_eq!(text, &format!("Presented {name} ({} bytes).", bytes.len()));
        assert_eq!(file.filename.as_deref(), Some(*name), "the title never renames the file");
        assert_eq!(file.media_type, *media_type);
        assert_eq!(file.reference, files::reference_of(bytes));
        assert_eq!(*size, bytes.len() as u64);
        let snapshot = calling.volume().snapshot().expect("a snapshot");
        assert_eq!(
            files::read(&snapshot, "t1", &file.reference).expect("the stored file").as_deref(),
            Some(*bytes)
        );
        let json = serde_json::to_value(&result.content).expect("serialized for persistence");
        assert_eq!(json[1]["type"], "resource_link");
        assert_eq!(
            serde_json::from_value::<Vec<InputContent>>(json).expect("read back"),
            result.content
        );
        std::fs::write(workspace.join(name), "changed").expect("changed");
        std::fs::remove_file(workspace.join(name)).expect("removed");
    }
    let snapshot = calling.volume().snapshot().expect("a snapshot after removing the sources");
    for ((_, bytes, _), result) in originals.iter().zip(&presented) {
        let InputContent::ResourceLink { file, .. } = &result.content[1] else {
            panic!("a persisted resource link")
        };
        assert_eq!(
            files::read(&snapshot, "t1", &file.reference).expect("read").as_deref(),
            Some(*bytes)
        );
    }
    calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_presentations_return_no_success_resource_or_title() {
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    let large = std::fs::File::create(workspace.join("large.bin")).expect("a file");
    large.set_len(files::BYTES_MAX as u64 + 1).expect("too large");
    let beak = Beak::new().expect("Beak");
    let calling = Calling::start(&beak, |tools| tools.with(PresentFileTool));
    let results = called(
        &calling,
        "failed",
        &workspace,
        vec![
            json!({ "path": ".", "title": "A directory" }),
            json!({ "path": "missing.txt", "title": "Missing" }),
            json!({ "path": "large.bin", "title": "Too large" }),
            json!({ "path": "bad\0path", "title": "Invalid" }),
        ],
    )
    .await;
    for result in results {
        assert_eq!(result.status, CallStatus::ToolError, "{}", result.text);
        assert!(result.content.iter().all(|part| matches!(part, InputContent::InputText { .. })));
        assert_eq!(result.metadata, None);
    }
    assert!(
        calling
            .volume()
            .snapshot()
            .expect("a snapshot")
            .stat(b"siphon.thread:t1/files")
            .expect("read")
            .is_none(),
        "no failed call stores a blob"
    );
    calling.close().await;
}
