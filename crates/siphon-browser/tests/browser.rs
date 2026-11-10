//! A thread's pages in a real headless Chromium, where the machine has one: opened, titled, painted, spoken to, and
//! closed. Where it has none, each test says so and passes.

use std::time::Duration;

use beak::{Beak, Kernel, Options};
use serde_json::json;
use std::collections::HashSet;

use siphon_browser::{Action, Browser, BrowserError, BrowserEvent, BrowserHost, Button, Evaluated, Size, Target, Wanted, node_of};
use siphon_machine::Host;

const SIZE: Size = Size {
    width: 800,
    height: 600,
    scale: 1.0,
};

fn browser(beak: &Beak, profile: &std::path::Path) -> Browser {
    let kernel = Kernel::open(beak, &Options::default()).expect("a kernel").0;
    Browser::start(BrowserHost {
        kernel,
        machine: Host {
            home: std::env::var_os("HOME").map(Into::into),
            ..Host::default()
        },
        program: None,
        profile: Some(profile.to_owned()),
        downloads: None,
    })
}

async fn until(events: &mut tokio::sync::broadcast::Receiver<BrowserEvent>, done: impl Fn(&BrowserEvent) -> bool) {
    let waiting = async {
        loop {
            if done(&events.recv().await.expect("an event")) {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(20), waiting).await.expect("in time");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_is_opened_for_a_thread_titled_painted_spoken_to_and_closed() {
    let beak = Beak::new().expect("Beak");
    let profile = tempfile::tempdir().expect("a profile");
    let browser = browser(&beak, profile.path());
    let mut events = browser.events();
    let page = match browser
        .open("t1", Some("data:text/html,<title>Hello</title><h1>Hi</h1>"), SIZE)
        .await
    {
        Err(BrowserError::NoBrowser) => {
            eprintln!("no browser on this machine");
            return;
        }
        other => other.expect("opened"),
    };
    until(&mut events, |event| matches!(event, BrowserEvent::Painted { .. })).await;
    let waiting = async {
        loop {
            if browser.pages("t1").await.first().is_some_and(|view| view.title == "Hello") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let titled = tokio::time::timeout(Duration::from_secs(10), waiting).await;
    assert!(titled.is_ok(), "not titled: {:?}", browser.pages("t1").await);
    let frame = browser.frame(&page).await.expect("a frame");
    assert!(frame.jpeg.starts_with(b"\xff\xd8"), "a JPEG");
    let attached = browser.attached(&page).await.expect("attached");
    let read = attached
        .cdp
        .call(
            Some(&attached.session),
            "Runtime.evaluate",
            json!({ "expression": "document.querySelector('h1').textContent", "returnByValue": true }),
        )
        .await
        .expect("evaluated");
    assert_eq!(read["result"]["value"], "Hi");
    assert!(browser.pages("t2").await.is_empty(), "another thread's are its own");
    browser.close(&page);
    let waiting = async {
        while !browser.pages("t1").await.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("closed");
}

/// A form the model works: a field, a button that greets what was typed, a choice, a button a box covers, one that
/// raises a dialog, and a link to a second page.
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

/// Serves `/` the form and `/second` a page of its own, over HTTP on a port of the machine's: its address.
async fn serve() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("its address"));
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
                let (kind, body) = if line.starts_with("GET /second") {
                    ("text/html", "<title>Second</title><h1>Second</h1>")
                } else if line.starts_with("GET /file") {
                    (
                        "application/octet-stream\r\ncontent-disposition: attachment; filename=\"notes.txt\"",
                        "the notes",
                    )
                } else if line.starts_with("GET /opener") {
                    ("text/html", OPENER)
                } else {
                    ("text/html", FORM)
                };
                let answer = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(answer.as_bytes()).await;
            });
        }
    });
    address
}

/// The ref of the line of `snapshot` that holds `role` and `name`.
fn ref_in(snapshot: &str, role: &str, name: &str) -> i64 {
    let line = snapshot
        .lines()
        .find(|line| line.contains(&format!("- {role} \"{name}\"")))
        .unwrap_or_else(|| panic!("no {role} {name} in:\n{snapshot}"));
    let at = line.find("ref=").expect("a ref") + 4;
    node_of(line[at..].split(']').next().expect("its end")).expect("a node")
}

