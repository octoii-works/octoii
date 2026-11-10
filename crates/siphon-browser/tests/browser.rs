//! The Agent working a real headless Chromium through `Browser::act`, where the machine has one — tabs, navigation,
//! reading, a person's input, dialogs, scripts in pages and frames, waits, the console, downloads and raw CDP — and the
//! person seeing its pages painted. Where the machine has no browser, each test says so and passes.

use std::time::Duration;

use beak::{Beak, Kernel, Options};
use serde_json::json;
use siphon_browser::{Action, Browser, BrowserEvent, BrowserHost, Outcome, Place, Refusal};
use siphon_machine::Host;

/// A form: a field, a button that greets what was typed, a choice, a button a box covers, one that raises a dialog,
/// and a link to a second page.
const FORM: &str = "<title>Form</title>\
<input aria-label='Name' id='name'>\
<button onclick=\"document.getElementById('out').textContent='Hello '+document.getElementById('name').value\">Greet</button>\
<p id='out'></p>\
<select aria-label='Colour'><option value='r'>Red</option><option value='g'>Green</option></select>\
<div style='position:relative'><button id='under'>Hidden</button>\
<div id='cover' style='position:absolute;inset:0;background:white'>Cover</div></div>\
<button onclick=\"alert('careful')\">Warn</button>\
<a href='/second'>Next</a>";

/// A page that logs, opens a page of its own, and offers a download.
const OPENER: &str = "<title>Opener</title>\
<script>console.log('ready', 42); console.error('trouble')</script>\
<button onclick=\"window.open('/second')\">Open</button>\
<a href='/file'>Notes</a>";

/// What the test's site answers `line`, the first line of a request, with: its content type and its body.
fn answer(line: &str, port: u16) -> (&'static str, String) {
    if line.starts_with("GET /second") {
        ("text/html", "<title>Second</title><h1>Second</h1>".to_owned())
    } else if line.starts_with("GET /file") {
        (
            "application/octet-stream\r\ncontent-disposition: attachment; filename=\"notes.txt\"",
            "the notes".to_owned(),
        )
    } else if line.starts_with("GET /opener") {
        ("text/html", OPENER.to_owned())
    } else if line.starts_with("GET /framing") {
        // localhost is another site than 127.0.0.1, so its frame runs in a process of its own.
        (
            "text/html",
            format!("<title>Framing</title><iframe src='http://localhost:{port}/inner'></iframe>"),
        )
    } else if line.starts_with("GET /inner") {
        ("text/html", "<title>Inner</title><p id='secret'>inside</p>".to_owned())
    } else {
        ("text/html", FORM.to_owned())
    }
}

