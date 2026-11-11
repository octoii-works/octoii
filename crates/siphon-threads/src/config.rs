//! How a thread runs — the model its turns ask, how hard it reasons, how its calls go ahead, the folder it works in,
//! its title, what its model requests may cost — as its directory's annotations hold it now, each value as JSON. A
//! change of it is logged where it took effect, each field it changed from what it was to what it is, so the log tells
//! what changed and who changed it.

use beak::Directory;
use bytes::Bytes;
use octoii_ai::vocabulary::ReasoningEffort;
use serde::{Deserialize, Serialize};
use siphon_runtime::Named;

/// How a thread's calls go ahead: in `default`, what goes beyond the workspace and the sandbox asks the person; in
/// `auto`, what would ask is judged by a second model, the person asked where it doubts; in `full-access`, everything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Default,
    Auto,
    FullAccess,
}

/// A field changed: what it was, and what it is; `None` for not set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Changed<T> {
    pub from: Option<T>,
    pub to: Option<T>,
}

/// The value of the annotation `name` on `directory`, as JSON; none where it has none.
fn annotation<T: serde::de::DeserializeOwned>(directory: &Directory, name: &str) -> serde_json::Result<Option<T>> {
    directory
        .annotation(name.as_bytes())
        .map(|value| serde_json::from_slice(value))
        .transpose()
}

fn json<T: Serialize>(value: &T) -> Bytes {
    serde_json::to_vec(value).expect("a setting is JSON").into()
}

macro_rules! settings {
    ($($(#[$doc:meta])* $field:ident: $ty:ty => $annotation:literal;)*) => {
        /// How a thread runs: each setting, or none where it has none.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[derive(ts_rs::TS)]
        pub struct Config {
            $($(#[$doc])* #[serde(default, skip_serializing_if = "Option::is_none")] pub $field: Option<$ty>,)*
        }

        /// A change of how a thread runs: each setting given, `Some(None)` to let it go; each left out kept.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[derive(ts_rs::TS)]
        pub struct ConfigChange {
            $(#[serde(default, skip_serializing_if = "Option::is_none", with = "::serde_with::rust::double_option")]
            #[ts(as = "_", optional = nullable)]
            pub $field: Option<Option<$ty>>,)*
        }

        /// What a change changed, as the log holds it: each field changed, from what it was to what it is.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[derive(ts_rs::TS)]
        pub struct ConfigChanged {
            $(#[serde(default, skip_serializing_if = "Option::is_none")] pub $field: Option<Changed<$ty>>,)*
        }

        impl Config {
            /// The settings `directory`'s annotations hold.
            ///
            /// # Errors
            /// An annotation that is not the JSON of its setting.
            pub fn of(directory: &Directory) -> serde_json::Result<Self> {
                Ok(Self { $($field: annotation(directory, $annotation)?,)* })
            }

            /// These settings with `change` made, and what it changed: a setting given what it had is no change.
            #[must_use]
            pub fn with(&self, change: &ConfigChange) -> (Self, ConfigChanged) {
                let mut after = self.clone();
                let mut changed = ConfigChanged::default();
                $(if let Some(to) = &change.$field && *to != self.$field {
                    changed.$field = Some(Changed { from: self.$field.clone(), to: to.clone() });
                    after.$field.clone_from(to);
                })*
                (after, changed)
            }

            /// These settings as what made them changed, every one from none.
            #[must_use]
            pub fn as_made(&self) -> ConfigChanged {
                Self::default().with(&ConfigChange { $($field: self.$field.clone().map(Some),)* }).1
            }
        }

        impl ConfigChanged {
            #[must_use]
            pub fn is_empty(&self) -> bool {
                true $(&& self.$field.is_none())*
            }

            /// The annotations that hold what was changed as it is now.
            pub(crate) fn annotations(&self) -> Vec<Named> {
                let mut out = Vec::new();
                $(if let Some(changed) = &self.$field {
                    out.push((Bytes::from_static($annotation.as_bytes()), changed.to.as_ref().map(json)));
                })*
                out
            }
        }
    };
}

settings! {
    /// The model its turns ask, by its reference.
    model: String => "siphon.model";
    /// How hard its model reasons; as the model does, where none.
    effort: ReasoningEffort => "siphon.effort";
    /// How its calls go ahead; `default` where none.
    mode: Mode => "siphon.mode";
    /// The folder on the machine its work is done in, by its absolute path.
    workspace: String => "siphon.workspace";
    title: String => "siphon.title";
    /// The most its model requests may cost in all, in billionths of a US dollar; no bound, where none.
    budget_nanodollars: u64 => "siphon.budget_nanodollars";
}
