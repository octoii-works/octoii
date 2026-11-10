//! A page as the model works it: taken to an address, back or forward an entry, loaded again; its scripts run; painted
//! for the model to see; waited on till it shows what is asked; and, after each of these, waited on till it settles —
//! its document parsed, no request of the last 1.5 s in flight, and its DOM quiet — never a sleep of a fixed length.

use std::time::{Duration, Instant};

use base64::Engine as _;
use bytes::Bytes;
use serde_json::{Value, json};

use crate::browser::{Attached, Browser, BrowserError};

/// What tells how a page stands, installed once in each document it shows: whether it is parsed, how long its DOM has
/// been quiet, how often it changed, which document it is, its address and its title.
pub(crate) const PROBE: &str = r"(() => {
  const w = window;
  if (!w.__octoiiProbe) {
    const probe = { last: performance.now(), changes: 0, document: Math.random().toString(36).slice(2) };
    new MutationObserver((records) => {
      for (const r of records) {
        if (r.attributeName && r.oldValue === r.target.getAttribute(r.attributeName)) continue;
        probe.last = performance.now();
        probe.changes++;
        return;
      }
    }).observe(document, { subtree: true, childList: true, attributes: true, attributeOldValue: true, characterData: true });
    for (const kind of ['input', 'change']) addEventListener(kind, () => probe.changes++, true);
    Object.defineProperty(w, '__octoiiProbe', { value: probe });
  }
  const probe = w.__octoiiProbe;
  return { ready: document.readyState, idle: performance.now() - probe.last, changes: probe.changes,
           document: probe.document, url: location.href, title: document.title };
})()";

/// How a page stood as it was probed.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub ready: String,
    pub idle_ms: f64,
    pub changes: u64,
    /// Which document it is, by a name of its own: another as the page shows another.
    pub document: String,
    pub url: String,
    pub title: String,
}

/// Where a page landed: its address and title, and the browser's words for why it could not go where asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landed {
    pub url: String,
    pub title: String,
    pub failed: Option<String>,
}

/// What a script came to: its value, as JSON, or what it threw.
#[derive(Debug, Clone, PartialEq)]
pub enum Evaluated {
    Value(Value),
    Threw(String),
}

/// What a wait asks a page to show: text it shows, text it no longer shows, and an address holding a part.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wanted {
    pub text: Option<String>,
    pub gone: Option<String>,
    pub url: Option<String>,
}

/// A page of a thread's, as the model works it.
#[derive(Debug, Clone)]
pub struct Page {
    pub(crate) browser: Browser,
    pub id: String,
}

/// How long a page is given to settle after an action, and after a navigation.
pub(crate) const SETTLE_CAP: Duration = Duration::from_millis(800);
const LOAD_CAP: Duration = Duration::from_secs(15);
/// How long its DOM is to be quiet to count as settled.
const QUIET_MS: u64 = 80;

impl Browser {
    /// The page `id`, to work it.
    #[must_use]
    pub fn page(&self, id: &str) -> Page {
        Page {
            browser: self.clone(),
            id: id.to_owned(),
        }
    }
}

impl Page {
    pub(crate) async fn attached(&self) -> Result<Attached, BrowserError> {
        self.browser.attached(&self.id).await
    }

    /// Runs `expression` in the page's own world, waiting a second at most — a page a dialog holds answers nothing:
    /// its value by value.
    pub(crate) async fn run(&self, expression: &str) -> Result<Value, BrowserError> {
        let attached = self.attached().await?;
        let params = json!({ "expression": expression, "returnByValue": true, "awaitPromise": true });
        let ran = attached
            .cdp
            .call_within(Some(&attached.session), "Runtime.evaluate", params, Duration::from_secs(1))
            .await?;
        Ok(ran["result"]["value"].clone())
    }

    /// How the page stands now.
    ///
    /// # Errors
    /// The browser's refusal, as a page between documents gives.
    pub async fn probe(&self) -> Result<Probe, BrowserError> {
        let value = self.run(PROBE).await?;
        Ok(Probe {
            ready: value["ready"].as_str().unwrap_or_default().to_owned(),
            idle_ms: value["idle"].as_f64().unwrap_or_default(),
            changes: value["changes"].as_u64().unwrap_or_default(),
            document: value["document"].as_str().unwrap_or_default().to_owned(),
            url: value["url"].as_str().unwrap_or_default().to_owned(),
            title: value["title"].as_str().unwrap_or_default().to_owned(),
        })
    }

