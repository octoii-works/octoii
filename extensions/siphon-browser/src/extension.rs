//! The browser as an extension of Siphon's: started on the machine's kernel as the host gives its program and folders,
//! the `browser` tool its model calls, and its methods and topics — a page opened, moved, answered and closed, the
//! person's mouse, keys and text on it, a thread's pages as they change and a page as it is painted.

use serde_json::{Value, json};
use siphon::{Builder, Caller, Extension, Lagged, Subscription};
use siphon_protocol::{Error, Nothing, Scope};
use tokio::sync::broadcast::error::RecvError;

use crate::protocol::{self, BrowserEvent, BrowserTopic, Method, PageDialog, PageKey, PageListed, PageMouse, View};
use crate::{Browser, BrowserError, BrowserHost, BrowserTool, Change, PageView, Size};

/// The browser, as a host adds it: the program it runs — one found on the machine, where none — the folder of its
/// profile, and the folder what a page downloads goes to — nothing downloaded, where none.
#[derive(Debug, Clone, Default)]
pub struct BrowserExtension {
    pub program: Option<std::path::PathBuf>,
    pub profile: Option<std::path::PathBuf>,
    pub downloads: Option<std::path::PathBuf>,
}

impl Extension for BrowserExtension {
    fn name(&self) -> &'static str {
        "browser"
    }

    fn build(self: Box<Self>, builder: &mut Builder) {
        let browser = Browser::start(BrowserHost {
            kernel: builder.kernel.clone(),
            machine: builder.machine.clone(),
            program: self.program,
            profile: self.profile,
            downloads: self.downloads,
        });
        let closing = browser.clone();
        builder.on_thread_deleted(move |thread| closing.close_thread(thread));
        let tool = BrowserTool { browser: browser.clone() };
        builder.tools(|tools| tools.with(tool));
        let doing = browser.clone();
        builder.methods(protocol::set(), Method::read, move |_: Caller, method| {
            let browser = doing.clone();
            async move { answered(&browser, method).await }
        });
        builder.topics(Scope::BrowserOperate, move |subscription: Subscription, topic: BrowserTopic| {
            let browser = browser.clone();
            async move { told(&browser, &subscription, topic).await }
        });
    }
}

/// A view as the browser sizes a page.
fn size(view: View) -> Size {
    Size {
        width: view.width,
        height: view.height,
        scale: view.scale.unwrap_or(1.0),
    }
}

/// Does `method` on `browser`.
async fn answered(browser: &Browser, method: Method) -> Result<Value, Error> {
    let done = match method {
        Method::Open(open) => {
            let page = browser
                .open(&open.thread, open.url.as_deref(), size(open.view))
                .await
                .map_err(|error| failed(&error))?;
            return Ok(json!(protocol::PageOpened { page }));
        }
        Method::Navigate(navigate) => browser.page(&navigate.page).navigate(&navigate.url).await.map(drop),
        Method::Back(of) => browser.page(&of.page).go(-1).await.map(drop),
        Method::Forward(of) => browser.page(&of.page).go(1).await.map(drop),
        Method::Reload(of) => browser.page(&of.page).reload().await.map(drop),
        Method::Dialog(answer) => {
            let params = json!({ "accept": answer.accept, "promptText": answer.text.unwrap_or_default() });
            browser
                .page(&answer.page)
                .cdp("Page.handleJavaScriptDialog", params)
                .await
                .map(drop)
        }
        Method::Close(of) => {
            browser.close(&of.page);
            Ok(())
        }
        Method::Resize(resize) => browser.resize(&resize.page, size(resize.view)).await,
        Method::Mouse(on) => browser
            .page(&on.page)
            .cdp("Input.dispatchMouseEvent", mouse(&on.event))
            .await
            .map(drop),
        Method::Key(on) => browser.page(&on.page).cdp("Input.dispatchKeyEvent", key(&on.event)).await.map(drop),
        Method::Text(typed) => {
            let params = json!({ "text": typed.text });
            browser.page(&typed.page).cdp("Input.insertText", params).await.map(drop)
        }
    };
    done.map_err(|error| failed(&error))?;
    Ok(json!(Nothing {}))
}

