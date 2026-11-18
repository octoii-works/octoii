//! Picking evidence through the person's protocol: no click sent to Chromium, finite main-document descriptions,
//! access and ownership checked before capture, and a screenshot kept after its page is closed.

use octoii::Octoii;
use octoii::protocol::Scope;
use octoii::testing::{self, Client};
use octoii::threads::files;
use serde_json::{Value, json};
use siphon_browser::BrowserExtension;
use siphon_browser::protocol::{ElementFeedback, PickElement, View};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;

struct Fixture {
    _folder: tempfile::TempDir,
    siphon: Octoii,
    client: Client,
    thread: String,
    page: String,
    url: String,
}

impl Fixture {
    async fn start(body: &str) -> Option<Self> {
        let found = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
        ]
        .iter()
        .any(|path| std::path::Path::new(path).exists());
        if !found {
            eprintln!("no browser on this machine");
            return None;
        }
        let folder = tempfile::tempdir().expect("the host's folder");
        let kept = folder.path().canonicalize().expect("its real path");
        let workspace = kept.join("workspace");
        std::fs::create_dir(&workspace).expect("a workspace");
        let machine = octoii::machine::Host {
            home: std::env::var_os("HOME").map(Into::into),
            hidden: vec![kept.clone()],
            ..octoii::machine::Host::default()
        };
        let siphon = Octoii::builder(testing::setup(machine))
            .expect("built")
            .extension(siphon_base::BaseExtension::default())
            .extension(BrowserExtension {
                profile: Some(kept.join("browser/profile")),
                ..BrowserExtension::default()
            })
            .start()
            .expect("started");
        let mut client = Client::connect(&siphon, vec![Scope::ThreadsOperate, Scope::BrowserOperate], 64);
        client.message().await;
        let id = client
            .call(
                "threads.create",
                json!({ "config": { "workspace": workspace }, "key": "pick-thread" }),
            )
            .await;
        let thread = client.answer(&id).await["result"]["thread"].as_str().expect("a thread").to_owned();
        let pages = client.subscribe(json!({ "type": "pages", "thread": thread })).await;
        client.event(&pages, |event| event["event"] == "pages").await;
        let url = serve(body).await;
        let id = client
            .call(
                "browser.open",
                json!({ "thread": thread, "url": url, "view": { "width": WIDTH, "height": HEIGHT, "scale": 1 } }),
            )
            .await;
        let page = client.answer(&id).await["result"]["page"].as_str().expect("a page").to_owned();
        client
            .event(&pages, |event| {
                event["pages"].as_array().is_some_and(|pages| {
                    pages
                        .iter()
                        .any(|listed| listed["id"] == page && listed["title"] == "Pick" && listed["loading"] == false)
                })
            })
            .await;
        Some(Self {
            _folder: folder,
            siphon,
            client,
            thread,
            page,
            url,
        })
    }

    fn request(&self, x: f64, y: f64) -> PickElement {
        PickElement {
            thread: self.thread.clone(),
            page: self.page.clone(),
            x,
            y,
            expected_url: self.url.clone(),
            view: View {
                width: WIDTH,
                height: HEIGHT,
                scale: Some(1.0),
            },
        }
    }

    async fn pick(&mut self, request: &PickElement) -> Value {
        let id = self.client.call("browser.pick", json!(request)).await;
        self.client.answer(&id).await
    }

    async fn close(self) {
        self.siphon.runtime.close().await;
    }
}

