//! Multipart accepts a complete initial message; rejected bodies never create partial Threads.
use axum::http::StatusCode;
use http_body_util::BodyExt as _;
use octoii::protocol::{Machine, Scope};
use octoii::testing::Client;
use octoii::threads::{Entry, History, StartState};
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

fn isolated_manifest(key: &str) -> Value {
    manifest(
        key,
        &json!({"kind":"isolated", "worktree":{"path":"/repo", "destination":"/repo-task", "branch":"task", "base":"0".repeat(40)}}),
    )
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
async fn worktree_plans_use_the_nodes_temporary_directory() {
    let node = TestNode::start().await;
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(args)
            .output()
            .expect("ran git");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    };
    git(&["init", "--quiet", "--template="]);
    git(&[
        "-c",
        "user.name=Octoii Test",
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "commit.gpgSign=false",
        "commit",
        "--allow-empty",
        "--no-verify",
        "--quiet",
        "-m",
        "Initial commit",
    ]);
    let head = git(&["rev-parse", "HEAD"]);
    let key = "12345678-1234-1234-1234-123456789abc";
    let mut client = Client::connect(&node.siphon, vec![Scope::FilesRead], 64);
    client.message().await;
    let id = client
        .call(
            "git.worktree-plan",
            json!({"path": repo.path().canonicalize().unwrap(), "key": key, "base": head.trim()}),
        )
        .await;
    let reply = client.answer(&id).await;
    let root = Home::at(node.home.path()).temporary().unwrap().join("worktrees");
    let expected = std::path::absolute(root.join(key)).unwrap();
    assert_eq!(reply["result"]["destination"], json!(expected), "{reply}");
    assert!(!root.exists(), "planning a task does not create a checkout");
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn multipart_keeps_order_files_and_idempotency_without_starting_the_agent() {
    let node = TestNode::start().await;
    let request = isolated_manifest("accepted");
    let (status, body) = node.post(&node.writer, multipart(&request, &[("file-0", b"attached")])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let accepted: Value = serde_json::from_str(&body).unwrap();
    let thread = accepted["thread"].as_str().unwrap();
    assert_eq!(accepted, json!({"thread":thread}));
    let listed = node.siphon.threads.listed(thread).unwrap();
    assert_eq!(listed.status.start.as_ref().map(|start| &start.state), Some(&StartState::Pending));
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
    let request = isolated_manifest("bad");
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
        node.post(&node.thread_only, multipart(&isolated_manifest("forbidden"), &[]))
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
    let mut large = isolated_manifest("large");
    large["parts"][0]["content"]["text"] = json!("x".repeat((1 << 20) + 1));
    assert_eq!(
        node.post(&node.writer, multipart(&large, &[])).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let oversized = vec![0; (32 << 20) + 1];
    assert_eq!(
        node.post(&node.writer, multipart(&isolated_manifest("large-file"), &[("file-0", &oversized)]))
            .await
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut total = isolated_manifest("total-limit");
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
    let id = client
        .call("settings.set", json!({"environment":{"disabled_extensions":["git"]}}))
        .await;
    assert!(client.answer(&id).await["error"].is_null());
    assert_eq!(
        node.post(&node.writer, multipart(&isolated_manifest("disabled"), &[("file-0", b"x")]))
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

#[tokio::test(flavor = "multi_thread")]
async fn image_only_submission_keeps_the_fitted_reference_in_its_initial_input() {
    let node = TestNode::start().await;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(2048, 2048, image::Rgb([20, 40, 80]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut request = isolated_manifest("image-only");
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