    /// Waits till the page settles — parsed, its network and DOM quiet since `since` — `cap` at most, longer while a
    /// document loads: how it stands then, where it could be probed.
    pub async fn settle(&self, since: Instant, cap: Duration) -> Option<Probe> {
        let start = Instant::now();
        let mut limit = cap;
        let mut last = None;
        while start.elapsed() < limit {
            // A dialog holds its scripts: nothing settles till it is answered.
            if self.browser.view(&self.id).await.is_some_and(|view| view.dialog.is_some()) {
                return last;
            }
            match self.probe().await {
                Ok(probe) => {
                    if probe.ready == "loading" {
                        limit = limit.max(Duration::from_secs(3));
                    }
                    let idle = probe.idle_ms.min(since.elapsed().as_secs_f64() * 1000.0);
                    let quiet_for = Duration::from_millis(QUIET_MS);
                    let network = self.browser.network(&self.id).await;
                    let quiet = probe.ready != "loading"
                        && idle >= quiet_for.as_secs_f64() * 1000.0
                        && network.in_flight == 0
                        && network.quiet_ms >= QUIET_MS;
                    last = Some(probe);
                    if quiet {
                        return last;
                    }
                }
                // Between documents, as a navigation goes: the next is waited for.
                Err(_) => last = None,
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        last
    }

    /// Takes the page to `url`, then waits till it settles: where it landed.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn navigate(&self, url: &str) -> Result<Landed, BrowserError> {
        let attached = self.attached().await?;
        let since = Instant::now();
        let went = attached
            .cdp
            .call(Some(&attached.session), "Page.navigate", json!({ "url": url }))
            .await?;
        let failed = went["errorText"].as_str().filter(|text| !text.is_empty()).map(str::to_owned);
        self.landed(since, failed).await
    }

    async fn landed(&self, since: Instant, failed: Option<String>) -> Result<Landed, BrowserError> {
        let probe = self.settle(since, LOAD_CAP).await;
        let view = self.browser.pages_of_page(&self.id).await;
        Ok(Landed {
            url: probe.as_ref().map_or_else(|| view.0.clone(), |probe| probe.url.clone()),
            title: probe.map_or(view.1, |probe| probe.title),
            failed,
        })
    }

    /// Takes the page `steps` entries through its history — back where negative — where it has them: where it landed,
    /// or none where it has no such entry.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn go(&self, steps: i64) -> Result<Option<Landed>, BrowserError> {
        let attached = self.attached().await?;
        let history = attached
            .cdp
            .call(Some(&attached.session), "Page.getNavigationHistory", json!({}))
            .await?;
        let at = history["currentIndex"].as_i64().unwrap_or_default() + steps;
        let Some(entry) = usize::try_from(at)
            .ok()
            .and_then(|at| history["entries"].as_array()?.get(at).cloned())
        else {
            return Ok(None);
        };
        let since = Instant::now();
        attached
            .cdp
            .call(
                Some(&attached.session),
                "Page.navigateToHistoryEntry",
                json!({ "entryId": entry["id"] }),
            )
            .await?;
        Ok(Some(self.landed(since, None).await?))
    }

    /// Loads the page again: where it landed.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn reload(&self) -> Result<Landed, BrowserError> {
        let attached = self.attached().await?;
        let since = Instant::now();
        attached.cdp.call(Some(&attached.session), "Page.reload", json!({})).await?;
        self.landed(since, None).await
    }

    /// Runs `expression` in the page, as its own script, awaiting a promise it gives: its value as JSON, or what it
    /// threw.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn evaluate(&self, expression: &str) -> Result<Evaluated, BrowserError> {
        let attached = self.attached().await?;
        let ran = attached
            .cdp
            .call(
                Some(&attached.session),
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true, "awaitPromise": true, "userGesture": true }),
            )
            .await?;
        if let Some(thrown) = ran.get("exceptionDetails") {
            let words = thrown["exception"]["description"]
                .as_str()
                .or_else(|| thrown["text"].as_str())
                .unwrap_or("it threw");
            return Ok(Evaluated::Threw(words.to_owned()));
        }
        Ok(Evaluated::Value(ran["result"]["value"].clone()))
    }

    /// The page as it is painted now, for the model to see: a JPEG of what its view shows.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn screenshot(&self) -> Result<Bytes, BrowserError> {
        let attached = self.attached().await?;
        let shot = attached
            .cdp
            .call(
                Some(&attached.session),
                "Page.captureScreenshot",
                json!({ "format": "jpeg", "quality": 80 }),
            )
            .await?;
        let data = shot["data"].as_str().unwrap_or_default();
        Ok(base64::engine::general_purpose::STANDARD.decode(data).unwrap_or_default().into())
    }

    /// Waits till the page shows what `wanted` asks — each part given, case counting — `timeout` at most: whether it
    /// did.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn wait(&self, wanted: &Wanted, timeout: Duration) -> Result<bool, BrowserError> {
        let start = Instant::now();
        let check = format!(
            "(() => {{ const text = document.body ? document.body.innerText : ''; return ({}) && ({}) && ({}); }})()",
            wanted
                .text
                .as_ref()
                .map_or_else(|| "true".into(), |text| format!("text.includes({})", json!(text))),
            wanted
                .gone
                .as_ref()
                .map_or_else(|| "true".into(), |gone| format!("!text.includes({})", json!(gone))),
            wanted
                .url
                .as_ref()
                .map_or_else(|| "true".into(), |url| format!("location.href.includes({})", json!(url))),
        );
        while start.elapsed() < timeout {
            if self.run(&check).await.ok().and_then(|value| value.as_bool()) == Some(true) {
                return Ok(true);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(false)
    }
}

impl Browser {
    /// The address and title the page `id` has, as its owner keeps them.
    pub(crate) async fn pages_of_page(&self, id: &str) -> (String, String) {
        self.view(id).await.map(|view| (view.url, view.title)).unwrap_or_default()
    }
}
