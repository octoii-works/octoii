//! Input to a page as a person gives it: on an element by its ref, or at a point in its CSS pixels — clicked, hovered,
//! typed into, keys pressed, an option selected, scrolled, dragged, files given to an upload, a dialog answered. An
//! element is scrolled into view and clicked at its middle only where nothing covers it there; where something does,
//! nothing is clicked and what covers it is named. Each action waits till the page settles and tells what came of it:
//! it navigated, the page changed, or nothing did — and a dialog it raised.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::browser::{Attached, BrowserError};
use crate::cdp::CdpError;
use crate::keys;
use crate::page::{Page, SETTLE_CAP};

/// Where an action is done: on an element by its node, or at a point of the page's view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Node(i64),
    Point { x: f64, y: f64 },
}

/// A mouse button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Button {
    #[default]
    Left,
    Middle,
    Right,
}

/// What a person gives a page: a click, keys, text, a choice, a scroll, a drag, files, a dialog's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Click {
        target: Target,
        button: Button,
        clicks: u32,
    },
    Hover {
        target: Target,
    },
    /// Text typed at the caret of the element given, focused first, or of what has the focus; what is there replaced
    /// where `clear`, and Enter pressed after where `submit`.
    Type {
        node: Option<i64>,
        text: String,
        clear: bool,
        submit: bool,
    },
    /// A key pressed, as the DOM names it, with modifiers, to what has the focus.
    Press {
        key: String,
        modifiers: Vec<String>,
    },
    /// Options of a `<select>` chosen, by value or label.
    Select {
        node: i64,
        values: Vec<String>,
    },
    /// Scrolled by pixels, over the element or point, or the page's middle.
    Scroll {
        target: Option<Target>,
        delta_x: f64,
        delta_y: f64,
    },
    Drag {
        from: Target,
        to: Target,
    },
    /// Files of the machine, by absolute paths, given to a file input.
    Upload {
        node: i64,
        files: Vec<String>,
    },
    /// The dialog that holds the page answered, a prompt's text with it.
    Dialog {
        accept: bool,
        text: Option<String>,
    },
}

/// Why an action was not done, in words the model reads.
#[derive(Debug, Clone, thiserror::Error)]
pub enum InputError {
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Browser(#[from] BrowserError),
}

/// What tells where an element's middle is in the page's view — through the frames it is in — and whether something
/// else is drawn there, named as a person would know it.
const POINT: &str = r"function() {
  const e = this.nodeType === Node.ELEMENT_NODE ? this : this.parentElement;
  if (!e || !e.isConnected) throw Error('the element is gone from the page');
  if (e.matches(':disabled') || e.closest('[inert]')) throw Error('the element is disabled');
  if (e.matches('input[type=file]')) throw Error('it is a file input: give it files with upload instead of clicking it');
  if (!e.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) throw Error('the element is hidden');
  const r = e.getBoundingClientRect(), x = r.x + r.width / 2, y = r.y + r.height / 2;
  if (!r.width || !r.height || x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) throw Error('the element is outside the view');
  const hit = e.getRootNode().elementFromPoint(x, y);
  const covered = hit && !e.contains(hit) && !(hit.contains(e) && hit.tagName === 'LABEL');
  const name = (n) => {
    const label = n.getAttribute('aria-label') || n.getAttribute('title') || (n.innerText || '').trim().slice(0, 60);
    return n.tagName.toLowerCase() + (n.id ? '#' + n.id : '') + (label ? ' ' + JSON.stringify(label) : '');
  };
  let px = x, py = y;
  for (let w = e.ownerDocument.defaultView; w && w.frameElement; w = w.parent) {
    const f = w.frameElement, fr = f.getBoundingClientRect(), cs = getComputedStyle(f);
    px += fr.x + f.clientLeft + parseFloat(cs.paddingLeft);
    py += fr.y + f.clientTop + parseFloat(cs.paddingTop);
  }
  return { x: px, y: py, covered: covered ? name(hit) : null };
}";

