//! Where a window is cut, and when it is full.

use octoii_ai::vocabulary::{FunctionCallOutput, InputContent, Response, ResponseStatus, Usage};
use siphon_agent::compaction::{KEPT_TOKENS, RESERVE_TOKENS, cut, is_full};
use siphon_threads::{CallStatus, Entry, Window};

fn input(id: &str) -> Entry {
    Entry::Input {
        id: id.into(),
        actor: "person".into(),
        delivery: id.into(),
        content: vec![InputContent::text("x")],
    }
}

fn reply(id: &str, input_tokens: u64) -> Entry {
    Entry::Reply {
        id: id.into(),
        request: "q".into(),
        response: Response {
            id: "r".into(),
            model: "test/m".into(),
            status: ResponseStatus::Completed,
            output: Vec::new(),
            usage: Some(Usage {
                input_tokens,
                cached_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                web_searches: 0,
                cost_nanodollars: None,
            }),
            failure: None,
            incomplete: None,
            context_tokens: None,
        },
        cancelled: false,
    }
}

fn result(id: &str, bytes: usize) -> Entry {
    Entry::Result {
        id: id.into(),
        call: "k".into(),
        status: CallStatus::Ok,
        output: FunctionCallOutput {
            call_id: "c".into(),
            output: vec![InputContent::text("y".repeat(bytes))],
            is_error: false,
        },
    }
}

fn kept(window: &Window) -> Option<(Option<String>, String, String)> {
    cut(window).map(|cut| (cut.kept, cut.from, cut.until))
}

#[test]
fn a_window_is_cut_before_the_earliest_step_its_kept_tokens_reach_else_before_the_latest() {
    let long = usize::try_from(KEPT_TOKENS).expect("small") * 4;
    let turns = Window::of(vec![input("1"), result("2", long), input("3"), input("4")]);
    assert_eq!(kept(&turns), Some((None, "3".into(), "2".into())));
    let one = Window::of(vec![input("1"), reply("2", 1), result("3", long), reply("4", 1), result("5", 1)]);
    assert_eq!(
        kept(&one),
        Some((Some("1".into()), "4".into(), "3".into())),
        "a long turn is cut within it, keeping the input that began it"
    );
    assert_eq!(kept(&Window::of(vec![input("1")])), None, "nothing to recap");
    assert_eq!(kept(&Window::of(vec![input("1"), reply("2", 1)])), None);
}

#[test]
fn a_window_is_full_past_its_model_s_context_less_what_it_keeps_free_by_what_its_last_reply_says() {
    let window = Window::of(vec![input("1"), reply("2", 100_000), input("3")]);
    assert!(is_full(&window, Some(100_000 + RESERVE_TOKENS)));
    assert!(!is_full(&window, Some(200_000)));
    assert!(!is_full(&window, None), "never where the model's context is not known");
}
