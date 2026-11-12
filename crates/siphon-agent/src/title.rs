//! A thread's title, as its model gives it: once its first turn begins with none, a process beside the thread's main
//! process asks the model, by a model request on the window up to the thread's first input — so what the window begins
//! with is shared with the turn's request, and its cache holds — for a few words of what the person asked, and delivers
//! the thread that title where it still has none, a change of how it runs logged as every change is; a title the
//! person gave first is kept. The title given is the process's result, for the record; a request that failed leaves
//! the thread untitled.

use serde::{Deserialize, Serialize};
use siphon_runtime::{Answer, Ending, Program, Run, Wait};
use siphon_threads::inbox::{self, Inbound};
use siphon_threads::{Config, ConfigChange, Entry, Window, thread};

use crate::model::ModelRequest;
use crate::turn::charge;

/// What a title request asks of the model, after the window.
pub const INSTRUCTION: &str = "Do not answer the person or do what they ask. Write a title for this thread instead: the topic of what they ask, in at most six words, in the language they wrote in, names, paths and numbers kept as written, with no quotes and no full stop. Write the title alone.";

/// The most bytes a title is kept to.
const TITLE_BYTES_MAX: usize = 80;

/// Who the title is changed by, as the log records it.
pub const ACTOR: &str = "siphon";

/// What a title process is started with: its thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleInput {
    pub thread: String,
}

/// The program of a thread's title process, keyed `KEY` under its main process.
#[derive(Debug, Clone, Copy, Default)]
pub struct TitleProgram;

impl TitleProgram {
    pub const KEY: &'static str = "title";
}

impl Program for TitleProgram {
    const KIND: &'static str = "siphon.title";
    type Input = TitleInput;
    type Checkpoint = String;
    type Output = Option<String>;

    fn run(&self, run: &mut Run<'_>, input: TitleInput, asked: Option<String>) -> anyhow::Result<Ending<String, Option<String>>> {
        let id = &input.thread;
        let config = config_of(run, id)?;
        let Some(key) = asked else {
            let Some(model) = config.model else { return Ok(Ending::Done(None)) };
            let window = Window::read(run.snapshot(), id)?;
            let first = window.conversation.iter().find(|entry| matches!(entry, Entry::Input { .. }));
            let Some(first) = first else { return Ok(Ending::Done(None)) };
            let key = run.new_id()?;
            let request = ModelRequest {
                thread: id.clone(),
                until: first.id().to_owned(),
                model,
                instruction: Some(INSTRUCTION.to_owned()),
            };
            run.request(&key, &request)?;
            return Ok(Ending::Wait(key.clone(), Wait::new().mail(key)));
        };
        let Some(answer) = run.answer::<ModelRequest>(&key)? else {
            return Ok(Ending::Wait(key.clone(), Wait::new().mail(key)));
        };
        run.take_mail(&key);
        let Answer::Given(response) = answer else {
            return Ok(Ending::Done(None));
        };
        charge(run, &response);
        let title = title_of(&response.words());
        if title.is_empty() || config.title.is_some() {
            return Ok(Ending::Done(None));
        }
        let inbound = Inbound::Config {
            actor: ACTOR.to_owned(),
            change: ConfigChange {
                title: Some(Some(title.clone())),
                ..ConfigChange::default()
            },
        };
        let delivery = run.new_id()?;
        inbox::put(&mut run.changes, id, &delivery, &inbound)?;
        Ok(Ending::Done(Some(title)))
    }
}

fn config_of(run: &Run<'_>, id: &str) -> anyhow::Result<Config> {
    let directory = run
        .snapshot()
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .unwrap_or_default();
    Ok(Config::of(&directory)?)
}

/// The title `words` give: their first line of words — a line of markup alone let go of, as a tag a model echoes
/// before its answer — the quotes and the full stop around it let go of, cut short at a character's end past
/// `TITLE_BYTES_MAX`; empty where no line holds words.
#[must_use]
pub fn title_of(words: &str) -> String {
    let markup = |line: &str| line.starts_with('<') && line.ends_with('>');
    let Some(line) = words.lines().map(str::trim).find(|line| !line.is_empty() && !markup(line)) else {
        return String::new();
    };
    let quote = |char: char| matches!(char, '"' | '\'' | '“' | '”' | '「' | '」' | '*' | '#');
    let line = line.trim_matches(quote).trim_end_matches(['.', '。']).trim_matches(quote).trim();
    let mut end = line.len().min(TITLE_BYTES_MAX);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::title_of;

    #[test]
    fn a_title_is_the_first_line_of_words_without_its_quotes_and_full_stop() {
        assert_eq!(title_of("<title>\n\"Fix the login bug.\"\nmore"), "Fix the login bug");
        assert_eq!(title_of("  \n「修复登录问题」。"), "修复登录问题");
        assert_eq!(title_of("<x>"), "");
        let long = "长".repeat(40);
        assert_eq!(title_of(&long).len(), 78, "cut at a character's end");
    }
}
