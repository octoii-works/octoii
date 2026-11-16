//! Original files kept through a call's namespace: no model image fitting, byte limits, denied paths, and the bytes
//! still kept after the source changes and the volume is reopened.

use crate::machine::{Base, Host};
use crate::threads::files;
use crate::tools::Context;
use crate::tools::permissions::Writes;
use beak::Beak;
use bytes::Bytes;
use siphon_runtime::{Changes, Options, Runtime};
use tokio_util::sync::CancellationToken;

async fn context(beak: &Beak, folder: &std::path::Path) -> Context {
    let workspace = folder.join("workspace");
    std::fs::create_dir(&workspace).expect("a workspace");
    let runtime = Runtime::builder()
        .start(
            beak,
            Options {
                directory: Some(folder.join("volume")),
                ..Options::default()
            },
        )
        .expect("started");
    let mut changes = Changes::new();
    changes.mkdir(crate::threads::thread::path("t1")).meta(
        crate::threads::thread::path("t1"),
        vec![(
            Bytes::from_static(crate::threads::thread::PROJECT_LABEL.as_bytes()),
            Some(Bytes::new()),
        )],
        vec![],
    );
    runtime.commit(changes).await.expect("a thread");
    Context {
        thread: "t1".into(),
        process: crate::threads::thread::path("t1"),
        call: "c1".into(),
        base: Base { workspace, home: None },
        writes: Writes::Workspace,
        places: Vec::new(),
        kernel: runtime.kernel().clone(),
        host: Host::default(),
        runtime,
        cancelled: CancellationToken::new(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn namespace_file_ids_distinguish_host_files_from_virtual_volume_files() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let beak = Beak::new().expect("Beak");
    let mut context = context(&beak, &root).await;
    context.host.threads = Some(root.join("threads"));
    let bytes = b"an original file";
    std::fs::write(context.base.workspace.join("original.txt"), bytes).expect("written");
    let (file, _) = context.keep_file("original.txt").await.expect("kept");
    let machine = context.machine().expect("the call's machine");
    let host = machine
        .namespace()
        .stat(&machine.path("original.txt"))
        .expect("stat")
        .expect("a host file");
    assert!(host.file_id.is_some());
    let virtual_path = root
        .join("threads/t1/files")
        .join(file.reference.strip_prefix("sha256:").expect("its hash"));
    assert!(!virtual_path.exists(), "there is no matching host file");
    let virtual_path = machine.path(virtual_path.to_str().expect("a path"));
    let volume = machine.namespace().stat(&virtual_path).expect("stat").expect("a virtual file");
    assert_eq!(volume.file_id, None);
    assert_eq!(
        machine
            .read_file(&virtual_path, files::BYTES_MAX as u64)
            .expect("read through the namespace")
            .1,
        bytes.as_slice()
    );
    context.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn original_files_are_content_addressed_and_survive_source_changes_and_a_reopen() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let beak = Beak::new().expect("Beak");
    let context = context(&beak, &root).await;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(4096, 2048, image::Rgb([200, 10, 10]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a PNG beyond the model's image bounds");
    let originals = [
        ("shot.png", png, "image/png"),
        ("notes.txt", b"\xef\xbb\xbfone\r\ntwo\r\n".to_vec(), "text/plain"),
        ("report.pdf", b"%PDF-1.7\noriginal bytes\n%%EOF\n".to_vec(), "application/pdf"),
        (
            "archive.zip",
            b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0".to_vec(),
            "application/zip",
        ),
        ("data.bin", b"\0\xff\x01\x02".to_vec(), "application/octet-stream"),
        ("empty.txt", Vec::new(), "text/plain"),
    ];
    let mut kept = Vec::new();
    for (name, bytes, media_type) in &originals {
        let source = context.base.workspace.join(name);
        std::fs::write(&source, bytes).expect("written");
        let (file, size) = context.keep_file(name).await.expect("kept");
        assert_eq!(file.reference, files::reference_of(bytes));
        assert_eq!(file.filename.as_deref(), Some(*name));
        assert_eq!(file.media_type, *media_type);
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(
            files::read(&context.runtime.snapshot().expect("a snapshot"), "t1", &file.reference).expect("read"),
            Some(Bytes::copy_from_slice(bytes)),
            "original bytes, including the large image, BOM and line endings"
        );
        std::fs::write(&source, b"changed").expect("changed");
        std::fs::remove_file(&source).expect("removed");
        kept.push(file);
    }
    let renamed = context.base.workspace.join("renamed.txt");
    std::fs::write(&renamed, &originals[1].1).expect("the same bytes with another name");
    let (same, _) = context.keep_file(renamed.to_str().expect("a path")).await.expect("kept");
    assert_eq!(same.reference, kept[1].reference);
    assert_eq!(same.filename.as_deref(), Some("renamed.txt"));
    let snapshot = context.runtime.snapshot().expect("a snapshot");
    let count = snapshot
        .list(
            &crate::threads::thread::within("t1", "files"),
            &beak::Range {
                limit: usize::MAX,
                ..beak::Range::default()
            },
        )
        .expect("files")
        .expect("the files folder")
        .len();
    assert_eq!(count, originals.len(), "one blob for each distinct content");
    drop(snapshot);
    context.runtime.close().await;
    drop(context);

    let reopened = Runtime::builder()
        .start(
            &beak,
            Options {
                directory: Some(root.join("volume")),
                ..Options::default()
            },
        )
        .expect("reopened");
    let snapshot = reopened.snapshot().expect("a snapshot");
    for ((_, bytes, _), file) in originals.iter().zip(&kept) {
        assert_eq!(
            files::read(&snapshot, "t1", &file.reference).expect("read after reopening"),
            Some(Bytes::copy_from_slice(bytes))
        );
    }
    reopened.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_missing_hidden_nonregular_oversized_and_unstored_files_are_refused() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let beak = Beak::new().expect("Beak");
    let mut context = context(&beak, &root).await;
    assert_eq!(context.keep_file("").await.expect_err("empty path").0, "invalid file path");
    assert_eq!(context.keep_file("bad\0path").await.expect_err("NUL path").0, "invalid file path");
    assert!(
        context
            .keep_file("missing.txt")
            .await
            .expect_err("missing")
            .0
            .contains("nothing is there")
    );
    assert!(context.keep_file(".").await.expect_err("a directory").0.contains("it is a folder"));
    let secret = context.base.workspace.join("secret.txt");
    std::fs::write(&secret, "hidden").expect("written");
    context.host.hidden.push(secret);
    assert!(context.keep_file("secret.txt").await.is_err(), "the call's namespace hides it");
    #[cfg(unix)]
    {
        let link = context.base.workspace.join("link.txt");
        std::os::unix::fs::symlink(context.base.workspace.join("secret.txt"), link).expect("a link to the hidden file");
        assert!(context.keep_file("link.txt").await.is_err(), "a link cannot bypass the namespace");
    }
    let large = std::fs::File::create(context.base.workspace.join("large.bin")).expect("a file");
    large.set_len(files::BYTES_MAX as u64 + 1).expect("a sparse file beyond the limit");
    assert!(
        context
            .keep_file("large.bin")
            .await
            .expect_err("oversized")
            .0
            .contains("a file holds at most 33554432 bytes")
    );
    let snapshot = context.runtime.snapshot().expect("a snapshot");
    assert!(
        snapshot
            .stat(&crate::threads::thread::within("t1", "files"))
            .expect("read")
            .is_none()
    );
    drop(snapshot);
    std::fs::write(context.base.workspace.join("unkept.txt"), "not kept").expect("written");
    let mut changes = Changes::new();
    changes.write(
        crate::threads::thread::within("t1", "files"),
        Bytes::from_static(b"a file blocks blob storage"),
    );
    context.runtime.commit(changes).await.expect("a blocked storage directory");
    assert!(
        context.keep_file("unkept.txt").await.is_err(),
        "no success when the volume cannot store the file"
    );
    context.runtime.close().await;
}
