//! Octoii's base instructions, supplied by the application rather than embedded in Octoii.

mod environment;
mod instructions;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use octoii::agent::{Shared, Teller, Telling};
use octoii::tools::Place;
use octoii::volume::Volume;
use octoii::{Extension, Registrar};

#[derive(Debug, Clone, Default)]
pub struct BaseExtension {
    pub config: Option<PathBuf>,
}

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
        registrar.prompts(octoii::agent::Prompts {
            title: include_str!("../prompts/title.md").trim(),
            recap: include_str!("../prompts/recap.md").trim(),
            review: include_str!("../prompts/review.md").trim(),
        });
        registrar.teller(Shared::Machine, Arc::new(Base));
        registrar.teller(Shared::Project, Arc::new(instructions::Instructions { config: self.config }));
        let environment = environment::Environment {
            host: registrar.machine().clone(),
            writable: registrar.writable_folders(),
        };
        registrar.teller(Shared::Thread, Arc::new(environment));
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