async fn serve(body: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("its address");
    let body = format!(
        "<!doctype html><title>Pick</title><style>body{{margin:0}}#target{{position:absolute;left:40px;top:40px;width:160px;height:60px}}</style>{body}"
    );
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let body = body.clone();
            tokio::spawn(async move {
                let mut asked = [0; 4096];
                let _ = stream.read(&mut asked).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{address}/")
}

fn feedback(answer: &Value) -> ElementFeedback {
    assert!(answer.get("error").is_none(), "{answer}");
    serde_json::from_value(answer["result"].clone()).expect("the typed evidence")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_dispatches_no_click_ignores_unrelated_dom_changes_and_keeps_its_screenshot_after_close() {
    let body = "<button id=target onclick=\"this.textContent='Clicks:'+(++window.clicks)\">Clicks:0</button>\
        <input id=password aria-label=Password type=password value=never-copy-this style='position:absolute;left:240px;top:40px;width:120px;height:60px'>\
        <p id=changing></p><script>window.clicks=0;setInterval(()=>{document.getElementById('changing').textContent=Date.now()},1)</script>";
    let Some(mut fixture) = Fixture::start(body).await else { return };
    let request = fixture.request(60.0, 60.0);
    let picked = feedback(&fixture.pick(&request).await);
    assert_eq!(picked.thread, fixture.thread);
    assert_eq!(picked.page, fixture.page);
    assert_eq!(picked.url, fixture.url);
    assert_ne!(picked.document, "");
    assert!(picked.captured_at > 0);
    assert_eq!((picked.viewport.width, picked.viewport.height), (WIDTH, HEIGHT));
    assert_eq!(picked.element.tag, "button");
    assert_eq!(picked.element.selector.as_deref(), Some("#target"));
    assert_eq!(picked.element.text.as_deref(), Some("Clicks:0"));
    let again = feedback(&fixture.pick(&request).await);
    assert_eq!(
        again.element.text.as_deref(),
        Some("Clicks:0"),
        "Chromium received no click from either pick"
    );
    let password_request = fixture.request(260.0, 60.0);
    let password = feedback(&fixture.pick(&password_request).await);
    assert_eq!(password.element.label.as_deref(), Some("Password"));
    assert!(password.element.text.is_none());
    assert!(!json!(password.element).to_string().contains("never-copy-this"));

    let id = fixture
        .client
        .call(
            "browser.resize",
            json!({ "page": fixture.page, "view": { "width": WIDTH, "height": HEIGHT, "scale": 2 } }),
        )
        .await;
    assert_eq!(fixture.client.answer(&id).await["result"], json!({}));
    let retina_request = PickElement {
        view: View {
            scale: Some(2.0),
            ..request.view
        },
        ..request.clone()
    };
    let retina = feedback(&fixture.pick(&retina_request).await);
    assert_eq!((retina.viewport.width, retina.viewport.height), (WIDTH, HEIGHT));
    assert!((retina.viewport.scale - 2.0).abs() < f64::EPSILON);
    assert!((retina.element.rect.x - 40.0).abs() < 0.5 && (retina.element.rect.width - 160.0).abs() < 0.5);

    let snapshot = fixture.siphon.runtime.snapshot().expect("a snapshot");
    let bytes = files::read(&snapshot, &fixture.thread, &picked.screenshot.reference)
        .expect("the saved screenshot")
        .expect("its bytes");
    assert_eq!(&bytes[..3], [0xFF, 0xD8, 0xFF]);
    let id = fixture.client.call("browser.close", json!({ "page": fixture.page })).await;
    assert_eq!(fixture.client.answer(&id).await["result"], json!({}));
    let after_close = fixture.pick(&request).await;
    assert_eq!(after_close["error"]["code"], "no_such_page");
    assert_eq!(
        files::read(
            &fixture.siphon.runtime.snapshot().expect("a snapshot"),
            &fixture.thread,
            &picked.screenshot.reference
        )
        .expect("the historical screenshot"),
        Some(bytes),
        "the image's content reference needs no live page"
    );
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn element_words_are_bounded_without_returning_the_whole_dom() {
    let label = "界".repeat(1024);
    let text = "x".repeat(2048);
    let body = format!("<button id=target aria-label='{label}'>{text}</button>");
    let Some(mut fixture) = Fixture::start(&body).await else { return };
    let request = fixture.request(60.0, 60.0);
    let picked = feedback(&fixture.pick(&request).await);
    assert_eq!(picked.element.label.as_deref().expect("its bounded label").chars().count(), 256);
    let returned_text = picked.element.text.as_deref().expect("its bounded text");
    assert!(!returned_text.is_empty() && returned_text.chars().count() <= 512);
    let element = json!(picked.element);
    assert!(element.get("outer_html").is_none() && element.get("attributes").is_none());
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn picking_requires_both_scopes_a_live_thread_and_the_pages_owning_thread() {
    let Some(mut fixture) = Fixture::start("<button id=target>Pick me</button>").await else {
        return;
    };
    let request = fixture.request(60.0, 60.0);
    for scopes in [vec![Scope::BrowserOperate], vec![Scope::ThreadsOperate]] {
        let mut restricted = Client::connect(&fixture.siphon, scopes, 8);
        restricted.message().await;
        let id = restricted.call("browser.pick", json!(&request)).await;
        assert_eq!(restricted.answer(&id).await["error"]["code"], "forbidden");
    }
    let id = fixture
        .client
        .call("threads.create", json!({ "config": {}, "key": "another-thread" }))
        .await;
    let other = fixture.client.answer(&id).await["result"]["thread"]
        .as_str()
        .expect("another thread")
        .to_owned();
    let wrong_thread = PickElement {
        thread: other,
        ..request.clone()
    };
    assert_eq!(fixture.pick(&wrong_thread).await["error"]["code"], "page_not_in_thread");
    let missing_thread = PickElement {
        thread: "missing".into(),
        ..request.clone()
    };
    assert_eq!(fixture.pick(&missing_thread).await["error"]["code"], "no_such_thread");
    let missing_page = PickElement {
        page: "missing".into(),
        ..request.clone()
    };
    assert_eq!(fixture.pick(&missing_page).await["error"]["code"], "no_such_page");
    for invalid in [
        PickElement {
            x: -1.0,
            ..request.clone()
        },
        PickElement {
            y: f64::from(HEIGHT),
            ..request.clone()
        },
        PickElement {
            view: View { width: 0, ..request.view },
            ..request.clone()
        },
        PickElement {
            view: View {
                scale: Some(0.0),
                ..request.view
            },
            ..request.clone()
        },
    ] {
        assert_eq!(fixture.pick(&invalid).await["error"]["code"], "bad_params");
    }
    let moved = PickElement {
        expected_url: format!("{}other", fixture.url),
        ..request.clone()
    };
    assert_eq!(fixture.pick(&moved).await["error"]["code"], "page_changed");
    let resized = PickElement {
        view: View {
            width: WIDTH + 1,
            ..request.view
        },
        ..request
    };
    assert_eq!(fixture.pick(&resized).await["error"]["code"], "page_changed");
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_url_changed_during_capture_is_rejected() {
    let body = "<button id=target>Pick me</button><script>const e=document.getElementById('target'),read=e.getBoundingClientRect.bind(e);e.getBoundingClientRect=()=>{const r=read();location.hash='changed';return r}</script>";
    let Some(mut fixture) = Fixture::start(body).await else { return };
    let request = fixture.request(60.0, 60.0);
    let answer = fixture.pick(&request).await;
    assert_eq!(answer["error"]["code"], "page_changed", "{answer}");
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_target_moved_during_capture_is_rejected() {
    let body = "<button id=target>Pick me</button><script>const e=document.getElementById('target'),read=e.getBoundingClientRect.bind(e);let first=true;e.getBoundingClientRect=()=>{const r=read();if(first){first=false;e.style.left='120px'}return r}</script>";
    let Some(mut fixture) = Fixture::start(body).await else { return };
    let request = fixture.request(60.0, 60.0);
    let answer = fixture.pick(&request).await;
    assert_eq!(answer["error"]["code"], "target_changed", "{answer}");
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_iframes_contents_are_explicitly_rejected() {
    let body = "<iframe style='position:absolute;left:40px;top:40px;width:160px;height:100px' srcdoc='<button style=\"width:150px;height:80px\">Inner</button>'></iframe>";
    let Some(mut fixture) = Fixture::start(body).await else { return };
    let request = fixture.request(60.0, 60.0);
    let answer = fixture.pick(&request).await;
    assert_eq!(answer["error"]["code"], "unsupported_frame", "{answer}");
    fixture.close().await;
}
