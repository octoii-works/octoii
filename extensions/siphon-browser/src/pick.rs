//! Evidence of one main-document element, read without dispatching input. The document, viewport and selected target
//! are checked around a screenshot; unrelated DOM changes do not invalidate it. Only the kept image survives as a
//! handle: a locator describes the captured document, not a future page.

use bytes::Bytes;
use serde::Deserialize;
use serde_json::{Value, json};
use siphon::Caller;
use siphon::protocol::{Error, Scope};

use crate::browser::Attached;
use crate::extension::failed;
use crate::protocol::{CapturedViewport, ElementDescription, ElementFeedback, ElementRect, PagePoint, PickElement};
use crate::{Browser, BrowserError, Size};

const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
const RECT_TOLERANCE: f64 = 0.5;

const LAYOUT: &str =
    "({ url: location.href, width: innerWidth, height: innerHeight, scale: devicePixelRatio, scroll_x: scrollX, scroll_y: scrollY })";

/// Reads bounded words and one selector; form controls' values and the surrounding DOM are never returned.
const DESCRIBE: &str = r"function() {
  const e = this.nodeType === 1 ? this : this.parentElement;
  if (!e) return null;
  const doc = e.ownerDocument;
  const cut = (value, limit) => [...String(value || '').slice(0, limit * 2)].slice(0, limit).join('').replace(/\s+/g, ' ').trim();
  const words = () => {
    if (e.matches('input,textarea,select,option,script,style')) return '';
    const walker = doc.createTreeWalker(e, NodeFilter.SHOW_TEXT);
    let text = '', visited = 0;
    for (let n = walker.nextNode(); n && visited < 64 && text.length < 1024; n = walker.nextNode()) {
      visited++;
      const p = n.parentElement;
      if (!p || p.closest('input,textarea,select,option,script,style,[hidden]') || !p.checkVisibility({ checkVisibilityCSS: true })) continue;
      text += ' ' + (n.nodeValue || '').slice(0, 1024 - text.length);
    }
    return cut(text, 512);
  };
  const selector = () => {
    if (e.getRootNode() !== doc) return null;
    const unique = (s) => { const nodes = doc.querySelectorAll(s); return nodes.length === 1 && nodes[0] === e; };
    const path = [];
    for (let n = e, depth = 0; n && depth < 6; n = n.parentElement, depth++) {
      if (n.id && n.id.length <= 128) {
        const s = ['#' + CSS.escape(n.id), ...path].join(' > ');
        if (s.length <= 512 && unique(s)) return s;
      }
      let index = 1;
      for (let sibling = n.previousElementSibling, count = 0; sibling; sibling = sibling.previousElementSibling) {
        if (++count > 512) return null;
        if (sibling.localName === n.localName) index++;
      }
      path.unshift(CSS.escape(n.localName) + ':nth-of-type(' + index + ')');
      const s = path.join(' > ');
      if (s.length > 512) return null;
      if (unique(s)) return s;
    }
    return null;
  };
  const r = e.getBoundingClientRect();
  return {
    connected: e.isConnected,
    main_document: doc.defaultView === doc.defaultView.top,
    frame: e.matches('iframe,frame'),
    element: {
      tag: cut(e.localName, 64),
      role: cut(e.getAttribute('role'), 64) || null,
      label: cut(e.getAttribute('aria-label') || e.getAttribute('title') || e.getAttribute('alt'), 256) || null,
      text: words() || null,
      selector: selector(),
      rect: { x: r.x, y: r.y, width: r.width, height: r.height }
    }
  };
}";

const MEASURE: &str = r"function() {
  const e = this.nodeType === 1 ? this : this.parentElement;
  if (!e || !e.isConnected) return { connected: false, rect: { x: 0, y: 0, width: 0, height: 0 } };
  const r = e.getBoundingClientRect();
  return { connected: true, rect: { x: r.x, y: r.y, width: r.width, height: r.height } };
}";

