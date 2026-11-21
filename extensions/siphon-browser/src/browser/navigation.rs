//! The main document's address and load state. Frame and subresource failures cannot replace it.

use std::time::Instant;

use chromiumoxide_cdp::cdp::CdpEvent;

use crate::protocol::PageLoad;

use super::Page;

#[derive(Debug)]
pub(super) struct DocumentRequest {
    id: String,
    url: String,
}

pub(super) enum DocumentChange {
    Loading,
    Navigated,
    Settled,
}

impl Page {
    pub(super) fn document_event(&mut self, session: &str, event: &CdpEvent) -> Option<DocumentChange> {
        let root = self.session.as_deref() == Some(session);
        let main = |frame: &str| root && self.target.as_deref() == Some(frame);
        match event {
            CdpEvent::NetworkRequestWillBeSent(sent) => {
                let kind = sent.r#type.as_ref().map(AsRef::as_ref);
                if !matches!(kind, Some("WebSocket" | "EventSource")) {
                    self.in_flight.insert(sent.request_id.as_ref().to_owned(), Instant::now());
                }
                self.network_at = Instant::now();
                if kind == Some("Document") && sent.frame_id.as_ref().is_some_and(|frame| main(frame.as_ref())) {
                    self.document_request = Some(DocumentRequest {
                        id: sent.request_id.as_ref().to_owned(),
                        url: sent.request.url.clone(),
                    });
                    return self.start_loading();
                }
            }
            CdpEvent::NetworkLoadingFinished(done) => {
                self.in_flight.remove(done.request_id.as_ref());
                self.network_at = Instant::now();
            }
            CdpEvent::NetworkLoadingFailed(failed) => {
                self.in_flight.remove(failed.request_id.as_ref());
                self.network_at = Instant::now();
                if root
                    && let Some(request) = self.document_request.take_if(|pending| pending.id == failed.request_id.as_ref())
                    && failed.canceled != Some(true)
                    && failed.error_text != "net::ERR_ABORTED"
                {
                    self.fail_load(request.url, failed.error_text.clone());
                    return Some(DocumentChange::Settled);
                }
            }
            CdpEvent::PageFrameStartedLoading(started) if main(started.frame_id.as_ref()) => return self.start_loading(),
            CdpEvent::PageFrameNavigated(navigated) if root && navigated.frame.parent_id.is_none() => {
                let frame = &navigated.frame;
                if let Some(address) = &frame.unreachable_url {
                    if !matches!(self.load, PageLoad::Failed { .. }) {
                        self.fail_load(address.clone(), "The browser could not reach this page.".into());
                    }
                } else if !frame.url.starts_with("chrome-error:") {
                    let mut address = frame.url.clone();
                    address.push_str(frame.url_fragment.as_deref().unwrap_or_default());
                    self.url = address;
                    if matches!(self.load, PageLoad::Failed { .. }) {
                        self.load = PageLoad::Loading;
                    }
                }
                return Some(DocumentChange::Navigated);
            }
            CdpEvent::PageNavigatedWithinDocument(navigated) if main(navigated.frame_id.as_ref()) => {
                self.url.clone_from(&navigated.url);
                return Some(DocumentChange::Navigated);
            }
            CdpEvent::PageFrameStoppedLoading(stopped) if main(stopped.frame_id.as_ref()) => {
                self.document_request = None;
                if self.load == PageLoad::Loading {
                    self.load = PageLoad::Ready;
                    return Some(DocumentChange::Settled);
                }
            }
            _ => {}
        }
        None
    }

    fn start_loading(&mut self) -> Option<DocumentChange> {
        if self.load == PageLoad::Loading {
            return None;
        }
        self.load = PageLoad::Loading;
        Some(DocumentChange::Loading)
    }

    fn fail_load(&mut self, address: String, message: String) {
        if address != self.url {
            self.title.clear();
        }
        self.url = address;
        self.load = PageLoad::Failed { message };
    }
}
