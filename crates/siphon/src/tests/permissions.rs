//! A call's effect weighed by its thread's mode and its project's grants: command lines read as the shell reads them,
//! each command granted by the words that name what it does, and what reads alone told from what acts.

use std::path::PathBuf;

use crate::threads::Mode;
use crate::tools::Effect;
use crate::tools::effect::Line;
use crate::tools::permissions::Writes;
use crate::tools::permissions::{Bounds, Grants, Verdict, patterns_of, weigh};

fn runs(line: &str) -> Effect {
    Effect::Runs {
        line: line.into(),
        outside_sandbox: false,
        justification: None,
    }
}

#[test]
fn a_line_runs_each_of_its_commands_each_granted_by_the_words_that_name_what_it_does() {
    let read = Line::read("npm install left-pad && git push origin main 2>&1 | tail -5").expect("read");
    assert_eq!(read.commands, ["npm install left-pad", "git push origin main", "tail -5"]);
    assert_eq!(read.patterns, ["npm install *", "git push *", "tail *"]);
    assert!(!read.reads_alone);
    assert!(Line::read("git status && ls -la | grep src").expect("read").reads_alone);
    assert!(!Line::read("cat notes > copy.txt").expect("read").reads_alone, "a file written");
    assert!(Line::read("ls 2>/dev/null").expect("read").reads_alone);
    assert!(!Line::read("find . -delete").expect("read").reads_alone);
    assert!(!Line::read("git diff --output=patch").expect("read").reads_alone);
    assert_eq!(Line::read("gh pr create --fill").expect("read").patterns, ["gh pr create *"]);
    assert!(Line::read("echo $(whoami)").is_err(), "a substitution no reading knows whole");
}

#[test]
fn each_mode_weighs_a_call_by_what_it_does_the_places_beside_the_workspace_its_own() {
    let bounds = Bounds {
        workspace: PathBuf::from("/work/app"),
        places: vec![PathBuf::from("/home/me/.octoii/etc/memory"), PathBuf::from("/tmp")],
        confines: true,
    };
    let none = Grants::default();
    let inside = Effect::Edits(vec![PathBuf::from("/work/app/src/main.rs")]);
    let outside = Effect::Edits(vec![PathBuf::from("/etc/hosts")]);
    let remembered = Effect::Edits(vec![PathBuf::from("/home/me/.octoii/etc/memory/-work-app/MEMORY.md")]);
    let scratch = Effect::Edits(vec![PathBuf::from("/tmp/octoii-501/home/threads/t1/count.sh")]);
    let tool = Effect::Acts {
        tool: "github.create_issue".into(),
    };
    let cases = [
        (Mode::Default, Effect::Reads, Verdict::Go(Writes::Workspace)),
        (Mode::Default, inside.clone(), Verdict::Go(Writes::Workspace)),
        (Mode::Default, outside.clone(), Verdict::Ask),
        (Mode::Default, remembered, Verdict::Go(Writes::Workspace)),
        (Mode::Default, scratch, Verdict::Go(Writes::Workspace)),
        (Mode::Default, runs("cargo build"), Verdict::Go(Writes::Workspace)),
        (Mode::Default, tool.clone(), Verdict::Ask),
        (Mode::Auto, inside.clone(), Verdict::Go(Writes::Workspace)),
        (Mode::Auto, outside.clone(), Verdict::Review),
        (Mode::Auto, runs("git status"), Verdict::Go(Writes::Anywhere)),
        (Mode::Auto, runs("cargo build"), Verdict::Review),
        (Mode::Auto, tool.clone(), Verdict::Review),
        (Mode::FullAccess, outside.clone(), Verdict::Go(Writes::Anywhere)),
    ];
    for (mode, effect, verdict) in cases {
        assert_eq!(weigh(&effect, mode, &none, &bounds), verdict, "{mode:?} {effect:?}");
    }
    let outside_sandbox = Effect::Runs {
        line: "brew install jq".into(),
        outside_sandbox: true,
        justification: Some("to install".into()),
    };
    assert_eq!(weigh(&outside_sandbox, Mode::Default, &none, &bounds), Verdict::Ask);
    let unconfined = Bounds {
        confines: false,
        ..bounds.clone()
    };
    assert_eq!(
        weigh(&runs("cargo build"), Mode::Default, &none, &unconfined),
        Verdict::Ask,
        "no sandbox to hold it"
    );
}

#[test]
fn what_the_person_granted_goes_ahead_writing_anywhere() {
    let bounds = Bounds {
        workspace: PathBuf::from("/work/app"),
        places: Vec::new(),
        confines: true,
    };
    let mut grants = Grants::default();
    grants.add(patterns_of(&runs("git push origin main")));
    grants.add(patterns_of(&Effect::Acts {
        tool: "github.create_issue".into(),
    }));
    grants.add(Grants {
        paths: vec!["/etc/app/**".into()],
        ..Grants::default()
    });
    assert_eq!(grants.commands, ["git push *"]);
    assert_eq!(
        weigh(&runs("git push upstream feature"), Mode::Default, &grants, &bounds),
        Verdict::Go(Writes::Anywhere)
    );
    assert_eq!(
        weigh(&runs("git push origin main && rm -rf /"), Mode::Default, &grants, &bounds),
        Verdict::Go(Writes::Workspace),
        "held, as the line is not wholly granted"
    );
    assert_eq!(
        weigh(&runs("echo $(git push)"), Mode::Default, &grants, &bounds),
        Verdict::Go(Writes::Workspace)
    );
    assert_eq!(
        weigh(
            &Effect::Acts {
                tool: "github.create_issue".into()
            },
            Mode::Default,
            &grants,
            &bounds
        ),
        Verdict::Go(Writes::Anywhere)
    );
    assert_eq!(
        weigh(&Effect::Edits(vec!["/etc/app/config.toml".into()]), Mode::Auto, &grants, &bounds),
        Verdict::Go(Writes::Anywhere)
    );
    assert_eq!(
        weigh(&Effect::Edits(vec!["/etc/other".into()]), Mode::Auto, &grants, &bounds),
        Verdict::Review
    );
}
