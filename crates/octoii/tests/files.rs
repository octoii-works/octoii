//! A thread's files over the node's HTTP: an image sent kept as every model reads one and answered with its
//! reference; its bytes given back for good, and a picture of it fitted to a box; a file of no type, a session that
//! may only read, and another thread's file, each refused.

use std::io::Cursor;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper::Method;
use octoii::access::Access;
use octoii::home::Home;
use octoii::http::{self, Node};
use serde_json::Value;
use siphon_protocol::{Machine, Scope};
use siphon_threads::NewThread;

/// A PNG of `width` by `height`.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([200, 30, 30]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).expect("a PNG");
    bytes.into_inner()
}

async fn request(address: &str, method: Method, path: &str, headers: &[(&str, &str)], body: Vec<u8>) -> (u16, hyper::HeaderMap, Bytes) {
    let stream = tokio::net::TcpStream::connect(address).await.expect("connected");
    let (mut sender, connection) = hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
        .await
        .expect("HTTP");
    tokio::spawn(connection);
    let mut built = hyper::Request::builder().method(method).uri(path).header("host", address);
    for (name, value) in headers {
        built = built.header(*name, *value);
    }
    let response = sender
        .send_request(built.body(Full::new(Bytes::from(body))).expect("a request"))
        .await
        .expect("answered");
    let (parts, body) = response.into_parts();
    (parts.status.as_u16(), parts.headers, body.collect().await.expect("read").to_bytes())
}

/// The node's HTTP of `siphon` and `access`, served on a loopback port: its address.
async fn served(siphon: &siphon::Siphon, access: &Access) -> String {
    let node = Node {
        siphon: siphon.clone(),
        access: access.clone(),
        machine: Machine {
            name: "here".into(),
            workspace: siphon.machine.workspace_of(None).to_string_lossy().into_owned(),
        },
        origins: Vec::new(),
        cookie: http::cookie_of(7600),
        app: None,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bound");
    let address = listener.local_addr().expect("its address").to_string();
    tokio::spawn(async move { axum::serve(listener, http::router(node)).await });
    address
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_s_files_are_sent_kept_and_given_back_to_a_session_that_may() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("a Home");
    let siphon = octoii::serve::siphon(&home, siphon::providers::SignInSetup::default()).expect("Siphon");
    let access = Access::new(siphon.runtime.clone());
    let thread = siphon.threads.create(&NewThread::default(), "k1").await.expect("a thread");
    let other = siphon.threads.create(&NewThread::default(), "k2").await.expect("a thread");
    let address = served(&siphon, &access).await;
    let writer = format!(
        "Bearer {}",
        access
            .issue(None, vec![Scope::ThreadsRead, Scope::ThreadsOperate])
            .await
            .expect("a token")
            .credential
    );
    let reader = format!(
        "Bearer {}",
        access.issue(None, vec![Scope::ThreadsRead]).await.expect("a token").credential
    );

    let path = format!("/threads/{thread}/files?name=red.png");
    let headers = [("authorization", writer.as_str()), ("content-type", "image/png")];
    let (status, _, body) = request(&address, Method::POST, &path, &headers, png(3000, 1500)).await;
    assert_eq!(status, 200);
    let file: Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(
        (file["media_type"].as_str(), file["filename"].as_str()),
        (Some("image/png"), Some("red.png"))
    );
    let reference = file["ref"].as_str().expect("a reference").to_owned();
    let hash = reference.strip_prefix("sha256:").expect("a hash");

    let at = format!("/threads/{thread}/files/{hash}");
    let (status, headers, kept) = request(&address, Method::GET, &at, &[("authorization", reader.as_str())], Vec::new()).await;
    assert_eq!(status, 200);
    assert_eq!(headers["cache-control"], "private, max-age=31536000, immutable");
    let image = image::load_from_memory(&kept).expect("an image");
    assert_eq!(
        (image.width(), image.height()),
        (1516, 758),
        "kept as every model reads one: its long side and pixels within theirs"
    );
    let small = format!("{at}?width_max=200&height_max=200");
    let (_, _, picture) = request(&address, Method::GET, &small, &[("authorization", reader.as_str())], Vec::new()).await;
    let picture = image::load_from_memory(&picture).expect("an image");
    assert_eq!(
        (picture.width(), picture.height()),
        (200, 100),
        "a picture fitted to the box, its shape kept"
    );

    let untyped = request(&address, Method::POST, &path, &[("authorization", writer.as_str())], b"x".to_vec())
        .await
        .0;
    assert_eq!(untyped, 400, "a file says what it is");
    let reading = [("authorization", reader.as_str()), ("content-type", "text/plain")];
    assert_eq!(
        request(&address, Method::POST, &path, &reading, b"x".to_vec()).await.0,
        403,
        "a reader sends nothing"
    );
    let elsewhere = format!("/threads/{other}/files/{hash}");
    assert_eq!(
        request(&address, Method::GET, &elsewhere, &[("authorization", reader.as_str())], Vec::new())
            .await
            .0,
        404
    );
    assert_eq!(
        request(&address, Method::GET, &at, &[], Vec::new()).await.0,
        401,
        "no session, no file"
    );

    let on_machine = folder.path().canonicalize().expect("its real path").join("shot.png");
    std::fs::write(&on_machine, png(400, 200)).expect("written");
    let browser = format!(
        "Bearer {}",
        access.issue(None, vec![Scope::FilesRead]).await.expect("a token").credential
    );
    let asked = format!("/machine?path={}&width_max=100&height_max=100", on_machine.display());
    let (status, headers, bytes) = request(&address, Method::GET, &asked, &[("authorization", browser.as_str())], Vec::new()).await;
    assert_eq!((status, headers["content-type"].to_str().ok()), (200, Some("image/png")));
    let picture = image::load_from_memory(&bytes).expect("an image");
    assert_eq!(
        (picture.width(), picture.height()),
        (100, 50),
        "a file of the machine's, fitted to the box"
    );
    let refused = request(&address, Method::GET, &asked, &[("authorization", reader.as_str())], Vec::new())
        .await
        .0;
    assert_eq!(refused, 403, "a session that may not read files reads none of the machine's");
    siphon.runtime.close().await;
}