#[derive(Debug, Deserialize)]
struct Layout {
    url: String,
    #[serde(flatten)]
    viewport: CapturedViewport,
}

#[derive(Debug)]
struct Document {
    frame: String,
    id: String,
    layout: Layout,
}

impl Document {
    async fn read(attached: &Attached) -> Result<Self, Error> {
        let tree = call(attached, "Page.getFrameTree", json!({})).await?;
        let frame = &tree["frameTree"]["frame"];
        let frame_id = string(&frame["id"])?;
        let document = string(&frame["loaderId"])?;
        let read = call(
            attached,
            "Runtime.evaluate",
            json!({ "expression": LAYOUT, "returnByValue": true, "userGesture": false }),
        )
        .await?;
        let layout: Layout = value(&read)?;
        if !valid_viewport(layout.viewport) {
            return Err(refused("page_changed", "the page's viewport could not be read; select it again"));
        }
        Ok(Self {
            frame: frame_id,
            id: document,
            layout,
        })
    }

    fn matches_request(&self, request: &PickElement, size: Size) -> Result<(), Error> {
        let viewport = self.layout.viewport;
        let actual = Size {
            width: viewport.width,
            height: viewport.height,
            scale: viewport.scale,
        };
        let expected = Size {
            width: request.view.width,
            height: request.view.height,
            scale: request.view.scale.unwrap_or(1.0),
        };
        if self.layout.url != request.expected_url || actual != expected || actual != size {
            return Err(refused("page_changed", "the page or its viewport changed; select it again"));
        }
        Ok(())
    }

