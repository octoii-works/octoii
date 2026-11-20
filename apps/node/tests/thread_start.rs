//! Multipart accepts a complete initial message; rejected bodies never create partial Threads.
use std::path::Path;

use axum::http::StatusCode;
use http_body_util::BodyExt as _;
use octoii::protocol::{Machine, Scope};
use octoii::testing::Client;
use octoii::threads::{Entry, History, Listed, StartState, StartWorkspace};
use octoii_ai::vocabulary::{FileRef, InputContent};
use octoii_node::{
    home::Home,
    http::{self, Node},
};
use serde_json::{Value, json};
use tower::ServiceExt as _;

struct TestNode {
    home: tempfile::TempDir,
    siphon: octoii::Octoii,
    address: String,
    router: axum::Router,
    writer: String,
    thread_only: String,
    server: tokio::task::JoinHandle<()>,
}

impl TestNode {
    async fn start() -> Self {
        let folder = tempfile::tempdir().unwrap();
        let home = Home::at(folder.path());
        home.prepare().unwrap();
        let mut siphon = octoii_node::serve::siphon(&home, octoii::providers::SignInSetup::default()).unwrap();
        let workspace = folder.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        siphon.machine.workspace = Some(workspace);
        let access = siphon.access();
        let writer = access
            .issue(None, vec![Scope::ThreadsOperate, Scope::ThreadsRead, Scope::GitOperate])
            .await
            .unwrap()
            .credential;
        let thread_only = access.issue(None, vec![Scope::ThreadsOperate]).await.unwrap().credential;
        let node = Node {
            siphon: siphon.clone(),
            access,
            machine: Machine::of("test".into(), &siphon.machine),
            origins: vec!["http://localhost".into()],
            cookie: http::cookie_of(7600),
            home,
            app: None,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let router = http::router(node);
        let app = router.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            home: folder,
            siphon,
            address,
            router,
            writer,
            thread_only,
            server,
        }
    }

