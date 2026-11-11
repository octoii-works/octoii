//! The browser as Siphon's plugin, through a client's connection: a page of a thread's opened, its thread's pages told,
//! the page painted as a binary frame of JPEG, taken to an address and typed into by the person — where the machine
//! has a browser.

use beak::Beak;
use serde_json::json;
use siphon::Siphon;
use siphon::testing::{self, Client};
use siphon_browser::BrowserPlugin;
use siphon_protocol::Scope;

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
    let beak = Beak::new().expect("Beak");
    let profile = tempfile::tempdir().expect("a profile");
    let machine = siphon_machine::Host {
        home: std::env::var_os("HOME").map(Into::into),
        ..siphon_machine::Host::default()
    };
    let plugin = BrowserPlugin {
        profile: Some(profile.path().to_owned()),
        ..BrowserPlugin::default()
    };
    let siphon = Siphon::builder(&beak, testing::setup(machine))
        .expect("built")
        .plugin(plugin)
        .start()
        .expect("started");
    let mut client = Client::connect(&siphon, vec![Scope::ThreadsOperate, Scope::BrowserOperate], 64);
    client.message().await;
    let pages = client.subscribe(json!({ "type": "pages", "thread": "t1" })).await;
    client.event(&pages, |event| event["event"] == "pages").await;
    let id = client
        .call(
            "browser.open",
            json!({ "thread": "t1", "url": PAGE, "view": { "width": 400, "height": 300 } }),
        )
        .await;
    let page = client.answer(&id).await["result"]["page"].as_str().expect("a page").to_owned();
    let (listed, _) = client.event(&pages, |event| event["pages"][0]["title"] == "Hello").await;
    assert_eq!(listed["pages"][0]["id"].as_str(), Some(page.as_str()));

    let painted = client.subscribe(json!({ "type": "page", "page": page })).await;
    let (frame, jpeg) = client.event(&painted, |event| event["event"] == "frame").await;
    assert!(frame["width"].as_u64().is_some_and(|width| width > 0));
    assert_eq!(&jpeg[..3], [0xFF, 0xD8, 0xFF], "the frame's bytes are JPEG");

    let id = client.call("browser.text", json!({ "page": page, "text": "typed" })).await;
    assert_eq!(client.answer(&id).await["result"], json!({}));
    let id = client
        .call(
            "browser.navigate",
            json!({ "page": page, "url": "data:text/html,<title>Moved</title>" }),
        )
        .await;
    assert_eq!(client.answer(&id).await["result"], json!({}));
    client.event(&pages, |event| event["pages"][0]["title"] == "Moved").await;
    // Its history goes back from where it went, and nowhere forward; and it is told loading no more once it loaded.
    client
        .event(&pages, |event| {
            let listed = &event["pages"][0];
            listed["back"] == true && listed["forward"] == false && listed["loading"] == false
        })
        .await;
    let id = client.call("browser.back", json!({ "page": page })).await;
    client.answer(&id).await;
    client.event(&pages, |event| event["pages"][0]["title"] == "Hello").await;
    // Back where it began, its history goes forward again — and back still, to the blank page it was opened on.
    client
        .event(&pages, |event| {
            event["pages"][0]["back"] == true && event["pages"][0]["forward"] == true
        })
        .await;
    let id = client.call("browser.close", json!({ "page": page })).await;
    client.answer(&id).await;
    client.event(&pages, |event| event["pages"] == json!([])).await;
    siphon.runtime.close().await;
}