const CENTER: &str = "function() { const e = this.nodeType === 1 ? this : this.parentElement; e.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' }); }";

const CLEAR: &str = "function() { const e = this.nodeType === 1 ? this : this.parentElement; if ('value' in e) { e.select ? e.select() : null; } else if (e.isContentEditable) { const s = getSelection(); s.selectAllChildren(e); } }";

const SELECT: &str = r"function(wanted) {
  const norm = (s) => String(s).replace(/\s+/g, ' ').trim().toLowerCase();
  if (!(this instanceof HTMLSelectElement)) throw Error('the element is no <select>');
  const options = [...this.options].filter((o) => !o.disabled);
  const chosen = wanted.map((w) => {
    const o = options.find((o) => norm(o.value) === norm(w) || norm(o.label) === norm(w));
    if (!o) throw Error('no option ' + JSON.stringify(w) + '; its options: ' + options.slice(0, 40).map((o) => o.label).join(' | '));
    return o;
  });
  for (const o of options) o.selected = chosen.includes(o);
  this.dispatchEvent(new Event('input', { bubbles: true }));
  this.dispatchEvent(new Event('change', { bubbles: true }));
  return chosen.map((o) => o.label);
}";

/// Calls `function` on the element of `node`, with `arguments`: what it gives, or the words it threw.
async fn on_node(attached: &Attached, node: i64, function: &str, arguments: &[Value]) -> Result<Value, InputError> {
    let session = Some(attached.session.as_str());
    let resolved = attached
        .cdp
        .call(session, "DOM.resolveNode", json!({ "backendNodeId": node }))
        .await
        .map_err(|_| InputError::Refused(format!("no element e{node} on the page now; read it again")))?;
    let object = resolved["object"]["objectId"].clone();
    let arguments: Vec<Value> = arguments.iter().map(|value| json!({ "value": value })).collect();
    let called = attached
        .cdp
        .call(session, "Runtime.callFunctionOn", json!({ "functionDeclaration": function, "objectId": object, "arguments": arguments, "returnByValue": true, "awaitPromise": true }))
        .await
        .map_err(BrowserError::from)?;
    let _ = attached
        .cdp
        .call(session, "Runtime.releaseObject", json!({ "objectId": object }))
        .await;
    if let Some(thrown) = called.get("exceptionDetails") {
        let words = thrown["exception"]["description"]
            .as_str()
            .or_else(|| thrown["text"].as_str())
            .unwrap_or("it failed");
        return Err(InputError::Refused(
            words.trim_start_matches("Error: ").lines().next().unwrap_or_default().to_owned(),
        ));
    }
    Ok(called["result"]["value"].clone())
}

/// The point of the page's view `target` is at: an element's middle, scrolled into view and, where something covers
/// it, to the view's middle; refused where something covers it still.
async fn point_of(attached: &Attached, target: Target) -> Result<(f64, f64), InputError> {
    let node = match target {
        Target::Point { x, y } => return Ok((x, y)),
        Target::Node(node) => node,
    };
    let _ = attached
        .cdp
        .call(
            Some(&attached.session),
            "DOM.scrollIntoViewIfNeeded",
            json!({ "backendNodeId": node }),
        )
        .await;
    let mut point = on_node(attached, node, POINT, &[]).await?;
    if !point["covered"].is_null() {
        on_node(attached, node, CENTER, &[]).await?;
        point = on_node(attached, node, POINT, &[]).await?;
    }
    if let Some(cover) = point["covered"].as_str() {
        return Err(InputError::Refused(format!(
            "e{node} is covered by {cover} there; nothing was done"
        )));
    }
    Ok((point["x"].as_f64().unwrap_or_default(), point["y"].as_f64().unwrap_or_default()))
}

