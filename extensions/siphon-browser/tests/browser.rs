//! The Agent working a real headless Chromium through `Browser::act`, where the machine has one — tabs, navigation,
//! reading, a person's input, dialogs, scripts in pages and frames, waits, the console, downloads and raw CDP — and the
//! person seeing its pages painted. Where the machine has no browser, each test says so and passes.

use std::time::Duration;

use serde_json::json;
use siphon::machine::{Base, Host, Machine, Writes};
use siphon::testing::Beak;
use siphon_browser::{Action, Browser, BrowserHost, Change, Outcome, Place, Refusal, Size};

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

/// An expandable preview that reports its theme and preserves local interaction state.
const THEMED_PREVIEW: &str = r#"<!doctype html>
    <style>#authored { background: rgb(12, 34, 56); color: rgb(210, 220, 230); font-family: serif; }</style>
    <details open><summary onclick="clicks += 1">Toggle report</summary><div style="height:1400px"></div></details>
    <code>Code</code><span id="authored">Authored design</span>
    <script>
        let clicks = 0;
        const report = stage => {
            const root = getComputedStyle(document.documentElement);
            const body = getComputedStyle(document.body);
            const authored = getComputedStyle(document.getElementById('authored'));
            top.postMessage({ type: 'preview.state', stage, clicks,
                background: root.backgroundColor, foreground: body.color,
                font: body.fontFamily, mono: getComputedStyle(document.querySelector('code')).fontFamily,
                scheme: root.colorScheme, authoredBackground: authored.backgroundColor,
                authoredForeground: authored.color, authoredFont: authored.fontFamily }, '*');
        };
        addEventListener('message', event => {
            if (event.data?.type === 'preview.toggle') document.querySelector('summary').click();
            if (event.data?.type === 'preview.spoof') {
                parent.postMessage({ type: 'octoii.html.theme', theme: event.data.theme }, '*');
            }
            if (event.data?.stage) requestAnimationFrame(() => requestAnimationFrame(() => report(event.data.stage)));
        });
        addEventListener('DOMContentLoaded', () => report('initial'));
    </script>"#;

