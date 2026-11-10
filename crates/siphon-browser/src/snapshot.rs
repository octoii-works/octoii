//! A page's accessibility tree as the model reads it, as a screen reader reads a page: a line for each thing a person
//! perceives on it, nested as they are — its role, its name, its states, its value — and the element that is each, by
//! a ref the actions take, the browser's id of its node, which lasts as long as the node; a ref new since the snapshot
//! before marked, so the model sees what an action brought. What no one perceives is passed over, its children read in
//! its place. Each frame of the page is read in turn; one of another process is named and passed over.

use std::collections::HashSet;
use std::fmt::Write as _;

use serde_json::{Value, json};

use crate::browser::BrowserError;
use crate::page::Page;

/// The roles of what is only a container, read through to its children where it has no name of its own.
const CONTAINERS: [&str; 7] = ["generic", "none", "presentation", "LabelText", "Section", "group", "paragraph"];
/// The roles of what a person perceives nothing of: pieces of text the browser lays out, and markers of lists.
const UNPERCEIVED: [&str; 3] = ["InlineTextBox", "LineBreak", "ListMarker"];
/// The states shown of a thing, by the property the browser names each by.
const STATES: [&str; 10] = [
    "level", "checked", "pressed", "selected", "expanded", "disabled", "required", "readonly", "invalid", "focused",
];

/// A page read: its lines, and the refs they name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub text: String,
    pub refs: Vec<i64>,
}

/// A ref, as the model writes it.
#[must_use]
pub fn ref_of(node: i64) -> String {
    format!("e{node}")
}

/// The node a ref names; none where it names none.
#[must_use]
pub fn node_of(reference: &str) -> Option<i64> {
    reference.strip_prefix('e')?.parse().ok()
}

fn text(value: &Value) -> &str {
    value["value"].as_str().unwrap_or_default()
}

/// The nodes of one frame's tree, by the browser's node id.
struct Tree<'a> {
    nodes: std::collections::HashMap<&'a str, &'a Value>,
}

impl<'a> Tree<'a> {
    fn of(nodes: &'a [Value]) -> Self {
        Self {
            nodes: nodes.iter().filter_map(|node| Some((node["nodeId"].as_str()?, node))).collect(),
        }
    }

    fn children(&self, node: &Value) -> Vec<&'a Value> {
        node["childIds"]
            .as_array()
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id.as_str()?).copied()).collect())
            .unwrap_or_default()
    }
}

/// What is known of the refs of the snapshot before, and what this one gathers.
struct Reading<'k> {
    known: &'k HashSet<i64>,
    out: Snapshot,
}

impl Reading<'_> {
    /// Reads `node` and what is under it, at `depth`, `parent_name` the name of what holds it.
    fn read(&mut self, tree: &Tree<'_>, node: &Value, depth: usize, parent_name: &str) {
        self.read_under(tree, node, depth, parent_name, "");
    }

    /// Reads `node` as `read` does, `parent_value` the value of what holds it, which its text repeats.
    fn read_under(&mut self, tree: &Tree<'_>, node: &Value, depth: usize, parent_name: &str, parent_value: &str) {
        let role = text(&node["role"]);
        let name = text(&node["name"]).trim();
        let children = tree.children(node);
        let ignored = node["ignored"].as_bool().unwrap_or_default();
        let through = ignored || UNPERCEIVED.contains(&role) || (CONTAINERS.contains(&role) && name.is_empty()) || role == "RootWebArea";
        let repeated = role == "StaticText" && (name.is_empty() || name == parent_name || name == parent_value);
        if through || repeated {
            for child in children {
                self.read_under(tree, child, depth, if name.is_empty() { parent_name } else { name }, parent_value);
            }
            return;
        }
        let mut line = format!("{}- {role}", "  ".repeat(depth));
        if !name.is_empty() {
            let _ = write!(line, " {}", json!(name));
        }
        let states: Vec<String> = node["properties"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|property| STATES.contains(&property["name"].as_str().unwrap_or_default()))
            .filter_map(|property| {
                let state = property["name"].as_str()?;
                match &property["value"]["value"] {
                    Value::Bool(true) => Some(state.to_owned()),
                    Value::Bool(false) | Value::Null => None,
                    Value::String(value) if value == "false" => None,
                    other => Some(format!(
                        "{state}={}",
                        other.as_str().map_or_else(|| other.to_string(), str::to_owned)
                    )),
                }
            })
            .collect();
        if !states.is_empty() {
            let _ = write!(line, " [{}]", states.join(", "));
        }
        // Text is read, not acted on: an element is.
        if let Some(node_id) = node["backendDOMNodeId"].as_i64().filter(|_| role != "StaticText") {
            let mark = if self.known.is_empty() || self.known.contains(&node_id) {
                ""
            } else {
                "*"
            };
            let _ = write!(line, " [{mark}ref={}]", ref_of(node_id));
            self.out.refs.push(node_id);
        }
        let value = text(&node["value"]);
        if !value.is_empty() {
            let _ = write!(line, ": {}", json!(value));
        }
        self.out.text.push_str(&line);
        self.out.text.push('\n');
        for child in children {
            self.read_under(tree, child, depth + 1, name, value);
        }
    }
}

/// The frames of the frame tree `tree`, the main one first: each frame's id and address.
fn frames(tree: &Value, out: &mut Vec<(String, String)>) {
    let frame = &tree["frame"];
    out.push((
        frame["id"].as_str().unwrap_or_default().to_owned(),
        frame["url"].as_str().unwrap_or_default().to_owned(),
    ));
    for child in tree["childFrames"].as_array().into_iter().flatten() {
        frames(child, out);
    }
}

impl Page {
    /// Reads the page as a screen reader does, each frame in turn; refs not among `known` — the snapshot before's —
    /// marked new.
    ///
    /// # Errors
    /// The browser's refusal.
    pub async fn snapshot(&self, known: &HashSet<i64>) -> Result<Snapshot, BrowserError> {
        let attached = self.attached().await?;
        let session = Some(attached.session.as_str());
        let tree = attached.cdp.call(session, "Page.getFrameTree", json!({})).await?;
        let mut all = Vec::new();
        frames(&tree["frameTree"], &mut all);
        let mut reading = Reading {
            known,
            out: Snapshot::default(),
        };
        for (at, (frame, url)) in all.iter().enumerate() {
            if at > 0 {
                let _ = writeln!(reading.out.text, "- frame {}", json!(url));
            }
            match attached
                .cdp
                .call(session, "Accessibility.getFullAXTree", json!({ "frameId": frame }))
                .await
            {
                Ok(read) => {
                    let nodes = read["nodes"].as_array().cloned().unwrap_or_default();
                    let tree = Tree::of(&nodes);
                    let roots = nodes.iter().filter(|node| node.get("parentId").is_none());
                    for root in roots {
                        reading.read(&tree, root, usize::from(at > 0), "");
                    }
                }
                Err(_) => {
                    let _ = writeln!(reading.out.text, "  (a frame of another site's process, not read)");
                }
            }
        }
        Ok(reading.out)
    }
}
