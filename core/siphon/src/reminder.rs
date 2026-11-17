//! What Octoii says to a thread's model within what it reads, apart from what is read: a system reminder, the tag
//! `<system-reminder>` around Octoii's words — a note after a tool's result, how another thread's turn ended. What
//! comes from outside — a file, a command's output, a page, another model's answer — holds the tag escaped, so it never
//! speaks as Octoii.

use std::borrow::Cow;

/// `raw`, from outside, its system reminder tags escaped: `<` of an opening or closing tag as `&lt;`.
#[must_use]
pub fn escaped(raw: &str) -> Cow<'_, str> {
    if raw.contains("<system-reminder") || raw.contains("</system-reminder") {
        Cow::Owned(
            raw.replace("</system-reminder", "&lt;/system-reminder")
                .replace("<system-reminder", "&lt;system-reminder"),
        )
    } else {
        Cow::Borrowed(raw)
    }
}
