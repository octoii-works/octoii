//! Git-specific instructions, including operations performed through shell.

use siphon::agent::{Teller, Telling};
use siphon::tools::Place;
use siphon::volume::Volume;
use std::future::Future;
use std::pin::Pin;

pub(crate) struct GitContext;

impl Teller for GitContext {
    fn tell<'a>(
        &'a self,
        _: &'a Volume,
        _: &'a str,
        _: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        Box::pin(std::future::ready(Ok(Telling {
            sections: vec![("git".into(), include_str!("git.md").trim().into())],
            tools: Vec::new(),
        })))
    }
}
