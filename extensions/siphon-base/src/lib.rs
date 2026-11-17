//! Octoii's base instructions, supplied by the application rather than embedded in Siphon.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use siphon::agent::{Shared, Teller, Telling};
use siphon::tools::Place;
use siphon::volume::Volume;
use siphon::{Extension, Registrar};

#[derive(Debug, Clone, Copy, Default)]
pub struct BaseExtension;

impl Extension for BaseExtension {
    fn name(&self) -> &'static str {
        "base"
    }

    fn title(&self) -> &'static str {
        "Base"
    }

    fn description(&self) -> &'static str {
        "Octoii's required base instructions."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.teller(Shared::Machine, Arc::new(Base));
    }
}

#[derive(Debug)]
struct Base;

impl Teller for Base {
    fn tell<'a>(
        &'a self,
        _: &'a Volume,
        _: &'a str,
        _: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        Box::pin(std::future::ready(Ok(Telling {
            sections: vec![("base".into(), include_str!("../prompts/base.md").trim().into())],
            tools: Vec::new(),
        })))
    }
}
