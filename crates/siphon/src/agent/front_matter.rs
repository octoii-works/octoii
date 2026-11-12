//! The front matter a Markdown file of the model's starts with — a skill's, a memory's: `key: value` lines between two
//! `---` lines, a value's quotes taken off.

/// The keys and values of the front matter `text` starts with; none where it starts with none.
#[must_use]
pub fn of(text: &str) -> Vec<(&str, &str)> {
    let Some(matter) = text.strip_prefix("---\n").and_then(|rest| rest.split("\n---").next()) else {
        return Vec::new();
    };
    matter
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim(), value.trim().trim_matches(|quote| quote == '"' || quote == '\'')))
        .collect()
}