/// Serves the test's site over HTTP on a port of the machine's: its address.
async fn serve() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().expect("its address").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            tokio::spawn(async move {
                let mut asked = vec![0; 4096];
                let read = stream.read(&mut asked).await.unwrap_or_default();
                let line = String::from_utf8_lossy(&asked[..read])
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                let (kind, body) = answer(&line, port);
                let answer = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(answer.as_bytes()).await;
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// A browser on a kernel of its own, its profile and downloads in folders of the test's; none where the machine has
/// no browser.
fn browser(beak: &Beak, profile: &std::path::Path, downloads: &std::path::Path) -> Option<Browser> {
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
    let kernel = Kernel::open(beak, &Options::default()).expect("a kernel").0;
    Some(Browser::start(BrowserHost {
        kernel,
        machine: Host {
            home: std::env::var_os("HOME").map(Into::into),
            ..Host::default()
        },
        program: None,
        profile: Some(profile.to_owned()),
        downloads: Some(downloads.canonicalize().expect("its real path")),
    }))
}

async fn act(browser: &Browser, action: serde_json::Value) -> Result<Outcome, Refusal> {
    let action: Action = serde_json::from_value(action).expect("an action");
    browser.act("t1", action).await
}

/// The ref of the line of `reading` that holds `role` and `name`.
fn ref_in(reading: &str, role: &str, name: &str) -> String {
    let line = reading
        .lines()
        .find(|line| line.contains(&format!("- {role} \"{name}\"")))
        .unwrap_or_else(|| panic!("no {role} {name} in:\n{reading}"));
    let at = line.find("ref=").expect("a ref") + 4;
    line[at..].split(']').next().expect("its end").to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_opens_reads_fills_and_submits_a_form_and_the_person_sees_it_painted() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let mut events = browser.events();
    let address = serve().await;
    let opened = act(&browser, json!({ "action": "open", "url": address })).await.expect("opened");
    let page = opened.page.expect("its page");
    assert_eq!(page.title, "Form");
    let read = act(&browser, json!({ "action": "read" })).await.expect("read").text;
    let field = ref_in(&read, "textbox", "Name");
    let typed = act(
        &browser,
        json!({ "action": "type", "target": { "ref": field }, "text": "Ada", "clear": true }),
    )
    .await
    .expect("typed");
    assert_eq!(typed.text, "the page changed");
    let clicked = act(&browser, json!({ "action": "click", "target": { "selector": "button" } }))
        .await
        .expect("clicked");
    assert_eq!(clicked.text, "the page changed");
    let greeted = act(
        &browser,
        json!({ "action": "evaluate", "expression": "document.getElementById('out').textContent" }),
    )
    .await
    .expect("ran");
    assert_eq!(greeted.value, Some(json!("Hello Ada")));
    let again = act(&browser, json!({ "action": "read" })).await.expect("read").text;
    assert!(again.contains("\"Hello Ada\""), "{again}");
    let colour = ref_in(&read, "combobox", "Colour");
    let chose = act(
        &browser,
        json!({ "action": "select", "target": { "ref": colour }, "values": ["Green"] }),
    )
    .await
    .expect("chose");
    assert!(chose.text.starts_with("chose [\"Green\"]"), "{}", chose.text);
    let looked = act(&browser, json!({ "action": "look" })).await.expect("looked");
    assert!(looked.image.expect("a picture").starts_with(b"\xff\xd8"), "a JPEG");
    let info = act(&browser, json!({ "action": "info" })).await.expect("measured");
    assert!(info.text.contains("its view 1280×800"), "{}", info.text);
    let waiting = async {
        loop {
            if let BrowserEvent::Painted { page: painted, .. } = events.recv().await.expect("an event")
                && painted == page.id
            {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("painted for the person");
    assert!(browser.frame(&page.id).await.expect("a frame").jpeg.starts_with(b"\xff\xd8"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_covered_element_is_refused_a_dialog_is_answered_and_the_page_goes_back_through_its_history() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    act(&browser, json!({ "action": "open", "url": address })).await.expect("opened");
    let read = act(&browser, json!({ "action": "read" })).await.expect("read").text;
    let covered = act(
        &browser,
        json!({ "action": "click", "target": { "ref": ref_in(&read, "button", "Hidden") } }),
    )
    .await;
    let words = covered.expect_err("covered").0;
    assert!(
        words.contains("covered by div#cover") && words.contains("nothing was done"),
        "{words}"
    );
    let warned = act(
        &browser,
        json!({ "action": "click", "target": { "ref": ref_in(&read, "button", "Warn") } }),
    )
    .await
    .expect("clicked");
    assert!(
        warned.text.contains("a alert dialog holds the page: \"careful\""),
        "{}",
        warned.text
    );
    let held = act(
        &browser,
        json!({ "action": "cdp", "method": "Runtime.evaluate", "params": { "expression": "1" } }),
    )
    .await;
    assert!(held.expect_err("held").0.contains("dialog"));
    act(&browser, json!({ "action": "dialog", "accept": true }))
        .await
        .expect("answered");
    let went = act(
        &browser,
        json!({ "action": "click", "target": { "ref": ref_in(&read, "link", "Next") } }),
    )
    .await
    .expect("clicked");
    assert_eq!(went.text, format!("navigated to {address}/second"));
    let back = act(&browser, json!({ "action": "back" })).await.expect("went back");
    assert_eq!(back.page.expect("its page").title, "Form");
    // The page began blank, as it was set up before it loaded anything; before that, no entry.
    assert!(
        act(&browser, json!({ "action": "back" }))
            .await
            .expect("went back")
            .text
            .ends_with("about:blank")
    );
    assert!(act(&browser, json!({ "action": "back" })).await.is_err(), "no entry so far back");
    let missing = act(&browser, json!({ "action": "click", "target": { "selector": "#nowhere" } })).await;
    let words = missing.expect_err("missing").0;
    assert!(words.contains("no element matches"), "{words}");
    let thrown = act(
        &browser,
        json!({ "action": "evaluate", "expression": "(() => { throw new Error('nope') })()" }),
    )
    .await;
    assert!(thrown.expect_err("thrown").0.contains("nope"));
}

#[tokio::test(flavor = "multi_thread")]
async fn tabs_are_the_thread_s_its_console_and_downloads_are_told_and_cdp_reaches_the_page() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    let first = act(&browser, json!({ "action": "open", "url": format!("{address}/opener") }))
        .await
        .expect("opened");
    let opener = first.page.expect("a page");
    let console = act(&browser, json!({ "action": "console" })).await.expect("read").text;
    assert_eq!(console, "#1 log: ready 42\n#2 error: trouble\n");
    act(&browser, json!({ "action": "click", "target": { "selector": "button" } }))
        .await
        .expect("clicked");
    let waiting = async {
        while browser.pages("t1").await.len() < 2 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("the page it opened is the thread's");
    let tabs = act(&browser, json!({ "action": "tabs" })).await.expect("listed").text;
    assert!(tabs.contains(&format!("{} (current)", opener.id)), "{tabs}");
    act(&browser, json!({ "action": "click", "target": { "selector": "a" } }))
        .await
        .expect("clicked");
    let waiting = async {
        loop {
            let told = act(&browser, json!({ "action": "downloads" })).await.expect("listed").text;
            if told.contains("completed") {
                return told;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let told = tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("downloaded in time");
    assert!(told.starts_with("\"notes.txt\" completed: 9 of 9 bytes"), "{told}");
    let params = json!({ "expression": "2 + 2", "returnByValue": true });
    let called = act(&browser, json!({ "action": "cdp", "method": "Runtime.evaluate", "params": params }))
        .await
        .expect("called");
    assert_eq!(called.value.expect("a value")["result"]["value"], 4);
    act(&browser, json!({ "action": "close" })).await.expect("closed");
    assert!(act(&browser, json!({ "action": "switch", "page": "nonesuch" })).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_script_runs_inside_a_frame_of_another_site_s_process() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    act(&browser, json!({ "action": "open", "url": format!("{address}/framing") }))
        .await
        .expect("opened");
    act(&browser, json!({ "action": "wait", "selector": "iframe" }))
        .await
        .expect("framed");
    let expression = "document.getElementById('secret').textContent";
    let inside = async {
        loop {
            match act(
                &browser,
                json!({ "action": "evaluate", "expression": expression, "frame": "localhost" }),
            )
            .await
            {
                Ok(outcome) => return outcome,
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    };
    let outcome = tokio::time::timeout(Duration::from_secs(20), inside)
        .await
        .expect("the frame reached");
    assert_eq!(outcome.value, Some(json!("inside")));
}

#[test]
fn an_action_is_json_its_schema_given() {
    let action: Action =
        serde_json::from_value(json!({ "action": "click", "target": { "x": 3, "y": 4 }, "clicks": 2 })).expect("an action");
    assert_eq!(
        action,
        Action::Click {
            target: Place::Point { x: 3.0, y: 4.0 },
            button: siphon_browser::MouseButton::Left,
            clicks: Some(2),
        }
    );
    let schema = schemars::schema_for!(Action).to_value();
    assert!(schema.to_string().contains("\"navigate\""), "{schema}");
}
