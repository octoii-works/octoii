//! The browser as Octoii's extension, through a client's connection: a page of a thread's opened, its thread's pages told,
//! the page painted as a binary frame of JPEG, taken to an address and typed into by the person — where the machine
//! has a browser.

use octoii::Octoii;
use octoii::connection::{self, Access};
use octoii::protocol::{Command, Scope};
use octoii::testing::{self, Client};
use serde_json::json;
use siphon_browser::BrowserExtension;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const PAGE: &str = "data:text/html,<title>Hello</title><input id=box autofocus>";

fn chrome_available() -> bool {
    [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
    ]
    .iter()
    .any(|path| std::path::Path::new(path).exists())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_is_opened_listed_painted_and_worked_by_the_person() {
    if !chrome_available() {
        eprintln!("no browser on this machine");
        return;
    }
    // The profile within what the host keeps hidden, as a node keeps it in its Home.
    let folder = tempfile::tempdir().expect("the host's own folder");
    let kept = std::fs::canonicalize(folder.path()).expect("its real path");
    let machine = octoii::machine::Host {
        home: std::env::var_os("HOME").map(Into::into),
        hidden: vec![kept.clone()],
        ..octoii::machine::Host::default()
    };
    let extension = BrowserExtension {
        profile: Some(kept.join("browser/profile")),
        ..BrowserExtension::default()
    };
    let siphon = Octoii::builder(testing::setup(machine))
        .expect("built")
        .extension(siphon_base::BaseExtension::default())
        .extension(extension)
        .start()
        .expect("started");
    let mut client = Client::connect(&siphon, vec![Scope::ThreadsOperate, Scope::BrowserOperate], 64);
    client.message().await;
    let pages = client.subscribe(json!({ "type": "pages", "thread": "t1" })).await;
    client.event(&pages, |event| event["event"] == "pages").await;
    let page = client
        .command(
            "browser.open",
            json!({ "thread": "t1", "url": PAGE, "placement": "workspace", "view": { "width": 400, "height": 300 } }),
        )
        .await["result"]["page"]
        .as_str()
        .expect("a page")
        .to_owned();
    let (listed, _) = client.event(&pages, |event| event["pages"][0]["title"] == "Hello").await;
    assert_eq!(listed["pages"][0]["id"].as_str(), Some(page.as_str()));

    let painted = client
        .subscribe(json!({ "type": "page", "page": page, "presentation": "stream" }))
        .await;
    let (frame, jpeg) = client.event(&painted, |event| event["event"] == "frame").await;
    assert!(frame["width"].as_u64().is_some_and(|width| width > 0));
    assert_eq!(&jpeg[..3], [0xFF, 0xD8, 0xFF], "the frame's bytes are JPEG");

    room_belongs_to_controller(&siphon, &mut client, &page, &pages).await;
    fixed_viewport_survives_zoom_and_room_resize(&mut client, &page, &pages, &painted).await;

    assert_eq!(
        client.command("browser.text", json!({ "page": page, "text": "typed" })).await["result"],
        json!({})
    );
    assert_eq!(
        client
            .command(
                "browser.navigate",
                json!({ "page": page, "url": "data:text/html,<title>Moved</title>" }),
            )
            .await["result"],
        json!({})
    );
    // Its history goes back from where it went, and nowhere forward; and it is told loading no more once it loaded.
    client
        .event(&pages, |event| {
            let listed = &event["pages"][0];
            listed["title"] == "Moved" && listed["back"] == true && listed["forward"] == false && listed["load"]["state"] == "ready"
        })
        .await;
    assert_eq!(client.command("browser.back", json!({ "page": page })).await["result"], json!({}));
    // Back where it began, its history goes forward again — and back still, to the blank page it was opened on.
    client
        .event(&pages, |event| {
            let listed = &event["pages"][0];
            listed["title"] == "Hello" && listed["back"] == true && listed["forward"] == true
        })
        .await;
    client.command("browser.close", json!({ "page": page })).await;
    client.event(&pages, |event| event["pages"] == json!([])).await;
    siphon.close().await;
}

async fn room_belongs_to_controller(siphon: &Octoii, controller: &mut Client, page: &str, pages: &str) {
    let viewer = Access {
        session: "passive-viewer".into(),
        scopes: vec![Scope::BrowserOperate],
    };
    let resize = json!({ "page": page, "presentation": "stream", "view": { "width": 310, "height": 290 } });
    let measured = Command {
        method: "browser.resize".into(),
        params: resize.clone(),
    };
    assert_eq!(
        connection::call(siphon.clone(), viewer, measured)
            .await
            .expect("passive measurement"),
        json!({})
    );
    let listed = controller.subscribe(json!({ "type": "pages", "thread": "t1" })).await;
    let (view, _) = controller.event(&listed, |event| event["event"] == "pages").await;
    assert_eq!(view["pages"][0]["viewport_size"]["width"], 400);
    assert_eq!(controller.command("browser.resize", resize).await["result"], json!({}));
    controller
        .event(pages, |event| event["pages"][0]["viewport_size"]["width"] == 310)
        .await;
}

async fn fixed_viewport_survives_zoom_and_room_resize(client: &mut Client, page: &str, pages: &str, painted: &str) {
    let resized = client
        .command(
            "browser.resize",
            json!({
                "page": page, "presentation": "stream", "viewport": "fixed", "view": { "width": 800, "height": 600 }
            }),
        )
        .await;
    assert_eq!(resized["result"], json!({}));
    client.event(pages, |event| event["pages"][0]["viewport"] == "fixed").await;
    let zoomed = client
        .command("browser.zoom", json!({ "page": page, "presentation": "stream", "percent": 125 }))
        .await;
    assert_eq!(zoomed["result"], json!({}));
    let (listed, _) = client.event(pages, |event| event["pages"][0]["zoom"] == 125).await;
    assert_eq!(listed["pages"][0]["viewport_size"]["width"], 800);
    assert_eq!(listed["pages"][0]["view"]["width"], 640);
    assert_eq!(listed["pages"][0]["view"]["height"], 480);
    let (frame, _) = client.event(painted, |frame| frame["width"] == 640 && frame["height"] == 480).await;
    assert_eq!(frame["event"], "frame");
    let measured = client
        .command(
            "browser.resize",
            json!({
                "page": page, "presentation": "stream", "view": { "width": 550, "height": 400 }
            }),
        )
        .await;
    assert_eq!(measured["result"], json!({}));
    let reread = client.subscribe(json!({ "type": "pages", "thread": "t1" })).await;
    let (listed, _) = client.event(&reread, |event| event["event"] == "pages").await;
    assert_eq!(listed["pages"][0]["viewport_size"]["width"], 800);
    assert_eq!(listed["pages"][0]["view"]["width"], 640);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_page_can_be_retried_and_its_address_survives_reopening_the_volume() {
    if !chrome_available() {
        return;
    }
    let folder = tempfile::tempdir().expect("a folder");
    let running = || {
        let mut setup = testing::setup(octoii::machine::Host {
            home: std::env::var_os("HOME").map(Into::into),
            ..octoii::machine::Host::default()
        });
        setup.directory = Some(folder.path().join("volume"));
        Octoii::builder(setup)
            .expect("built")
            .extension(siphon_base::BaseExtension::default())
            .extension(BrowserExtension {
                profile: Some(folder.path().join("browser/profile")),
                ..BrowserExtension::default()
            })
            .start()
            .expect("started")
    };
    let siphon = running();
    let mut client = Client::connect(&siphon, vec![Scope::ThreadsOperate, Scope::BrowserOperate], 64);
    client.message().await;
    let key = siphon.volume().new_id().expect("a key");
    let created = client.command("threads.create", json!({ "key": key, "config": {} })).await;
    let thread = created["result"]["thread"].as_str().expect("a thread");
    let pages = client.subscribe(json!({ "type": "pages", "thread": thread })).await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a local site");
    let address = format!("http://{}/preview", listener.local_addr().expect("its address"));
    let serving = std::sync::Arc::new(AtomicBool::new(false));
    let work = serving.clone();
    let site = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).await;
            if work.load(Ordering::Relaxed) {
                let body = "<title>Recovered preview</title>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        }
    });

    let opened = client
        .command(
            "browser.open",
            json!({ "thread": thread, "url": address, "placement": "background", "view": { "width": 400, "height": 300 } }),
        )
        .await;
    let page = opened["result"]["page"].as_str().expect("a page");
    let (failed, _) = client.event(&pages, |event| event["pages"][0]["load"]["state"] == "failed").await;
    assert_eq!(failed["pages"][0]["url"], address);
    serving.store(true, Ordering::Relaxed);
    assert_eq!(client.command("browser.retry", json!({ "page": page })).await["result"], json!({}));
    client
        .event(&pages, |event| {
            let listed = &event["pages"][0];
            listed["id"] == page && listed["title"] == "Recovered preview" && listed["load"]["state"] == "ready"
        })
        .await;
    client.command("browser.close", json!({ "page": page })).await;
    client.event(&pages, |event| event["pages"] == json!([])).await;
    drop(client);
    siphon.close().await;

    let reopened = running();
    let mut client = Client::connect(&reopened, vec![Scope::BrowserOperate], 16);
    client.message().await;
    let reference = client.command("browser.reference", json!({ "thread": thread, "page": page })).await;
    assert_eq!(
        reference["result"]["reference"],
        json!({ "title": "Recovered preview", "url": address, "host": "headless" })
    );
    reopened.close().await;
    site.abort();
}