    fn unchanged(&self, after: &Self) -> Result<(), Error> {
        if self.frame != after.frame
            || self.id != after.id
            || self.layout.url != after.layout.url
            || self.layout.viewport != after.layout.viewport
        {
            return Err(refused(
                "page_changed",
                "the page or its viewport changed during capture; select it again",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct Described {
    connected: bool,
    main_document: bool,
    frame: bool,
    element: ElementDescription,
}

#[derive(Debug, Deserialize)]
struct Measured {
    connected: bool,
    rect: ElementRect,
}

struct Evidence {
    document: Document,
    element: ElementDescription,
    image: Bytes,
}

/// Captures and keeps evidence in the page's owning thread, after both browser and thread access were checked.
pub(crate) async fn pick(browser: &Browser, caller: &Caller, request: PickElement) -> Result<ElementFeedback, Error> {
    caller.require_scope(Scope::ThreadsOperate)?;
    validate(&request)?;
    if caller.workspace_of(&request.thread).is_none() {
        return Err(refused("no_such_thread", "no such thread"));
    }
    let view = browser
        .view(&request.page)
        .await
        .ok_or_else(|| refused("no_such_page", "no such page"))?;
    if view.thread != request.thread {
        return Err(refused("page_not_in_thread", "the page does not belong to this thread"));
    }
    if view.dialog.is_some() {
        return Err(refused("failed", "a dialog holds the page; answer it before selecting an element"));
    }
    let point = PagePoint {
        x: request.x,
        y: request.y,
    };
    let page = browser.page(&request.page);
    let attached = page.attached().await.map_err(|error| failed(&error))?;
    let document = Document::read(&attached).await?;
    document.matches_request(&request, view.size)?;
    let node = node_at(&attached, &document.frame, point).await?;
    let resolved = call(&attached, "DOM.resolveNode", json!({ "backendNodeId": node })).await?;
    let object = string(&resolved["object"]["objectId"])?;
    let captured_at = caller.volume().now_ms();
    let captured = capture(&page, &attached, document, node, &object, point).await;
    // The resolved object is local to this capture, including every refusal after it was obtained.
    let _ = call(&attached, "Runtime.releaseObject", json!({ "objectId": object })).await;
    let captured = captured?;
    let (screenshot, _) = caller
        .volume()
        .keep_image(&request.thread, &captured.image, Some("browser-feedback.jpg"))
        .await
        .map_err(|error| match error {
            siphon::threads::files::Refused::NoSuchThread => refused("no_such_thread", "the thread is gone"),
            other => refused("failed", &other.to_string()),
        })?;
    Ok(ElementFeedback {
        thread: request.thread,
        page: request.page,
        url: captured.document.layout.url,
        document: captured.document.id,
        captured_at,
        viewport: captured.document.layout.viewport,
        point,
        element: captured.element,
        screenshot,
    })
}

async fn capture(
    page: &crate::page::Page,
    attached: &Attached,
    document: Document,
    node: i64,
    object: &str,
    point: PagePoint,
) -> Result<Evidence, Error> {
    let described: Option<Described> = on_object(attached, object, DESCRIBE).await?;
    let described = described.ok_or_else(|| refused("no_element", "there is no element at this point"))?;
    if !described.main_document || described.frame {
        return Err(refused(
            "unsupported_frame",
            "select an element in the main document; frame contents are not supported",
        ));
    }
    if !described.connected || !valid_rect(described.element.rect) {
        return Err(refused(
            "target_changed",
            "the selected element is gone or has no bounds; select it again",
        ));
    }
    let image = tokio::time::timeout(READ_TIMEOUT, page.screenshot())
        .await
        .map_err(|_| refused("failed", "the page gave no screenshot in time"))?
        .map_err(|error| failed(&error))?;
    let after = Document::read(attached).await?;
    document.unchanged(&after)?;
    let measured: Measured = on_object(attached, object, MEASURE)
        .await
        .map_err(|_| refused("target_changed", "the selected element is gone; select it again"))?;
    check_target(described.element.rect, &measured)?;
    let hit = node_at(attached, &document.frame, point).await?;
    if hit != node {
        return Err(refused(
            "target_changed",
            "the element at this point changed during capture; select it again",
        ));
    }
    Ok(Evidence {
        document,
        element: described.element,
        image,
    })
}

fn validate(request: &PickElement) -> Result<(), Error> {
    let scale = request.view.scale.unwrap_or(1.0);
    if request.view.width == 0
        || request.view.height == 0
        || !scale.is_finite()
        || scale <= 0.0
        || !request.x.is_finite()
        || !request.y.is_finite()
        || request.x < 0.0
        || request.y < 0.0
        || request.x >= f64::from(request.view.width)
        || request.y >= f64::from(request.view.height)
    {
        return Err(refused(
            "bad_params",
            "select a finite point inside a nonempty viewport with a positive finite scale",
        ));
    }
    Ok(())
}

fn valid_viewport(viewport: CapturedViewport) -> bool {
    viewport.width > 0
        && viewport.height > 0
        && viewport.scale.is_finite()
        && viewport.scale > 0.0
        && viewport.scroll_x.is_finite()
        && viewport.scroll_y.is_finite()
}

fn valid_rect(rect: ElementRect) -> bool {
    [rect.x, rect.y, rect.width, rect.height].iter().all(|number| number.is_finite()) && rect.width > 0.0 && rect.height > 0.0
}

fn check_target(before: ElementRect, after: &Measured) -> Result<(), Error> {
    let same_rect = [
        (before.x, after.rect.x),
        (before.y, after.rect.y),
        (before.width, after.rect.width),
        (before.height, after.rect.height),
    ]
    .iter()
    .all(|(a, b)| (a - b).abs() <= RECT_TOLERANCE);
    if !after.connected || !valid_rect(after.rect) || !same_rect {
        return Err(refused(
            "target_changed",
            "the selected element moved or disappeared during capture; select it again",
        ));
    }
    Ok(())
}

async fn node_at(attached: &Attached, frame: &str, point: PagePoint) -> Result<i64, Error> {
    // The request was checked to lie inside a u32-sized CSS viewport; CDP hit testing takes integer pixels.
    #[expect(clippy::cast_possible_truncation, reason = "finite CSS coordinates inside a u32 viewport fit i64")]
    let (x, y) = (point.x.floor() as i64, point.y.floor() as i64);
    let hit = call(attached, "DOM.getNodeForLocation", json!({ "x": x, "y": y }))
        .await
        .map_err(|_| refused("no_element", "there is no element at this point"))?;
    if hit["frameId"].as_str() != Some(frame) {
        return Err(refused(
            "unsupported_frame",
            "select an element in the main document; frame contents are not supported",
        ));
    }
    hit["backendNodeId"]
        .as_i64()
        .ok_or_else(|| refused("no_element", "there is no element at this point"))
}

async fn on_object<T: serde::de::DeserializeOwned>(attached: &Attached, object: &str, function: &str) -> Result<T, Error> {
    let read = call(
        attached,
        "Runtime.callFunctionOn",
        json!({ "objectId": object, "functionDeclaration": function, "returnByValue": true, "userGesture": false }),
    )
    .await?;
    value(&read)
}

fn value<T: serde::de::DeserializeOwned>(read: &Value) -> Result<T, Error> {
    if let Some(thrown) = read.get("exceptionDetails") {
        let words = thrown["exception"]["description"]
            .as_str()
            .unwrap_or("the element could not be read");
        return Err(refused("failed", words));
    }
    serde_json::from_value(read["result"]["value"].clone()).map_err(|_| refused("failed", "the page returned no readable element evidence"))
}

async fn call(attached: &Attached, method: &str, params: Value) -> Result<Value, Error> {
    attached
        .cdp
        .call_within(Some(&attached.session), method, params, READ_TIMEOUT)
        .await
        .map_err(|error| failed(&BrowserError::from(error)))
}

fn string(value: &Value) -> Result<String, Error> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| refused("failed", "the browser returned no document or element identity"))
}

fn refused(code: &str, message: &str) -> Error {
    Error {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::View;

    fn request() -> PickElement {
        PickElement {
            thread: "t1".into(),
            page: "p1".into(),
            x: 12.0,
            y: 20.0,
            expected_url: "https://example.test/".into(),
            view: View {
                width: 400,
                height: 300,
                scale: Some(1.0),
            },
        }
    }

    #[test]
    fn capture_coordinates_and_viewport_require_finite_valid_numbers() {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 400.0] {
            assert!(validate(&PickElement { x: invalid, ..request() }).is_err());
        }
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
            assert!(
                validate(&PickElement {
                    view: View {
                        scale: Some(invalid),
                        ..request().view
                    },
                    ..request()
                })
                .is_err()
            );
        }
        assert!(validate(&request()).is_ok());
    }

    fn document(id: &str) -> Document {
        Document {
            frame: "main".into(),
            id: id.into(),
            layout: Layout {
                url: "https://example.test/".into(),
                viewport: CapturedViewport {
                    width: 400,
                    height: 300,
                    scale: 1.0,
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                },
            },
        }
    }

    #[test]
    fn a_same_url_reload_and_a_changed_scroll_position_are_different_evidence() {
        let before = document("loader-a");
        assert!(before.unchanged(&document("loader-a")).is_ok());
        assert_eq!(
            before.unchanged(&document("loader-b")).expect_err("another document").code,
            "page_changed"
        );
        let mut scrolled = document("loader-a");
        scrolled.layout.viewport.scroll_y = 10.0;
        assert_eq!(before.unchanged(&scrolled).expect_err("another viewport").code, "page_changed");
    }

    #[test]
    fn a_detached_or_moved_target_is_rejected_even_when_the_document_did_not_change() {
        let rect = ElementRect {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 30.0,
        };
        assert!(check_target(rect, &Measured { connected: true, rect }).is_ok());
        assert!(check_target(rect, &Measured { connected: false, rect }).is_err());
        assert!(
            check_target(
                rect,
                &Measured {
                    connected: true,
                    rect: ElementRect { x: 50.0, ..rect }
                }
            )
            .is_err()
        );
    }
}