/// How long an input event waits for the page to take it: past it, a handler of the page's holds it — a dialog it
/// raised, or work of its own — and the event is taken as given, the page then waited on to settle.
const INPUT_WAIT: Duration = Duration::from_secs(2);

/// Sends the input event `method` with `params`.
async fn send(attached: &Attached, method: &str, params: Value) -> Result<(), InputError> {
    match attached.cdp.call_within(Some(&attached.session), method, params, INPUT_WAIT).await {
        Ok(_) | Err(CdpError::TimedOut(_)) => Ok(()),
        Err(error) => Err(BrowserError::from(error).into()),
    }
}

async fn mouse(attached: &Attached, kind: &str, (x, y): (f64, f64), button: &str, clicks: u32, extra: Value) -> Result<(), InputError> {
    let mut event = json!({ "type": kind, "x": x, "y": y, "button": button, "clickCount": clicks });
    if let (Value::Object(event), Value::Object(extra)) = (&mut event, extra) {
        event.extend(extra);
    }
    send(attached, "Input.dispatchMouseEvent", event).await
}

async fn press(attached: &Attached, name: &str, modifier_names: &[String]) -> Result<(), InputError> {
    let key = keys::key(name)
        .ok_or_else(|| InputError::Refused(format!("no key {name}; name it as the DOM does, as Enter, Tab, ArrowDown or a")))?;
    let modifiers = keys::modifiers(modifier_names);
    let typed = key.text.as_ref().filter(|_| modifiers & 0b0110 == 0);
    let mut down = json!({ "type": if typed.is_some() { "keyDown" } else { "rawKeyDown" }, "key": key.name, "code": key.code, "windowsVirtualKeyCode": key.windows_code, "modifiers": modifiers });
    if let Some(text) = typed {
        down["text"] = json!(text);
    }
    send(attached, "Input.dispatchKeyEvent", down).await?;
    let up =
        json!({ "type": "keyUp", "key": key.name, "code": key.code, "windowsVirtualKeyCode": key.windows_code, "modifiers": modifiers });
    send(attached, "Input.dispatchKeyEvent", up).await
}

/// Clicks `target` with `button`, `clicks` times in a row, the pointer moved there first.
async fn click(attached: &Attached, target: Target, button: Button, clicks: u32) -> Result<(), InputError> {
    let at = point_of(attached, target).await?;
    let name = match button {
        Button::Left => "left",
        Button::Middle => "middle",
        Button::Right => "right",
    };
    mouse(attached, "mouseMoved", at, "none", 0, json!({})).await?;
    for count in 1..=clicks.max(1) {
        mouse(attached, "mousePressed", at, name, count, json!({})).await?;
        mouse(attached, "mouseReleased", at, name, count, json!({})).await?;
    }
    Ok(())
}

/// Drags from `from` to `to`, pressed at the one, moved in steps, released at the other.
async fn drag(attached: &Attached, from: Target, to: Target) -> Result<(), InputError> {
    let (start, end) = (point_of(attached, from).await?, point_of(attached, to).await?);
    mouse(attached, "mouseMoved", start, "none", 0, json!({})).await?;
    mouse(attached, "mousePressed", start, "left", 1, json!({})).await?;
    for step in 1..=10 {
        let t = f64::from(step) / 10.0;
        let at = (start.0 + (end.0 - start.0) * t, start.1 + (end.1 - start.1) * t);
        mouse(attached, "mouseMoved", at, "left", 0, json!({ "buttons": 1 })).await?;
    }
    mouse(attached, "mouseReleased", end, "left", 1, json!({})).await
}

