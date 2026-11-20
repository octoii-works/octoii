//! The browser as Octoii's extension, through a client's connection: a page of a thread's opened, its thread's pages told,
//! the page painted as a binary frame of JPEG, taken to an address and typed into by the person — where the machine
//! has a browser.

use octoii::Octoii;
use octoii::connection::{self, Access};
use octoii::protocol::{Command, Scope};
use octoii::testing::{self, Client};
use serde_json::json;
use siphon_browser::BrowserExtension;

const PAGE: &str = "data:text/html,<title>Hello</title><input id=box autofocus>";

#[tokio::test(flavor = "multi_thread")]
async fn a_page_is_opened_listed_painted_and_worked_by_the_person() {
    let found = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
    ]
    .iter()
    .any(|path| std::path::Path::new(path).exists());
    if !found {
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
    client.event(&pages, |event| event["pages"][0]["title"] == "Moved").await;
    // Its history goes back from where it went, and nowhere forward; and it is told loading no more once it loaded.
    client
        .event(&pages, |event| {
            let listed = &event["pages"][0];
            listed["back"] == true && listed["forward"] == false && listed["loading"] == false
        })
        .await;
    client.command("browser.back", json!({ "page": page })).await;
    client.event(&pages, |event| event["pages"][0]["title"] == "Hello").await;
    // Back where it began, its history goes forward again — and back still, to the blank page it was opened on.
    client
        .event(&pages, |event| {
            event["pages"][0]["back"] == true && event["pages"][0]["forward"] == true
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