    async fn post(&self, token: &str, body: Vec<u8>) -> (StatusCode, String) {
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/threads")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "multipart/form-data; boundary=octoii-test")
            .body(axum::body::Body::from(body))
            .unwrap();
        let reply = self.router.clone().oneshot(request).await.unwrap();
        let status = reply.status();
        let bytes = reply.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn accepted(&self, request: &Value, files: &[(&str, &[u8])]) -> String {
        let (status, receipt) = self.post(&self.writer, multipart(request, files)).await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        let receipt: Value = serde_json::from_str(&receipt).unwrap();
        let thread = receipt["thread"].as_str().unwrap();
        assert_eq!(receipt, json!({ "thread": thread }));
        thread.to_owned()
    }

    async fn ready_checkout(&self, thread: &str) -> StartWorkspace {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let start = self.siphon.volume().thread_start(thread).unwrap().expect("a preparation");
                match start.state {
                    StartState::Ready { directory } => return directory,
                    StartState::Failed { reason } | StartState::Interrupted { reason } => panic!("{reason}"),
                    _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("the node prepares the checkout without a connected client")
    }

    async fn close(self) {
        self.server.abort();
        let _ = self.server.await;
        self.siphon.close().await;
    }
}

fn manifest(key: &str, workspace: &Value) -> Value {
    json!({"key":key, "config":{"model":"test/no-model"}, "workspace":workspace,
        "parts":[{"type":"content","content":{"type":"input_text","text":"before"}},
            {"type":"attachment","index":0,"media_type":"text/plain","filename":"note.txt"},
            {"type":"content","content":{"type":"input_text","text":"after"}}]})
}

fn isolated_manifest(source: &Path, key: &str) -> Value {
    manifest(key, &json!({ "kind": "isolated", "path": source }))
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(directory)
        .args([
            "-c",
            "user.name=Octoii Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .output()
        .expect("ran git");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "--template="]);
    git(
        dir.path(),
        &["commit", "--allow-empty", "--no-verify", "--quiet", "-m", "Initial commit"],
    );
    dir
}

#[cfg(unix)]
fn reject_checkout(repo: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    let hooks = repo.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    git(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
}

fn multipart(manifest: &Value, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut body = format!("--octoii-test\r\nContent-Disposition: form-data; name=\"manifest\"\r\n\r\n{manifest}\r\n").into_bytes();
    for (name, bytes) in files {
        body.extend_from_slice(format!("--octoii-test\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--octoii-test--\r\n");
    body
}

#[tokio::test(flavor = "multi_thread")]
async fn a_worktree_is_prepared_in_the_nodes_temporary_directory_without_a_client() {
    let node = TestNode::start().await;
    let repo = repository();
    let head = git(repo.path(), &["rev-parse", "HEAD"]);
    let request = isolated_manifest(repo.path(), "automatic-preparation");
    let (thread, replay) = tokio::join!(
        node.accepted(&request, &[("file-0", b"attached")]),
        node.accepted(&request, &[("file-0", b"attached")]),
    );
    assert_eq!(thread, replay);
    let directory = node.ready_checkout(&thread).await;
    let expected = Home::at(node.home.path()).worktrees().unwrap().join(&thread);
    assert_eq!(
        std::path::Path::new(&directory.workspace).canonicalize().unwrap(),
        expected.canonicalize().unwrap()
    );
    assert_eq!(git(Path::new(&directory.workspace), &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(Path::new(&directory.workspace), &["branch", "--show-current"]),
        format!("octoii/{thread}\n")
    );
    let listing = node.siphon.threads.listed(&thread).unwrap();
    assert_eq!(listing.config.workspace.as_deref(), Some(directory.workspace.as_str()));
    let reference = format!("sha256:{}", octoii::tools::sha256_hex(b"attached"));
    let bytes = node.siphon.volume().read_file(&thread, &reference).unwrap().unwrap();
    assert_eq!(bytes.as_ref(), b"attached");

    git(
        repo.path(),
        &["commit", "--allow-empty", "--no-verify", "--quiet", "-m", "Advance HEAD"],
    );
    let mut pinned = isolated_manifest(repo.path(), "second-preparation");
    pinned["workspace"]["base"] = json!(head.trim());
    let second = node.accepted(&pinned, &[("file-0", b"attached")]).await;
    let second_directory = node.ready_checkout(&second).await;
    assert_ne!(second_directory.workspace, directory.workspace);
    assert_eq!(git(Path::new(&second_directory.workspace), &["rev-parse", "HEAD"]), head);
    let git_dir = repo.path().join(".git");
    std::fs::rename(&git_dir, repo.path().join("moved-git")).unwrap();
    assert_eq!(node.accepted(&request, &[("file-0", b"attached")]).await, thread);
    std::fs::rename(repo.path().join("moved-git"), &git_dir).unwrap();
    assert_eq!(node.siphon.threads.list().unwrap().len(), 2);
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_temporary_checkout_leaves_its_thread_and_files_readable() {
    let node = TestNode::start().await;
    let repo = repository();
    let request = isolated_manifest(repo.path(), "before-cleanup");
    let thread = node.accepted(&request, &[("file-0", b"saved attachment")]).await;
    let directory = node.ready_checkout(&thread).await;
    let root = Home::at(node.home.path()).worktrees().unwrap();
    assert!(
        Path::new(&directory.workspace)
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    let volume = node.siphon.volume();
    let first_input = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let history = History::open(&volume, &thread).unwrap().unwrap();
            if let Some(input) = history
                .after(None, 32)
                .unwrap()
                .into_iter()
                .find(|entry| matches!(entry, Entry::Input { .. }))
            {
                break input;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the first input is logged");
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(node.accepted(&request, &[("file-0", b"saved attachment")]).await, thread);
    assert!(!root.exists(), "a receipt replay does not open the worktree root");

    let mut client = Client::connect(&node.siphon, vec![Scope::ThreadsRead], 16);
    client.message().await;
    let listed = client.command("threads.list", json!({})).await;
    let listed: Vec<Listed> = serde_json::from_value(listed["result"]["threads"].clone()).unwrap();
    let unavailable = listed
        .iter()
        .find(|entry| entry.id == thread)
        .expect("the same Thread remains visible");
    assert_eq!(
        unavailable.status.start.as_ref().unwrap().state,
        StartState::Unavailable { directory }
    );
    let reference = format!("sha256:{}", octoii::tools::sha256_hex(b"saved attachment"));
    assert_eq!(
        volume.read_file(&thread, &reference).unwrap().unwrap().as_ref(),
        b"saved attachment"
    );
    assert!(
        History::open(&volume, &thread)
            .unwrap()
            .unwrap()
            .after(None, 32)
            .unwrap()
            .contains(&first_input)
    );

    let next = isolated_manifest(repo.path(), "after-cleanup");
    let second = node.accepted(&next, &[("file-0", b"new attachment")]).await;
    assert_ne!(second, thread);
    assert!(Path::new(&node.ready_checkout(&second).await.workspace).is_dir());
    drop(client);
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_submission_accepts_an_intent_and_rejects_caller_allocated_paths() {
    let node = TestNode::start().await;
    let repo = repository();
    let mut request = isolated_manifest(repo.path(), "client-allocated");
    request["workspace"]["destination"] = json!(node.home.path().join("not-managed"));
    assert_eq!(
        node.post(&node.writer, multipart(&request, &[("file-0", b"x")])).await.0,
        StatusCode::BAD_REQUEST,
    );
    let mut request = isolated_manifest(repo.path(), "moving-base");
    request["workspace"]["base"] = json!("HEAD");
    assert_eq!(
        node.post(&node.writer, multipart(&request, &[("file-0", b"x")])).await.0,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    assert_eq!(node.siphon.threads.list().unwrap(), []);
    node.close().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn multipart_keeps_order_files_and_idempotency_without_starting_the_agent() {
    let node = TestNode::start().await;
    let repo = repository();
    reject_checkout(repo.path());
    let request = isolated_manifest(repo.path(), "accepted");
    let (status, body) = node.post(&node.writer, multipart(&request, &[("file-0", b"attached")])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let accepted: Value = serde_json::from_str(&body).unwrap();
    let thread = accepted["thread"].as_str().unwrap();
    assert_eq!(accepted, json!({"thread":thread}));
    let listed = node.siphon.threads.listed(thread).unwrap();
    assert!(listed.status.start.as_ref().is_some_and(octoii::threads::Start::blocks_execution));
    let content = serde_json::to_value(&listed.status.inbox).unwrap();
    assert_eq!(content[0][1]["content"][0]["text"], "before");
    assert_eq!(content[0][1]["content"][2]["text"], "after");
    let reference = content[0][1]["content"][1]["file"]["ref"]
        .as_str()
        .unwrap()
        .strip_prefix("sha256:")
        .unwrap();
    let file = reqwest::Client::new()
        .get(format!("{}/threads/{thread}/files/{reference}", node.address))
        .bearer_auth(&node.writer)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(file, "attached");
    let (status, replay) = node.post(&node.writer, multipart(&request, &[("file-0", b"attached")])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&replay).unwrap(), accepted);
    assert_eq!(
        node.post(&node.writer, multipart(&request, &[("file-0", b"changed")])).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(node.siphon.threads.list().unwrap().len(), 1);
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_attachments_and_failed_images_never_leave_a_thread() {
    let node = TestNode::start().await;
    let request = isolated_manifest(&node.home.path().join("missing"), "bad");
    for files in [
        vec![],
        vec![("file-0", b"x".as_slice()), ("file-0", b"x".as_slice())],
        vec![("file-1", b"x".as_slice())],
    ] {
        assert_eq!(
            node.post(&node.writer, multipart(&request, &files)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut repeated = request.clone();
    repeated["parts"].as_array_mut().unwrap().push(request["parts"][1].clone());
    assert_eq!(
        node.post(&node.writer, multipart(&repeated, &[("file-0", b"x")])).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut invalid_image = manifest("bad-image", &json!({"kind":"current"}));
    invalid_image["parts"][1]["media_type"] = json!("image/png");
    assert_eq!(
        node.post(&node.writer, multipart(&invalid_image, &[("file-0", b"invalid")]))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(node.siphon.threads.list().unwrap().len(), 0);
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn permissions_origin_and_size_limits_precede_acceptance() {
    let node = TestNode::start().await;
    assert_eq!(
        node.post(
            &node.thread_only,
            multipart(&isolated_manifest(&node.home.path().join("missing"), "forbidden"), &[])
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let git_only = node.siphon.access().issue(None, vec![Scope::GitOperate]).await.unwrap().credential;
    assert_eq!(node.post(&git_only, b"not multipart".to_vec()).await.0, StatusCode::FORBIDDEN);
    assert_eq!(node.post("unknown", b"not multipart".to_vec()).await.0, StatusCode::UNAUTHORIZED);
    let foreign = reqwest::Client::new()
        .post(format!("{}/threads", node.address))
        .bearer_auth(&node.writer)
        .header("origin", "http://unrelated.example")
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    let mut large = isolated_manifest(&node.home.path().join("missing"), "large");
    large["parts"][0]["content"]["text"] = json!("x".repeat((1 << 20) + 1));
    assert_eq!(
        node.post(&node.writer, multipart(&large, &[])).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let oversized = vec![0; (32 << 20) + 1];
    assert_eq!(
        node.post(
            &node.writer,
            multipart(
                &isolated_manifest(&node.home.path().join("missing"), "large-file"),
                &[("file-0", &oversized)]
            )
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut total = isolated_manifest(&node.home.path().join("missing"), "total-limit");
    total["parts"] = json!(
        (0..3)
            .map(|index| json!({"type":"attachment", "index":index, "media_type":"text/plain", "filename":"part.txt"}))
            .collect::<Vec<_>>()
    );
    let chunk = vec![0; 24 << 20];
    assert_eq!(
        node.post(
            &node.writer,
            multipart(&total, &[("file-0", &chunk), ("file-1", &chunk), ("file-2", &chunk)])
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut client = octoii::testing::Client::connect(&node.siphon, vec![Scope::ThreadsOperate], 16);
    client.message().await;
    assert!(
        client
            .command("settings.set", json!({"environment":{"disabled_extensions":["git"]}}))
            .await["error"]
            .is_null()
    );
    assert_eq!(
        node.post(
            &node.writer,
            multipart(
                &isolated_manifest(&node.home.path().join("missing"), "disabled"),
                &[("file-0", b"x")]
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(node.siphon.threads.list().unwrap().len(), 0);

    let current = manifest("without-git", &json!({"kind":"current"}));
    let (status, body) = node.post(&node.thread_only, multipart(&current, &[("file-0", b"x")])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let accepted: Value = serde_json::from_str(&body).unwrap();
    let listed = node.siphon.threads.listed(accepted["thread"].as_str().unwrap()).unwrap();
    assert_eq!(listed.config.workspace.as_deref(), node.siphon.machine.workspace_of(None).to_str());
    assert!(listed.project.is_none());
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_current_workspace_accepts_files_and_claims_the_key_for_each_session() {
    let node = TestNode::start().await;
    let workspace = node.siphon.machine.workspace_of(None);
    let repository = workspace.join("repo");
    let folder = repository.join("subdirectory");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::create_dir(repository.join(".git")).unwrap();
    let mut request = manifest("current-start", &json!({"kind":"current"}));
    request["config"]["workspace"] = json!("repo/./subdirectory");
    let body = multipart(&request, &[("file-0", b"attached")]);
    let (status, receipt) = node.post(&node.thread_only, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let accepted: Value = serde_json::from_str(&receipt).unwrap();
    let thread = accepted["thread"].as_str().unwrap();
    assert_eq!(accepted, json!({"thread":thread}));
    let listed = node.siphon.threads.listed(thread).unwrap();
    assert_eq!(listed.config.workspace.as_deref(), folder.to_str());
    assert_eq!(listed.project.as_deref(), repository.to_str());
    assert!(listed.status.start.is_none());
    let reference = format!("sha256:{}", octoii::tools::sha256_hex(b"attached"));
    let bytes = node.siphon.volume().read_file(thread, &reference).unwrap().unwrap();
    assert_eq!(bytes.as_ref(), b"attached");

    assert_eq!(node.post(&node.thread_only, body.clone()).await, (StatusCode::OK, receipt));
    assert_eq!(
        node.post(&node.thread_only, multipart(&request, &[("file-0", b"different")]))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, other) = node.post(&node.writer, body).await;
    assert_eq!(status, StatusCode::OK, "{other}");
    assert_ne!(serde_json::from_str::<Value>(&other).unwrap()["thread"], thread);
    assert_eq!(node.siphon.threads.list().unwrap().len(), 2);
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_keeps_the_original_workspace_binding_when_git_membership_changes() {
    let node = TestNode::start().await;
    let repository = node.siphon.machine.workspace_of(None).join("repo");
    let folder = repository.join("subdirectory");
    std::fs::create_dir_all(&folder).unwrap();
    let mut request = manifest("same-original-request", &json!({"kind":"current"}));
    request["config"]["workspace"] = json!("repo/subdirectory");
    let body = multipart(&request, &[("file-0", b"attached")]);
    let http = reqwest::Client::new();
    let post = async || {
        let response = http
            .post(format!("{}/threads", node.address))
            .bearer_auth(&node.thread_only)
            .header("content-type", "multipart/form-data; boundary=octoii-test")
            .body(body.clone())
            .send()
            .await
            .unwrap();
        (response.status(), response.text().await.unwrap())
    };
    let (status, receipt) = post().await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let accepted: Value = serde_json::from_str(&receipt).unwrap();
    let thread = accepted["thread"].as_str().unwrap();
    let original = node.siphon.threads.listed(thread).unwrap();
    assert_eq!(original.config.workspace.as_deref(), folder.to_str());
    assert_eq!(original.project.as_deref(), folder.to_str());

    let git = std::process::Command::new("git")
        .args(["init", "--quiet", "--template="])
        .current_dir(&repository)
        .output()
        .unwrap();
    assert!(git.status.success(), "{}", String::from_utf8_lossy(&git.stderr));
    assert_eq!(octoii::tools::Place::of(&folder).project, repository);

    assert_eq!(post().await, (StatusCode::OK, receipt));
    let listed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let listed = node.siphon.threads.listed(thread).unwrap();
            if listed.status.inbox.is_empty() {
                break listed;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the initial input was logged");
    assert_eq!(listed.config.workspace, original.config.workspace);
    assert_eq!(listed.project, original.project);
    assert_eq!(node.siphon.threads.list().unwrap().len(), 1);

    let volume = node.siphon.volume();
    let history = History::open(&volume, thread).unwrap().unwrap();
    let inputs: Vec<_> = history
        .after(None, 32)
        .unwrap()
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Input { content, .. } => Some(content),
            _ => None,
        })
        .collect();
    let reference = format!("sha256:{}", octoii::tools::sha256_hex(b"attached"));
    let bytes = volume.read_file(thread, &reference).unwrap().unwrap();
    assert_eq!(bytes.as_ref(), b"attached");
    assert_eq!(
        inputs,
        [vec![
            InputContent::text("before"),
            InputContent::InputFile {
                file: FileRef {
                    reference,
                    media_type: "text/plain".into(),
                    filename: Some("note.txt".into()),
                },
            },
            InputContent::text("after"),
        ]]
    );
    node.close().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn image_only_submission_keeps_the_fitted_reference_in_its_initial_input() {
    let node = TestNode::start().await;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(2048, 2048, image::Rgb([20, 40, 80]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let repo = repository();
    reject_checkout(repo.path());
    let mut request = isolated_manifest(repo.path(), "image-only");
    request["parts"] = json!([{"type":"attachment", "index":0, "media_type":"image/png", "filename":"image.png"}]);
    let (status, body) = node.post(&node.writer, multipart(&request, &[("file-0", &png)])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let accepted: Value = serde_json::from_str(&body).unwrap();
    let thread = accepted["thread"].as_str().unwrap();
    let listed = node.siphon.threads.listed(thread).unwrap();
    let inbox = serde_json::to_value(listed.status.inbox).unwrap();
    let content = &inbox[0][1]["content"];
    assert_eq!(content.as_array().unwrap().len(), 1);
    assert_eq!(content[0]["type"], "input_image");
    let reference = content[0]["file"]["ref"].as_str().unwrap().strip_prefix("sha256:").unwrap();
    let response = reqwest::Client::new()
        .get(format!("{}/threads/{thread}/files/{reference}", node.address))
        .bearer_auth(&node.writer)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let image = image::load_from_memory(&response.bytes().await.unwrap()).unwrap();
    assert!(image.width() < 2048 && image.height() < 2048);
    node.close().await;
}
