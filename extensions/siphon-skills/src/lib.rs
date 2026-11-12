//! Skills as an extension of Siphon's: what a thread's model may follow for a kind of task — each a folder with a
//! `SKILL.md`, found in the person's own folder of skills and in each project folder's `.agents/skills`, from its root
//! down to the workspace — told by name, a line of what it is for, and where its file is.

pub mod skills;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use siphon::agent::{Shared, Teller, Telling};
use siphon::tools::Place;
use siphon::{Extension, Registrar};

/// Skills, as a host adds them: the person's own found in the folder of their configuration `config`.
#[derive(Debug, Clone, Default)]
pub struct SkillsExtension {
    pub config: Option<PathBuf>,
}

impl Extension for SkillsExtension {
    fn name(&self) -> &'static str {
        "skills"
    }

    fn title(&self) -> &'static str {
        "Skills"
    }

    fn description(&self) -> &'static str {
        "Instructions for kinds of tasks, in the person's folder of skills and each project's, which the model follows."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.teller(Shared::Project, Arc::new(Told { config: self.config }));
    }
}

/// The skills a thread at a place may use, as its model is told them.
#[derive(Debug)]
struct Told {
    config: Option<PathBuf>,
}

impl Teller for Told {
    fn tell<'a>(
        &'a self,
        _: &'a siphon::Volume,
        _: &'a str,
        place: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        let found = place
            .map(|place| skills::find(&places(self.config.as_ref(), place)))
            .unwrap_or_default();
        let telling = Telling {
            sections: if found.is_empty() {
                Vec::new()
            } else {
                vec![("skills".to_owned(), skills::told(&found))]
            },
            tools: Vec::new(),
        };
        Box::pin(std::future::ready(Ok(telling)))
    }
}

/// The folders of skills a thread working at `place` finds, the farthest first: the person's own, in the folder of
/// their configuration `config`, then each project folder's from its root down to the workspace.
fn places(config: Option<&PathBuf>, place: &Place) -> Vec<PathBuf> {
    let mut places: Vec<PathBuf> = config.map(|config| config.join("skills")).into_iter().collect();
    places.extend(
        place
            .folders_down()
            .into_iter()
            .filter(|folder| folder.starts_with(&place.project))
            .map(|folder| folder.join(".agents/skills")),
    );
    places
}
