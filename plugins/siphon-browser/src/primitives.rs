//! The primitives a script works a page with, as a browser harness gives them: raw CDP on the page's target — refused
//! while a dialog holds it, as every call would hang — and its events as they came; how the page stands and is laid
//! out; waits for it to load, for an element, for its network to go quiet; a field filled and files given by selector;
//! and script run inside a frame of another site's process, through a session of the frame's own.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::browser::BrowserError;
use crate::cdp::CdpError;
use crate::page::{Evaluated, Page};

/// How long a script run in a frame waits for the frame's answer.
const FRAME_WAIT: Duration = Duration::from_secs(5);

/// The methods a page a dialog holds still takes: the dialog's own answer, and the browser's targets.
fn taken_under_dialog(method: &str) -> bool {
    method == "Page.handleJavaScriptDialog" || method.starts_with("Target.") || method.starts_with("Browser.")
}

/// How a page stands and is laid out: its address and title; its view's width and height; how far it is scrolled;
/// its whole width and height. Or the dialog that holds it.
#[derive(Debug, Clone, PartialEq)]
pub enum PageInfo {
    Shown {
        url: String,
        title: String,
        width: f64,
        height: f64,
        scroll_x: f64,
        scroll_y: f64,
        page_width: f64,
        page_height: f64,
    },
    Dialog {
        kind: String,
        message: String,
    },
}

const INFO: &str = "({ url: location.href, title: document.title, width: innerWidth, height: innerHeight, scroll_x: scrollX, scroll_y: scrollY, page_width: document.documentElement.scrollWidth, page_height: document.documentElement.scrollHeight })";

impl Page {
    /// Calls `method` with `params` on the page's target, as a script asks: refused while a dialog holds the page, but
    /// for the dialog's answer and the browser's targets.
    ///
    /// # Errors
    /// `Refused` naming the dialog; the browser's refusal.
    pub async fn cdp(&self, method: &str, params: Value) -> Result<Value, BrowserError> {
        if !taken_under_dialog(method)
            && let Some(dialog) = self.browser.view(&self.id).await.and_then(|view| view.dialog)
        {
            let message = format!(
                "a {} dialog holds the page: {}; answer it first",
                dialog.kind,
                json!(dialog.message)
            );
            return Err(BrowserError::Cdp(CdpError::Refused { code: 0, message }));
        }
        let attached = self.attached().await?;
        Ok(attached.cdp.call(Some(&attached.session), method, params).await?)
    }