/// The person's mouse `event` as CDP dispatches it.
fn mouse(event: &PageMouse) -> Value {
    let kind = match event.motion {
        protocol::MouseMotion::Down => "mousePressed",
        protocol::MouseMotion::Up => "mouseReleased",
        protocol::MouseMotion::Move => "mouseMoved",
        protocol::MouseMotion::Wheel => "mouseWheel",
    };
    json!({
        "type": kind,
        "x": event.x,
        "y": event.y,
        "button": event.button,
        "buttons": event.buttons,
        "clickCount": event.clicks,
        "modifiers": event.modifiers,
        "deltaX": event.delta_x,
        "deltaY": event.delta_y,
    })
}

/// The person's key `event` as CDP dispatches it: a key that types nothing pressed raw, as a browser's own input does.
fn key(event: &PageKey) -> Value {
    let kind = match (event.pressed, &event.text) {
        (true, Some(_)) => "keyDown",
        (true, None) => "rawKeyDown",
        (false, _) => "keyUp",
    };
    let mut params = json!({
        "type": kind,
        "key": event.key,
        "code": event.code,
        "modifiers": event.modifiers,
        "windowsVirtualKeyCode": event.key_code,
        "commands": event.commands,
    });
    if let Some(text) = &event.text {
        params["text"] = json!(text);
        params["unmodifiedText"] = json!(text);
    }
    params
}

/// Why the browser did not do what was asked, as a client is told.
fn failed(error: &BrowserError) -> Error {
    let code = match &error {
        BrowserError::NoSuchPage => "no_such_page",
        BrowserError::NoBrowser => "no_browser",
        _ => "failed",
    };
    Error {
        code: code.to_owned(),
        message: error.to_string(),
    }
}

/// A page as a list of a thread's shows it.
fn listed(view: PageView) -> PageListed {
    PageListed {
        id: view.id,
        url: view.url,
        title: view.title,
        view: View {
            width: view.size.width,
            height: view.size.height,
            scale: Some(view.size.scale),
        },
        dialog: view.dialog.map(|dialog| PageDialog {
            kind: dialog.kind,
            message: dialog.message,
            offered: dialog.offered,
        }),
        loading: view.loading,
        back: view.back,
        forward: view.forward,
    }
}

/// Tells `topic` to `subscription`'s client: a thread's pages, each time they change; or a page's newest frame, each
/// time it is painted — a frame the client has no room for passed over, as the next makes it old.
async fn told(browser: &Browser, subscription: &Subscription, topic: BrowserTopic) -> Result<(), Lagged> {
    let sink = &subscription.sink;
    let mut changes = browser.events();
    let lagged = || Lagged(serde_json::to_value(&topic).unwrap_or_default());
    let tell_pages = |pages: Vec<PageView>| {
        let pages = pages.into_iter().map(listed).collect();
        sink.offer(&BrowserEvent::Pages {
            subscription: sink.id().to_owned(),
            pages,
        })
    };
    let tell_frame = |frame: crate::Frame| {
        let event = BrowserEvent::Frame {
            subscription: sink.id().to_owned(),
            number: frame.number,
            width: frame.width,
            height: frame.height,
        };
        let _ = sink.offer_with(&event, &frame.jpeg);
    };
    match &topic {
        BrowserTopic::Pages { thread } => tell_pages(browser.pages(thread).await).map_err(|_| lagged())?,
        BrowserTopic::Page { page } => {
            if let Some(frame) = browser.frame(page).await {
                tell_frame(frame);
            }
        }
    }
    loop {
        let change = tokio::select! {
            () = subscription.stop.cancelled() => return Ok(()),
            change = changes.recv() => change,
        };
        match (&topic, change) {
            (_, Err(RecvError::Closed)) => return Ok(()),
            (BrowserTopic::Pages { thread }, Ok(Change::Pages { thread: of })) if of == *thread => {
                tell_pages(browser.pages(thread).await).map_err(|_| lagged())?;
            }
            (BrowserTopic::Pages { thread }, Err(RecvError::Lagged(_))) => {
                tell_pages(browser.pages(thread).await).map_err(|_| lagged())?;
            }
            (BrowserTopic::Page { page }, Ok(Change::Painted { page: painted, .. })) if painted == *page => {
                if let Some(frame) = browser.frame(page).await {
                    tell_frame(frame);
                }
            }
            _ => {}
        }
    }
}
