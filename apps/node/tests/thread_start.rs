//! Multipart accepts a complete initial message; rejected bodies never create partial Threads.
use axum::http::StatusCode;
use http_body_util::BodyExt as _;
use octoii::{
    access::Access,
    home::Home,
    http::{self, Node},
};
use serde_json::{Value, json};
use siphon::protocol::{Machine, Scope};
use tower::ServiceExt as _;

struct TestNode {
    _home: tempfile::TempDir,
    siphon: siphon::Siphon,
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
        let siphon = octoii::serve::siphon(&home, siphon::providers::SignInSetup::default()).unwrap();
        let access = Access::new(siphon.runtime.clone());
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
            _home: folder,
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
            .uri("/git/threads")
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
        self.siphon.runtime.close().await;
    }
}

fn manifest(key: &str) -> Value {
    json!({"key":key, "config":{}, "worktree":{"path":"/repo", "destination":"/repo-task", "branch":"task", "base":"0".repeat(40)},
        "parts":[{"type":"content","content":{"type":"input_text","text":"before"}},
            {"type":"attachment","index":0,"media_type":"text/plain","filename":"note.txt"},
            {"type":"content","content":{"type":"input_text","text":"after"}}]})
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
async fn multipart_keeps_order_files_and_idempotency_without_starting_the_agent() {
    let node = TestNode::start().await;
    let request = manifest("accepted");
    let (status, body) = node.post(&node.writer, multipart(&request, &[("file-0", b"attached")])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let accepted: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(accepted["start"]["state"]["state"], "pending");
    let thread = accepted["thread"].as_str().unwrap();
    let listed = node.siphon.threads.listed(thread).unwrap();
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
    let request = manifest("bad");
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
    let mut invalid_image = request;
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
        node.post(&node.thread_only, b"not multipart".to_vec()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(node.post("unknown", b"not multipart".to_vec()).await.0, StatusCode::UNAUTHORIZED);
    let foreign = reqwest::Client::new()
        .post(format!("{}/git/threads", node.address))
        .bearer_auth(&node.writer)
        .header("origin", "http://unrelated.example")
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    let mut large = manifest("large");
    large["parts"][0]["content"]["text"] = json!("x".repeat((1 << 20) + 1));
    assert_eq!(
        node.post(&node.writer, multipart(&large, &[])).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let oversized = vec![0; (32 << 20) + 1];
    assert_eq!(
        node.post(&node.writer, multipart(&manifest("large-file"), &[("file-0", &oversized)]))
            .await
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut total = manifest("total-limit");
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
    let mut client = siphon::testing::Client::connect(&node.siphon, vec![Scope::ThreadsOperate], 16);
    client.message().await;
    let id = client
        .call("settings.set", json!({"environment":{"disabled_extensions":["git"]}}))
        .await;
    assert!(client.answer(&id).await["error"].is_null());
    assert_eq!(
        node.post(&node.writer, multipart(&manifest("disabled"), &[("file-0", b"x")]))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(node.siphon.threads.list().unwrap().len(), 0);
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn image_only_submission_keeps_the_fitted_reference_in_its_initial_input() {
    let node = TestNode::start().await;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(2048, 2048, image::Rgb([20, 40, 80]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut request = manifest("image-only");
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