/// The form opened for a thread, waited on till it shows, and read: none where the machine has no browser.
async fn form(browser: &Browser, address: &str) -> Option<(siphon_browser::Page, String, siphon_browser::Snapshot)> {
    let id = match browser.open("t1", Some(address), SIZE).await {
        Err(BrowserError::NoBrowser) => return None,
        other => other.expect("opened"),
    };
    let page = browser.page(&id);
    let shown = Wanted {
        text: Some("Greet".into()),
        ..Wanted::default()
    };
    assert!(page.wait(&shown, Duration::from_secs(10)).await.expect("waited"));
    let read = page.snapshot(&HashSet::new()).await.expect("read");
    Some((page, id, read))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_is_read_as_a_screen_reader_reads_it_and_acted_on_as_a_person_does() {
    let beak = Beak::new().expect("Beak");
    let profile = tempfile::tempdir().expect("a profile");
    let browser = browser(&beak, profile.path());
    let address = serve().await;
    let Some((page, id, read)) = form(&browser, &address).await else {
        return;
    };
    let field = ref_in(&read.text, "textbox", "Name");
    let greet = ref_in(&read.text, "button", "Greet");
    let typed = page
        .act(&Action::Type {
            node: Some(field),
            text: "Ada".into(),
            clear: true,
            submit: false,
        })
        .await
        .expect("typed");
    assert_eq!(typed, "the page changed");
    let clicked = page
        .act(&Action::Click {
            target: Target::Node(greet),
            button: Button::Left,
            clicks: 1,
        })
        .await
        .expect("clicked");
    assert_eq!(clicked, "the page changed");
    assert_eq!(
        page.evaluate("document.getElementById('out').textContent").await.expect("ran"),
        Evaluated::Value(json!("Hello Ada"))
    );
    let again = page.snapshot(&read.refs.iter().copied().collect()).await.expect("read");
    assert!(
        again.text.contains("StaticText \"Hello Ada\"") || again.text.contains("\"Hello Ada\""),
        "{}",
        again.text
    );
    let colour = ref_in(&read.text, "combobox", "Colour");
    let chose = page
        .act(&Action::Select {
            node: colour,
            values: vec!["Green".into()],
        })
        .await
        .expect("chose");
    assert!(chose.starts_with("chose [\"Green\"]"), "{chose}");
    let _ = id;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_covered_element_is_not_clicked_a_dialog_is_answered_and_a_page_goes_back_through_its_history() {
    let beak = Beak::new().expect("Beak");
    let profile = tempfile::tempdir().expect("a profile");
    let browser = browser(&beak, profile.path());
    let address = serve().await;
    let Some((page, id, read)) = form(&browser, &address).await else {
        return;
    };
    let under = ref_in(&read.text, "button", "Hidden");
    let refused = page
        .act(&Action::Click {
            target: Target::Node(under),
            button: Button::Left,
            clicks: 1,
        })
        .await;
    let words = refused.expect_err("covered").to_string();
    assert!(
        words.contains("covered by div#cover") && words.contains("nothing was done"),
        "{words}"
    );
    let warn = ref_in(&read.text, "button", "Warn");
    let warned = page
        .act(&Action::Click {
            target: Target::Node(warn),
            button: Button::Left,
            clicks: 1,
        })
        .await
        .expect("clicked");
    assert!(warned.contains("a alert dialog holds the page: \"careful\""), "{warned}");
    page.act(&Action::Dialog { accept: true, text: None }).await.expect("answered");
    assert!(browser.view(&id).await.expect("the page").dialog.is_none());
    let next = ref_in(&read.text, "link", "Next");
    let went = page
        .act(&Action::Click {
            target: Target::Node(next),
            button: Button::Left,
            clicks: 1,
        })
        .await
        .expect("clicked");
    assert_eq!(went, format!("navigated to {address}/second"));
    let back = page.go(-1).await.expect("went").expect("an entry back");
    assert_eq!(back.title, "Form");
    assert!(page.go(-5).await.expect("went").is_none(), "no entry so far back");
    let shot = page.screenshot().await.expect("painted");
    assert!(shot.starts_with(b"\xff\xd8"), "a JPEG");
    let thrown = page.evaluate("(() => { throw new Error('nope') })()").await.expect("ran");
    assert!(matches!(thrown, Evaluated::Threw(words) if words.contains("nope")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_a_page_opens_is_the_thread_s_its_console_is_kept_and_what_it_downloads_is_told() {
    let beak = Beak::new().expect("Beak");
    let profile = tempfile::tempdir().expect("a profile");
    let downloads = tempfile::tempdir().expect("a folder");
    let kernel = Kernel::open(&beak, &Options::default()).expect("a kernel").0;
    let browser = Browser::start(BrowserHost {
        kernel,
        machine: Host {
            home: std::env::var_os("HOME").map(Into::into),
            ..Host::default()
        },
        program: None,
        profile: Some(profile.path().to_owned()),
        downloads: Some(downloads.path().canonicalize().expect("its real path")),
    });
    let address = serve().await;
    let id = match browser.open("t1", Some(&format!("{address}/opener")), SIZE).await {
        Err(BrowserError::NoBrowser) => return,
        other => other.expect("opened"),
    };
    let page = browser.page(&id);
    assert!(
        page.wait(
            &Wanted {
                text: Some("Open".into()),
                ..Wanted::default()
            },
            Duration::from_secs(10)
        )
        .await
        .expect("waited")
    );
    let messages = browser.messages(&id, 0).await;
    let heard: Vec<(&str, &str)> = messages
        .iter()
        .map(|message| (message.level.as_str(), message.text.as_str()))
        .collect();
    assert_eq!(heard, [("log", "ready 42"), ("error", "trouble")]);
    let read = page.snapshot(&HashSet::new()).await.expect("read");
    let open = ref_in(&read.text, "button", "Open");
    page.act(&Action::Click {
        target: Target::Node(open),
        button: Button::Left,
        clicks: 1,
    })
    .await
    .expect("clicked");
    let waiting = async {
        loop {
            let pages = browser.pages("t1").await;
            if pages.len() == 2 && pages[1].title == "Second" {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("the page it opened is the thread's");
    let notes = ref_in(&read.text, "link", "Notes");
    page.act(&Action::Click {
        target: Target::Node(notes),
        button: Button::Left,
        clicks: 1,
    })
    .await
    .expect("clicked");
    let waiting = async {
        loop {
            if let Some(download) = browser
                .downloads(&id)
                .await
                .into_iter()
                .find(|download| download.state == "completed")
            {
                return download;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let download = tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("downloaded in time");
    assert_eq!(download.name, "notes.txt");
    let kept = std::fs::read(download.path.expect("where it was kept")).expect("read");
    assert_eq!(kept, b"the notes");
}
