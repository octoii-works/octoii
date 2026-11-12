//! Terminals as a thread's person works in them: a login shell in the thread's folder that writes and takes a title,
//! numbered in its thread the lowest number free, sized, drawn again whole, and gone as its shell ends.

use std::path::PathBuf;
use std::time::Duration;

use beak::{Beak, Kernel, Options};
use bytes::Bytes;
use siphon_machine::Host;
use siphon_terminals::{Error, Event, Running, Size, Terminals};

const SIZE: Size = Size { rows: 24, cols: 80 };

/// Waits till `done` holds of what the terminals tell.
async fn until(events: &mut tokio::sync::broadcast::Receiver<Event>, done: impl Fn(&Event) -> bool) {
    let waiting = async {
        loop {
            if done(&events.recv().await.expect("an event")) {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("in time");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_runs_the_person_s_shell_in_its_folder_numbered_in_its_thread_till_its_shell_ends() {
    let beak = Beak::new().expect("Beak");
    let kernel = Kernel::open(&beak, &Options::default()).expect("a kernel").0;
    let folder = tempfile::tempdir().expect("a folder");
    let cwd = folder.path().canonicalize().expect("its real path");
    let host = Host {
        shell: Some(PathBuf::from("/bin/sh")),
        ..Host::default()
    };
    let terminals = Terminals::start(kernel, host);
    let mut events = terminals.events();
    let first = terminals.open("t1", cwd.clone(), SIZE, Running::Shell).await.expect("opened");
    let second = terminals.open("t1", cwd.clone(), SIZE, Running::Shell).await.expect("opened");
    let elsewhere = terminals.open("t2", cwd.clone(), SIZE, Running::Shell).await.expect("opened");
    let numbers = |shown: Vec<siphon_terminals::Shown>| shown.into_iter().map(|each| each.number).collect::<Vec<_>>();
    assert_eq!(numbers(terminals.of_thread("t1").await), [1, 2]);
    assert_eq!(numbers(terminals.of_thread("t2").await), [1]);
    terminals
        .write(
            &first,
            Bytes::from_static(b"printf '\\033]0;building\\007'; echo made-$((40+2)) in $(pwd)\n"),
        )
        .await
        .expect("written");
    let id = first.clone();
    until(&mut events, |event| matches!(event, Event::Changed { thread } if thread == "t1")).await;
    let waiting = async {
        loop {
            let chunk = terminals.read_at(&id, 0).await.expect("read");
            if String::from_utf8_lossy(&chunk.bytes).contains(&format!("made-42 in {}", cwd.display())) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("it wrote in time");
    let titled = terminals.of_thread("t1").await;
    assert_eq!(titled[0].title, "building");
    assert!(!terminals.screen(&first).await.expect("drawn").sequences.is_empty());
    assert!(terminals.resize(&first, Size { rows: 20, cols: 70 }).await.expect("sized"));
    assert!(
        !terminals.resize(&first, Size { rows: 20, cols: 70 }).await.expect("sized"),
        "the same size changes nothing"
    );
    assert!(matches!(
        terminals.resize(&first, Size { rows: 0, cols: 70 }).await,
        Err(Error::EmptySize)
    ));
    terminals.close(&first);
    let third = terminals.open("t1", cwd.clone(), SIZE, Running::Shell).await.expect("opened");
    assert_eq!(numbers(terminals.of_thread("t1").await), [2, 1], "the lowest number free");
    terminals.write(&third, Bytes::from_static(b"exit\n")).await.expect("written");
    let waiting = async {
        while terminals.of_thread("t1").await.len() > 1 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("gone as its shell ended");
    assert!(matches!(terminals.screen(&third).await, Err(Error::NoSuchTerminal)));
    terminals.close_thread("t1");
    terminals.close(&elsewhere);
    let waiting = async {
        while !terminals.of_thread("t1").await.is_empty() || !terminals.of_thread("t2").await.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("closed");
    let _ = second;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_line_the_model_runs_tells_its_end_with_its_screen_and_one_closed_tells_none() {
    let beak = Beak::new().expect("Beak");
    let kernel = Kernel::open(&beak, &Options::default()).expect("a kernel").0;
    let folder = tempfile::tempdir().expect("a folder");
    let cwd = folder.path().canonicalize().expect("its real path");
    let host = Host {
        shell: Some(PathBuf::from("/bin/sh")),
        ..Host::default()
    };
    let terminals = Terminals::start(kernel, host);
    let mut events = terminals.events();
    let writes = siphon_machine::Writes::Within(vec![cwd.clone()]);
    let running = Running::Line {
        line: "echo served; exit 3".into(),
        writes: writes.clone(),
        set: Vec::new(),
    };
    let id = terminals.open("t1", cwd.clone(), SIZE, running).await.expect("opened");
    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Event::Ended {
                id: of,
                exit,
                screen,
                number,
                ..
            } = events.recv().await.expect("an event")
                && of == id
            {
                return (exit, screen, number);
            }
        }
    })
    .await
    .expect("in time");
    assert_eq!(ended.0, beak::Exit::Code(3));
    assert!(ended.1.contains("served"), "its last screen: {}", ended.1);
    assert_eq!(ended.2, 1);
    let lasting = Running::Line {
        line: "sleep 30".into(),
        writes,
        set: Vec::new(),
    };
    let id = terminals.open("t1", cwd.clone(), SIZE, lasting).await.expect("opened");
    assert_eq!(terminals.of_thread("t1").await[0].line.as_deref(), Some("sleep 30"));
    terminals.close(&id);
    until(&mut events, |event| matches!(event, Event::Closed { id: of } if *of == id)).await;
    let told_end = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            if matches!(events.recv().await, Ok(Event::Ended { .. })) {
                return;
            }
        }
    })
    .await;
    assert!(told_end.is_err(), "a terminal closed tells no end");
}