impl Page {
    /// Gives the page `input`, then waits till it settles: what came of it, in a line the model reads.
    ///
    /// # Errors
    /// `Refused`, saying why, where it could not be done as asked; the browser's refusal.
    pub async fn input(&self, input: &Input) -> Result<String, InputError> {
        let attached = self.attached().await?;
        let before = match self.browser.view(&self.id).await.and_then(|view| view.dialog) {
            Some(_) => None,
            None => self.probe().await.ok(),
        };
        let dialog_before = self.browser.view(&self.id).await.and_then(|view| view.dialog);
        let detail = self.perform(&attached, input).await?;
        let since = Instant::now();
        let after = self.settle(since, SETTLE_CAP).await;
        let came = match (&before, &after) {
            (Some(before), Some(after)) if before.document != after.document || before.url != after.url => {
                format!("navigated to {}", after.url)
            }
            (Some(before), Some(after)) if before.changes == after.changes => "no change".to_owned(),
            _ => "the page changed".to_owned(),
        };
        let dialog = self.browser.view(&self.id).await.and_then(|view| view.dialog);
        let raised = match dialog {
            Some(dialog) if Some(&dialog) != dialog_before.as_ref() => {
                format!("; a {} dialog holds the page: {}", dialog.kind, json!(dialog.message))
            }
            _ => String::new(),
        };
        Ok(format!("{detail}{came}{raised}"))
    }

    async fn perform(&self, attached: &Attached, input: &Input) -> Result<String, InputError> {
        let session = Some(attached.session.as_str());
        match input {
            Input::Click { target, button, clicks } => {
                click(attached, *target, *button, *clicks).await?;
                Ok(String::new())
            }
            Input::Hover { target } => {
                let at = point_of(attached, *target).await?;
                mouse(attached, "mouseMoved", at, "none", 0, json!({})).await?;
                Ok(String::new())
            }
            Input::Type { node, text, clear, submit } => {
                if let Some(node) = node {
                    attached
                        .cdp
                        .call(session, "DOM.focus", json!({ "backendNodeId": node }))
                        .await
                        .map_err(|_| InputError::Refused(format!("e{node} takes no focus")))?;
                    if *clear {
                        on_node(attached, *node, CLEAR, &[]).await?;
                        press(attached, "Backspace", &[]).await?;
                    }
                }
                send(attached, "Input.insertText", json!({ "text": text })).await?;
                if *submit {
                    press(attached, "Enter", &[]).await?;
                }
                Ok(String::new())
            }
            Input::Press { key, modifiers } => {
                press(attached, key, modifiers).await?;
                Ok(String::new())
            }
            Input::Select { node, values } => {
                let chosen = on_node(attached, *node, SELECT, &[json!(values)]).await?;
                Ok(format!("chose {chosen}; "))
            }
            Input::Scroll { target, delta_x, delta_y } => {
                let at = if let Some(target) = target {
                    point_of(attached, *target).await?
                } else {
                    let size = self.browser.view(&self.id).await.map(|view| view.size);
                    size.map_or((400.0, 300.0), |size| (f64::from(size.width) / 2.0, f64::from(size.height) / 2.0))
                };
                mouse(
                    attached,
                    "mouseWheel",
                    at,
                    "none",
                    0,
                    json!({ "deltaX": delta_x, "deltaY": delta_y }),
                )
                .await?;
                Ok(String::new())
            }
            Input::Drag { from, to } => {
                drag(attached, *from, *to).await?;
                Ok(String::new())
            }
            Input::Upload { node, files } => {
                attached
                    .cdp
                    .call(session, "DOM.setFileInputFiles", json!({ "backendNodeId": node, "files": files }))
                    .await
                    .map_err(|error| InputError::Refused(format!("e{node} took no files: {error}")))?;
                Ok(format!("gave {} files; ", files.len()))
            }
            Input::Dialog { accept, text } => {
                let mut answer = json!({ "accept": accept });
                if let Some(text) = text {
                    answer["promptText"] = json!(text);
                }
                attached
                    .cdp
                    .call(session, "Page.handleJavaScriptDialog", answer)
                    .await
                    .map_err(|error| InputError::Refused(format!("the dialog was not answered: {error}")))?;
                Ok(String::new())
            }
        }
    }
}
