//! What the model is told, as the log keeps it, and the request a window is rendered as.

use octoii_ai::catalogue::{Features, InputLimits, Model};
use octoii_ai::vocabulary::{
    ConfigurationUpdate, FileRef, ImageDetail, InputContent, Item, Message, ReasoningConfig, ReasoningEffort, Response, ResponseStatus,
    Tool,
};
use siphon_agent::render::{STOPPED_NOTE, request};
use siphon_agent::told::{held, news};
use siphon_threads::{Changed, ConfigChanged, Entry, Outcome, Section, Window};

fn system(id: &str, sections: &[(&str, Option<&str>)], tools: Vec<Tool>) -> Entry {
    Entry::System {
        id: id.into(),
        sections: sections
            .iter()
            .map(|(name, text)| Section {
                name: (*name).into(),
                text: text.map(str::to_owned),
            })
            .collect(),
        tools,
    }
}

fn pairs(sections: &[(&str, &str)]) -> Vec<(String, String)> {
    sections.iter().map(|(name, text)| ((*name).into(), (*text).into())).collect()
}

fn effort(id: &str, to: ReasoningEffort) -> Entry {
    Entry::Config {
        id: id.into(),
        actor: "person".into(),
        changed: ConfigChanged {
            effort: Some(Changed { from: None, to: Some(to) }),
            ..ConfigChanged::default()
        },
    }
}

fn input(id: &str, content: Vec<InputContent>) -> Entry {
    Entry::Input {
        id: id.into(),
        actor: "person".into(),
        delivery: id.into(),
        content,
        reply_to: None,
    }
}

fn reply(id: &str) -> Entry {
    Entry::Reply {
        id: id.into(),
        request: "q".into(),
        response: Response {
            id: "r".into(),
            model: "test/m".into(),
            status: ResponseStatus::Completed,
            output: Vec::new(),
            usage: None,
            failure: None,
            incomplete: None,
            context_tokens: None,
        },
        cancelled: false,
    }
}

fn model(reads_images: bool, searches_web: bool) -> Model {
    Model {
        id: "m".into(),
        provider: "test".into(),
        name: "m".into(),
        context_length: None,
        max_output_tokens: None,
        reads_images,
        reads_files: true,
        reasoning: None,
        parameters: None,
        pricing: None,
        release_date: None,
        searches_web,
        limits: InputLimits::default(),
        features: Features::default(),
    }
}

#[test]
fn the_model_is_told_each_section_new_or_changed_and_each_no_longer_there() {
    let window = Window::of(vec![
        system(
            "1",
            &[("base", Some("b")), ("instructions", Some("v1")), ("old", Some("o"))],
            Vec::new(),
        ),
        input("2", vec![InputContent::text("hi")]),
        system("3", &[("instructions", Some("v2")), ("old", None)], Vec::new()),
    ]);
    assert_eq!(held(&window), pairs(&[("base", "b"), ("instructions", "v2")]));
    let now = pairs(&[("base", "b"), ("instructions", "v1"), ("environment", "e")]);
    let told = news(&held(&window), &now);
    let names: Vec<(&str, Option<&str>)> = told
        .iter()
        .map(|section| (section.name.as_str(), section.text.as_deref()))
        .collect();
    assert_eq!(names, [("instructions", Some("v1")), ("environment", Some("e"))]);
    let gone = news(&pairs(&[("base", "b"), ("skills", "s")]), &pairs(&[("base", "b")]));
    assert_eq!(
        gone,
        [Section {
            name: "skills".into(),
            text: None
        }]
    );
}

#[test]
fn a_window_is_asked_at_the_effort_set_before_its_first_reply_and_told_a_change_after_where_it_stands() {
    let window = Window::of(vec![
        system("1", &[("base", Some("b"))], vec![Tool::WebSearch]),
        effort("2", ReasoningEffort::Low),
        input("3", vec![InputContent::text("hi")]),
        reply("4"),
        Entry::TurnEnd {
            id: "5".into(),
            turn: Some("t".into()),
            outcome: Outcome::Stopped,
            reason: None,
        },
        effort("6", ReasoningEffort::High),
        input("7", vec![InputContent::text("again")]),
    ]);
    let asked = request("t1", "test/m", Some(&model(true, false)), &window, None);
    assert_eq!(
        asked.reasoning,
        Some(ReasoningConfig {
            effort: ReasoningEffort::Low
        })
    );
    assert_eq!(asked.tools, [], "a search the model does not run is left out");
    assert_eq!(asked.prompt_cache_key.as_deref(), Some("t1"));
    assert_eq!(asked.input.len(), 5);
    assert!(matches!(&asked.input[2], Item::Message(Message::Developer { content }) if content == &[InputContent::text(STOPPED_NOTE)]));
    assert_eq!(
        asked.input[3],
        Item::ConfigurationUpdate(ConfigurationUpdate {
            effort: ReasoningEffort::High
        })
    );
}

#[test]
fn a_model_that_reads_no_images_is_told_of_each_in_words() {
    let image = InputContent::InputImage {
        file: FileRef {
            reference: "sha256:x".into(),
            media_type: "image/png".into(),
            filename: None,
        },
        detail: ImageDetail::Auto,
    };
    let window = Window::of(vec![input("1", vec![InputContent::text("see"), image.clone()])]);
    let blind = request("t1", "test/m", Some(&model(false, true)), &window, None);
    assert_eq!(
        blind.input[0],
        Item::Message(Message::User {
            content: vec![
                InputContent::text("see"),
                InputContent::text("[an image, not shown: the model test/m reads no images]")
            ]
        })
    );
    let seeing = request("t1", "test/m", Some(&model(true, true)), &window, None);
    assert_eq!(
        seeing.input[0],
        Item::Message(Message::User {
            content: vec![InputContent::text("see"), image]
        })
    );
}