/// What the test's site answers `line`, the first line of a request, with: its content type and its body.
fn answer(line: &str, port: u16) -> (&'static str, String) {
    if line.starts_with("GET /remote.js") {
        ("text/javascript", "console.log('remote script loaded');".to_owned())
    } else if line.starts_with("GET /remote.css") {
        ("text/css", "body{--remote-style:loaded}".to_owned())
    } else if line.starts_with("GET /remote.svg") {
        ("image/svg+xml", "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10' fill='blue'/></svg>".to_owned())
    } else if line.starts_with("GET /remote-data") {
        ("text/plain\r\naccess-control-allow-origin: *", "remote data loaded".to_owned())
    } else if line.starts_with("GET /html-preview.html") {
        ("text/html", siphon_html::DOCUMENT.to_owned())
    } else if line.starts_with("GET /preview") {
        (
            "text/html",
            "<title>Embedded preview</title><iframe src='/html-preview.html' sandbox='allow-scripts'></iframe>".to_owned(),
        )
    } else if line.starts_with("GET /second") {
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

/// A browser running its commands on a kernel of its own, its profile and downloads in folders of the test's; none where the machine has
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
    let commands = siphon::testing::commands(beak);
    Some(Browser::start(BrowserHost {
        commands,
        machine: Host {
            home: std::env::var_os("HOME").map(Into::into),
            ..Host::default()
        },
        program: None,
        profile: Some(profile.to_owned()),
        downloads: Some(downloads.canonicalize().expect("its real path")),
    }))
}

fn machine(beak: &Beak, workspace: &std::path::Path) -> Machine {
    siphon::testing::commands(beak)
        .open(
            &Host::default(),
            Base {
                workspace: workspace.canonicalize().expect("its real path"),
                home: None,
            },
            &Writes::Within(Vec::new()),
        )
        .expect("the call's machine")
}

async fn act(browser: &Browser, machine: &Machine, action: serde_json::Value) -> Result<Outcome, Refusal> {
    let action: Action = serde_json::from_value(action).expect("an action");
    browser.act("t1", machine, action).await
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
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let mut events = browser.events();
    let address = serve().await;
    let opened = act(&browser, &machine, json!({ "action": "open", "url": address }))
        .await
        .expect("opened");
    let page = opened.page.expect("its page");
    assert_eq!(page.title, "Form");
    let observed = act(&browser, &machine, json!({ "action": "read" })).await.expect("read");
    let data = serde_json::to_value(&observed).expect("browser observation");
    let read = observed.text;
    let field = ref_in(&read, "textbox", "Name");
    assert!(data["observation"]["refs"].as_array().expect("refs").contains(&json!(field)));
    let typed = act(
        &browser,
        &machine,
        json!({ "action": "type", "target": { "ref": field }, "text": "Ada", "clear": true }),
    )
    .await
    .expect("typed");
    assert_eq!(typed.text, "the page changed");
    let clicked = act(&browser, &machine, json!({ "action": "click", "target": { "selector": "button" } }))
        .await
        .expect("clicked");
    assert_eq!(clicked.text, "the page changed");
    let greeted = act(
        &browser,
        &machine,
        json!({ "action": "evaluate", "expression": "document.getElementById('out').textContent" }),
    )
    .await
    .expect("ran");
    assert_eq!(greeted.value, Some(json!("Hello Ada")));
    let again = act(&browser, &machine, json!({ "action": "read" })).await.expect("read").text;
    assert!(again.contains("\"Hello Ada\""), "{again}");
    let colour = ref_in(&read, "combobox", "Colour");
    let chose = act(
        &browser,
        &machine,
        json!({ "action": "select", "target": { "ref": colour }, "values": ["Green"] }),
    )
    .await
    .expect("chose");
    assert!(chose.text.starts_with("chose [\"Green\"]"), "{}", chose.text);
    let looked = act(&browser, &machine, json!({ "action": "look" })).await.expect("looked");
    assert!(looked.image.expect("a picture").starts_with(b"\xff\xd8"), "a JPEG");
    let info = act(&browser, &machine, json!({ "action": "info" })).await.expect("measured");
    assert!(info.text.contains("its view 1280×800"), "{}", info.text);
    assert_eq!(serde_json::to_value(&info).expect("info")["observation"]["info"]["width"], 1280.0);
    let waiting = async {
        loop {
            if let Change::Painted { page: painted, .. } = events.recv().await.expect("an event")
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
async fn resizing_a_page_tells_its_thread_the_view_that_matches_the_new_frame() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let opened = act(&browser, &machine, json!({ "action": "open", "url": serve().await }))
        .await
        .expect("opened");
    let page = opened.page.expect("its page");
    let mut events = browser.events();
    for size in [
        Size {
            width: 913,
            height: 571,
            scale: 2.0,
        },
        Size {
            width: 640,
            height: 480,
            scale: 1.0,
        },
    ] {
        let before = browser.frame(&page.id).await.expect("a frame before resizing").number;
        browser.resize(&page.id, size).await.expect("resized");
        let synchronized = async {
            let mut listed = None;
            let mut painted = None;
            loop {
                match events.recv().await.expect("an event") {
                    Change::Pages { thread } if thread == "t1" => {
                        listed = browser
                            .pages(&thread)
                            .await
                            .into_iter()
                            .find(|view| view.id == page.id && view.size == size);
                    }
                    Change::Painted { page: id, .. } if id == page.id => {
                        painted = browser
                            .frame(&id)
                            .await
                            .filter(|frame| frame.number > before && (frame.width, frame.height) == (size.width, size.height));
                    }
                    _ => {}
                }
                if let (Some(listed), Some(painted)) = (&listed, &painted) {
                    return (listed.clone(), painted.clone());
                }
            }
        };
        let (listed, frame) = tokio::time::timeout(Duration::from_secs(5), synchronized)
            .await
            .expect("the page list and painted frame agree after a resize");
        assert_eq!(listed.size, size);
        assert_eq!((frame.width, frame.height), (size.width, size.height));
        assert!(frame.jpeg.starts_with(b"\xff\xd8"), "a JPEG");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_covered_element_is_refused_a_dialog_is_answered_and_the_page_goes_back_through_its_history() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    act(&browser, &machine, json!({ "action": "open", "url": address }))
        .await
        .expect("opened");
    let read = act(&browser, &machine, json!({ "action": "read" })).await.expect("read").text;
    let covered = act(
        &browser,
        &machine,
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
        &machine,
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
        &machine,
        json!({ "action": "cdp", "method": "Runtime.evaluate", "params": { "expression": "1" } }),
    )
    .await;
    assert!(held.expect_err("held").0.contains("dialog"));
    act(&browser, &machine, json!({ "action": "dialog", "accept": true }))
        .await
        .expect("answered");
    let went = act(
        &browser,
        &machine,
        json!({ "action": "click", "target": { "ref": ref_in(&read, "link", "Next") } }),
    )
    .await
    .expect("clicked");
    assert_eq!(went.text, format!("navigated to {address}/second"));
    let back = act(&browser, &machine, json!({ "action": "back" })).await.expect("went back");
    assert_eq!(back.page.expect("its page").title, "Form");
    // The page began blank, as it was set up before it loaded anything; before that, no entry.
    assert!(
        act(&browser, &machine, json!({ "action": "back" }))
            .await
            .expect("went back")
            .text
            .ends_with("about:blank")
    );
    assert!(
        act(&browser, &machine, json!({ "action": "back" })).await.is_err(),
        "no entry so far back"
    );
    let missing = act(
        &browser,
        &machine,
        json!({ "action": "click", "target": { "selector": "#nowhere" } }),
    )
    .await;
    let words = missing.expect_err("missing").0;
    assert!(words.contains("no element matches"), "{words}");
    let thrown = act(
        &browser,
        &machine,
        json!({ "action": "evaluate", "expression": "(() => { throw new Error('nope') })()" }),
    )
    .await;
    assert!(thrown.expect_err("thrown").0.contains("nope"));
}

#[tokio::test(flavor = "multi_thread")]
async fn tabs_are_the_thread_s_its_console_and_downloads_are_told_and_cdp_reaches_the_page() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    let first = act(&browser, &machine, json!({ "action": "open", "url": format!("{address}/opener") }))
        .await
        .expect("opened");
    let opener = first.page.expect("a page");
    let console = act(&browser, &machine, json!({ "action": "console" })).await.expect("read").text;
    assert_eq!(console, "#1 log: ready 42\n#2 error: trouble\n");
    act(&browser, &machine, json!({ "action": "click", "target": { "selector": "button" } }))
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
    let listed = act(&browser, &machine, json!({ "action": "tabs" })).await.expect("listed");
    let data = serde_json::to_value(&listed).expect("tabs");
    assert!(data["observation"]["pages"].as_array().expect("pages").len() >= 2);
    assert!(data["observation"]["current"].is_string());
    let tabs = listed.text;
    assert!(tabs.contains(&format!("{} (current)", opener.id)), "{tabs}");
    act(&browser, &machine, json!({ "action": "click", "target": { "selector": "a" } }))
        .await
        .expect("clicked");
    let waiting = async {
        loop {
            let told = act(&browser, &machine, json!({ "action": "downloads" }))
                .await
                .expect("listed")
                .text;
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
    let called = act(
        &browser,
        &machine,
        json!({ "action": "cdp", "method": "Runtime.evaluate", "params": params }),
    )
    .await
    .expect("called");
    assert_eq!(called.value.expect("a value")["result"]["value"], 4);
    act(&browser, &machine, json!({ "action": "close" })).await.expect("closed");
    assert!(
        act(&browser, &machine, json!({ "action": "switch", "page": "nonesuch" }))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_script_runs_inside_a_frame_of_another_site_s_process() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    act(&browser, &machine, json!({ "action": "open", "url": format!("{address}/framing") }))
        .await
        .expect("opened");
    act(&browser, &machine, json!({ "action": "wait", "selector": "iframe" }))
        .await
        .expect("framed");
    let expression = "document.getElementById('secret').textContent";
    let inside = async {
        loop {
            match act(
                &browser,
                &machine,
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

#[tokio::test(flavor = "multi_thread")]
async fn an_http_html_preview_loads_inside_an_opaque_sandbox_and_runs_isolated_scripts() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    act(&browser, &machine, json!({ "action": "open", "url": format!("{address}/preview") }))
        .await
        .expect("the embedded preview opened");
    let html = "<!doctype html><h1>Saved report</h1><h2>Details</h2><script>\
        let isolated = false;\
        try { parent.document.body.textContent = 'escaped'; } catch { isolated = true; }\
        addEventListener('DOMContentLoaded', () => {\
            const root = getComputedStyle(document.documentElement);\
            const body = getComputedStyle(document.body);\
            top.postMessage({ type: 'preview.script', isolated, scheme: root.colorScheme,\
                background: root.backgroundColor, foreground: body.color,\
                h1: getComputedStyle(document.querySelector('h1')).fontSize, h2: getComputedStyle(document.querySelector('h2')).fontSize, font: body.fontFamily, margin: body.margin, boxSizing: body.boxSizing, fontSize: root.fontSize, lineHeight: root.lineHeight, padding: root.padding, radius: root.getPropertyValue('--radius').trim() }, '*');\
        });\
        </script>";
    let payload = json!({ "type": "octoii.html.load", "html": html });
    let expression = format!(
        "new Promise(resolve => {{
            const frame = document.querySelector('iframe').contentWindow;
            const result = {{ loaded: false }};
            const finish = () => {{
                clearTimeout(timer);
                removeEventListener('message', receive);
                resolve(result);
            }};
            const receive = event => {{
                if (event.source === frame && event.data?.type === 'octoii.html.loaded') result.loaded = true;
                if (event.source === frame && event.data?.type === 'octoii.html.size') result.height = event.data.height;
                if (event.data?.type === 'preview.script') result.script = event.data;
                if (result.loaded && result.height > 0 && result.script) finish();
            }};
            const timer = setTimeout(finish, 5000);
            addEventListener('message', receive);
            frame.postMessage({payload}, '*');
        }})"
    );
    let outcome = act(&browser, &machine, json!({ "action": "evaluate", "expression": expression }))
        .await
        .expect("the embedded preview answered");
    let value = outcome.value.expect("the preview's state");
    assert_eq!(value["loaded"], true, "{value}");
    assert_eq!(value["script"]["isolated"], true, "{value}");
    assert_eq!(value["script"]["scheme"], "light dark", "{value}");
    assert_ne!(value["script"]["background"], value["script"]["foreground"], "{value}");
    assert!(
        value["script"]["font"].as_str().is_some_and(|font| font.contains("system-ui")),
        "{value}"
    );
    assert_eq!(value["script"]["margin"], "0px", "{value}");
    assert_eq!(value["script"]["fontSize"], "14px", "{value}");
    assert_eq!(value["script"]["lineHeight"], "22.75px", "{value}");
    assert_eq!(value["script"]["h1"], "16px", "{value}");
    assert_eq!(value["script"]["h2"], "15px", "{value}");
    assert_eq!(value["script"]["padding"], "8px", "{value}");
    assert_eq!(value["script"]["radius"], "0.625rem", "{value}");
    assert_eq!(value["script"]["boxSizing"], "border-box", "{value}");
    assert!(value["height"].as_f64().is_some_and(|height| height > 0.0), "{value}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_html_preview_changes_theme_in_place_and_its_height_shrinks_after_interaction() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    act(&browser, &machine, json!({ "action": "open", "url": format!("{address}/preview") }))
        .await
        .expect("the embedded preview opened");
    let light = json!({ "appearance": "light", "variables": {
        "--background": "rgb(250, 251, 252)", "--foreground": "rgb(20, 30, 40)",
        "--font-sans": "sans-serif", "--font-mono": "monospace",
        "--test-payload": "</script><script>top.postMessage('escaped', '*')</script>"
    }});
    let dark = json!({ "appearance": "dark", "variables": {
        "--background": "rgb(20, 30, 40)", "--foreground": "rgb(230, 240, 250)",
        "--font-sans": "serif", "--font-mono": "monospace"
    }});
    let payload = json!({ "type": "octoii.html.load", "html": THEMED_PREVIEW, "theme": light });
    let expression = format!(
        "new Promise(resolve => {{
            const element = document.querySelector('iframe');
            const frame = element.contentWindow;
            const result = {{ loads: 0, heights: [], escaped: false }};
            let content;
            let phase = 'initial';
            let state;
            let height;
            const finish = () => {{
                clearTimeout(timer);
                removeEventListener('message', receive);
                resolve(result);
            }};
            const receive = event => {{
                if (event.data === 'escaped') result.escaped = true;
                if (event.source === frame && event.data?.type === 'octoii.html.loaded') result.loads += 1;
                if (event.source === frame && event.data?.type === 'octoii.html.size') {{
                    height = event.data.height;
                    result.heights.push(height);
                    element.style.height = height + 'px';
                }}
                if (event.data?.type === 'preview.state') {{ content = event.source; state = event.data; }}
                if (phase === 'initial' && result.loads === 1 && height > 1400 && state?.stage === 'initial') {{
                    result.initial = state;
                    phase = 'updated';
                    frame.postMessage({{ type: 'octoii.html.theme', theme: {dark} }}, '*');
                    content.postMessage({{ type: 'preview.toggle', stage: 'updated' }}, '*');
                }} else if (phase === 'updated' && height < 100 && state?.stage === 'updated') {{
                    result.updated = state;
                    phase = 'spoofed';
                    content.postMessage({{ type: 'octoii.html.theme', theme: {light} }}, '*');
                    content.postMessage({{ type: 'preview.spoof', theme: {light}, stage: 'spoofed' }}, '*');
                }} else if (phase === 'spoofed' && state?.stage === 'spoofed') {{
                    result.spoofed = state;
                    finish();
                }}
            }};
            const timer = setTimeout(finish, 5000);
            addEventListener('message', receive);
            frame.postMessage({payload}, '*');
        }})"
    );
    let outcome = act(&browser, &machine, json!({ "action": "evaluate", "expression": expression }))
        .await
        .expect("the embedded preview answered");
    let value = outcome.value.expect("the preview's state");
    assert_eq!(value["loads"], 1, "theme changes preserve the document: {value}");
    assert_eq!(value["escaped"], false, "theme variables stay data: {value}");
    assert_eq!(value["initial"]["background"], "rgb(250, 251, 252)", "{value}");
    assert_eq!(value["initial"]["foreground"], "rgb(20, 30, 40)", "{value}");
    assert_eq!(value["initial"]["font"], "sans-serif", "{value}");
    assert_eq!(value["initial"]["mono"], "monospace", "{value}");
    assert_eq!(value["initial"]["scheme"], "light", "{value}");
    assert_eq!(value["initial"]["authoredFont"], "serif", "{value}");
    for stage in ["updated", "spoofed"] {
        assert_eq!(value[stage]["background"], "rgb(20, 30, 40)", "{value}");
        assert_eq!(value[stage]["foreground"], "rgb(230, 240, 250)", "{value}");
        assert_eq!(value[stage]["font"], "serif", "{value}");
        assert_eq!(value[stage]["scheme"], "dark", "{value}");
        assert_eq!(value[stage]["clicks"], 1, "script state survives: {value}");
        assert_eq!(value[stage]["authoredBackground"], "rgb(12, 34, 56)", "{value}");
        assert_eq!(value[stage]["authoredForeground"], "rgb(210, 220, 230)", "{value}");
        assert_eq!(value[stage]["authoredFont"], "serif", "{value}");
    }
    let heights = value["heights"].as_array().expect("reported heights");
    assert!(
        heights.iter().any(|height| height.as_f64().is_some_and(|height| height > 1400.0)),
        "{value}"
    );
    assert!(
        heights
            .last()
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|height| height < 100.0),
        "{value}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_local_html_preview_is_isolated_painted_clicked_and_its_console_is_read() {
    let beak = Beak::new().expect("Beak");
    let (profile, downloads) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let machine = machine(&beak, profile.path());
    let Some(browser) = browser(&beak, profile.path(), downloads.path()) else {
        return;
    };
    let address = serve().await;
    let html = format!(
        "<!doctype html><title>Preview</title>\
         <style>body{{background:white}}button{{position:absolute;left:20px;top:20px;width:180px;height:80px}}</style>\
         <button onclick=\"document.body.style.background='red';console.log('preview clicked')\">Click me</button>\
         <link rel='stylesheet' href='{address}/remote.css'>\
         <script src='{address}/remote.js'></script>\
         <img src='{address}/remote.svg' onload=\"console.log('remote image loaded')\">\
         <script>\
         console.log('preview ready');\
         fetch('{address}/remote-data').then(r => r.text()).then(console.log);\
         addEventListener('load', () => console.log('remote style ' + getComputedStyle(document.body).getPropertyValue('--remote-style')));\
         window.open('{address}/popup');\
         throw new Error('preview error');\
         </script>"
    );
    std::fs::write(profile.path().join("preview.html"), html).expect("a local HTML file");
    let preview = act(&browser, &machine, json!({ "action": "preview", "path": "preview.html" }))
        .await
        .expect("previewed");
    let page = preview.page.expect("its page");
    assert_eq!(browser.current("t1").await.as_deref(), Some(page.id.as_str()));
    assert_eq!(browser.pages("t1").await.len(), 1, "the preview opens no popup");
    let isolated = act(
        &browser,
        &machine,
        json!({ "action": "evaluate", "expression": "document.querySelector('iframe').contentDocument === null" }),
    )
    .await
    .expect("checked the frame");
    assert_eq!(isolated.value, Some(json!(true)), "the content has its own origin");
    let before = act(&browser, &machine, json!({ "action": "look" }))
        .await
        .expect("looked")
        .image
        .expect("a picture");
    assert!(before.starts_with(b"\xff\xd8"), "a JPEG");
    let console = act(&browser, &machine, json!({ "action": "console" }))
        .await
        .expect("its logs")
        .text;
    assert!(console.contains("preview ready"), "{console}");
    assert!(console.contains("preview error"), "{console}");
    assert!(console.contains("remote script loaded"), "{console}");
    assert!(console.contains("remote image loaded"), "{console}");
    assert!(console.contains("remote style loaded"), "{console}");
    assert!(console.contains("remote data loaded"), "{console}");
    act(&browser, &machine, json!({ "action": "click", "target": { "x": 100, "y": 60 } }))
        .await
        .expect("clicked inside the isolated frame");
    let after = act(&browser, &machine, json!({ "action": "look" }))
        .await
        .expect("looked again")
        .image
        .expect("a picture");
    assert_ne!(before, after, "the click changes the painted page");
    let console = act(&browser, &machine, json!({ "action": "console" }))
        .await
        .expect("its logs")
        .text;
    assert!(console.contains("preview clicked"), "{console}");

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
    assert!(schema.to_string().contains("\"preview\""), "{schema}");
}