    /// Calls `method` with `params` on the target of another session — a frame's, a worker's — as a script asks.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn cdp_on(&self, session: &str, method: &str, params: Value) -> Result<Value, BrowserError> {
        let attached = self.attached().await?;
        Ok(attached.cdp.call(Some(session), method, params).await?)
    }

    /// The page's own events since they were last taken, as they came.
    pub async fn drain_events(&self) -> Vec<Value> {
        self.browser.drain_events(&self.id).await
    }

    /// How the page stands and is laid out, or the dialog that holds it.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn info(&self) -> Result<PageInfo, BrowserError> {
        if let Some(dialog) = self.browser.view(&self.id).await.and_then(|view| view.dialog) {
            return Ok(PageInfo::Dialog {
                kind: dialog.kind,
                message: dialog.message,
            });
        }
        let value = self.run(INFO).await?;
        let number = |name: &str| value[name].as_f64().unwrap_or_default();
        Ok(PageInfo::Shown {
            url: value["url"].as_str().unwrap_or_default().to_owned(),
            title: value["title"].as_str().unwrap_or_default().to_owned(),
            width: number("width"),
            height: number("height"),
            scroll_x: number("scroll_x"),
            scroll_y: number("scroll_y"),
            page_width: number("page_width"),
            page_height: number("page_height"),
        })
    }

    /// Waits till the page's document has loaded and the page settles, `timeout` at most: whether it did.
    pub async fn wait_for_load(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.run("document.readyState").await.ok().as_ref().and_then(Value::as_str) == Some("complete") {
                return self.settle(start, timeout.saturating_sub(start.elapsed())).await.is_some();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    /// Waits till an element `selector` picks is on the page — and drawn, where `visible` — `timeout` at most: whether
    /// one was.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn wait_for_element(&self, selector: &str, visible: bool, timeout: Duration) -> Result<bool, BrowserError> {
        let check = format!(
            "(() => {{ const e = document.querySelector({}); return !!e && (!{visible} || e.checkVisibility({{ checkOpacity: true, checkVisibilityCSS: true }})); }})()",
            json!(selector)
        );
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.run(&check).await.ok().and_then(|value| value.as_bool()) == Some(true) {
                return Ok(true);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(false)
    }

    /// Waits till no request of the page's is in flight and none began or ended for `idle`, `timeout` at most: whether
    /// it went quiet.
    pub async fn wait_for_network_idle(&self, idle: Duration, timeout: Duration) -> bool {
        let start = Instant::now();
        let idle_ms = u64::try_from(idle.as_millis()).unwrap_or(u64::MAX);
        while start.elapsed() < timeout {
            let network = self.browser.network(&self.id).await;
            if network.in_flight == 0 && network.quiet_ms >= idle_ms {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    /// The browser's id of the node of the element `selector` picks in the page's document; none where none does.
    async fn node_by_selector(&self, selector: &str) -> Result<Option<i64>, BrowserError> {
        let document = self.cdp("DOM.getDocument", json!({ "depth": 0 })).await?;
        let root = document["root"]["nodeId"].clone();
        let found = self
            .cdp("DOM.querySelector", json!({ "nodeId": root, "selector": selector }))
            .await?;
        let node = found["nodeId"].as_i64().filter(|node| *node > 0);
        let Some(node) = node else { return Ok(None) };
        let described = self.cdp("DOM.describeNode", json!({ "nodeId": node })).await?;
        Ok(described["node"]["backendNodeId"].as_i64())
    }

    /// The node of the element `selector` picks; refused, saying so, where none does.
    ///
    /// # Errors
    /// `Refused` where nothing matches; the browser's refusal.
    pub async fn node(&self, selector: &str) -> Result<i64, BrowserError> {
        self.node_by_selector(selector).await?.ok_or_else(|| {
            BrowserError::Cdp(CdpError::Refused {
                code: 0,
                message: format!("no element matches {}", json!(selector)),
            })
        })
    }

    /// Gives the files at `paths`, absolute paths of the machine, to the file input `selector` picks.
    ///
    /// # Errors
    /// `Refused` where nothing matches or it takes no files; the browser's refusal.
    pub async fn upload_file(&self, selector: &str, paths: &[String]) -> Result<(), BrowserError> {
        let node = self.node(selector).await?;
        self.cdp("DOM.setFileInputFiles", json!({ "backendNodeId": node, "files": paths }))
            .await?;
        Ok(())
    }

    /// The session of the frame of another site's process whose address holds `url_part`, as the browser attached to
    /// it when it was made; none where the page has no such frame.
    pub async fn frame_session(&self, url_part: &str) -> Option<String> {
        self.browser.frame_session(&self.id, url_part).await
    }

    /// Runs `expression` as the page's own script — or a frame's, through its session — awaiting a promise it gives:
    /// its value as JSON, or what it threw.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn js(&self, expression: &str, frame: Option<&str>) -> Result<Evaluated, BrowserError> {
        let Some(session) = frame else {
            return self.evaluate(expression).await;
        };
        let params = json!({ "expression": expression, "returnByValue": true, "awaitPromise": true, "userGesture": true });
        // A frame just attached to answers once its document is there; one not there in time is told so, to try again.
        let attached = self.attached().await?;
        let ran = attached
            .cdp
            .call_within(Some(session), "Runtime.evaluate", params, FRAME_WAIT)
            .await
            .map_err(|error| match error {
                CdpError::TimedOut(_) => BrowserError::Cdp(CdpError::Refused {
                    code: 0,
                    message: "the frame gave no answer yet: its document may still be loading; try again".into(),
                }),
                other => BrowserError::Cdp(other),
            })?;
        if let Some(thrown) = ran.get("exceptionDetails") {
            let words = thrown["exception"]["description"]
                .as_str()
                .or_else(|| thrown["text"].as_str())
                .unwrap_or("it threw");
            return Ok(Evaluated::Threw(words.to_owned()));
        }
        Ok(Evaluated::Value(ran["result"]["value"].clone()))
    }
}
