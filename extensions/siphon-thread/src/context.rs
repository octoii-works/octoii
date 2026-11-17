//! Product instructions owned by this extension.
use siphon::agent::{Teller, Telling};
use siphon::tools::Place;
use siphon::volume::Volume;
use std::future::Future;
use std::pin::Pin;

pub(crate) struct Context;

impl Teller for Context {
    fn tell<'a>(
        &'a self,
        _: &'a Volume,
        _: &'a str,
        _: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        Box::pin(std::future::ready(Ok(Telling {
            sections: vec![("thread".into(), include_str!("context.md").trim().into())],
            tools: Vec::new(),
        })))
    }
}
