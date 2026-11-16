//! The browser receives exactly the HTML file's text; reading it still goes through the machine's namespace and bounds.

use siphon::machine::{Base, Host, Writes};
use siphon::testing::Beak;
use siphon_html::{Error, prepare};

#[tokio::test(flavor = "multi_thread")]
async fn html_text_is_returned_unchanged_and_other_formats_are_refused() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let beak = Beak::new().expect("Beak");
    let machine = siphon::testing::commands(&beak)
        .open(
            &Host::default(),
            Base {
                workspace: root.clone(),
                home: None,
            },
            &Writes::Within(Vec::new()),
        )
        .expect("the machine");
    for (name, source) in [
        ("report.HTML", "\u{feff}<!doctype html>\r\n<title>原件</title>\r\n"),
        ("page.htm", "<button onclick=\"this.textContent='done'\">Run</button>"),
        ("empty.html", ""),
    ] {
        std::fs::write(root.join(name), source).expect("written");
        assert_eq!(prepare(&machine, name).expect("HTML text"), source);
        assert_eq!(std::fs::read(root.join(name)).expect("the original"), source.as_bytes());
    }
    for (name, bytes) in [
        ("page.txt", b"<p>HTML syntax</p>".as_slice()),
        ("binary.html", b"<p>\0</p>".as_slice()),
        ("invalid.html", b"<p>\xff</p>".as_slice()),
        ("report.html", b"%PDF-1.7\nmisnamed\n%%EOF\n".as_slice()),
    ] {
        std::fs::write(root.join(name), bytes).expect("written");
        assert!(matches!(prepare(&machine, name), Err(Error::NotHtml)), "{name}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_directories_hidden_files_and_oversized_files_remain_unreadable() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let hidden = root.join("private");
    std::fs::create_dir(&hidden).expect("a hidden folder");
    std::fs::write(hidden.join("secret.html"), "<p>private</p>").expect("written");
    std::fs::File::create(root.join("large.html"))
        .expect("a file")
        .set_len(siphon::threads::files::BYTES_MAX as u64 + 1)
        .expect("past the limit");
    let beak = Beak::new().expect("Beak");
    let machine = siphon::testing::commands(&beak)
        .open(
            &Host {
                hidden: vec![hidden],
                ..Host::default()
            },
            Base {
                workspace: root,
                home: None,
            },
            &Writes::Within(Vec::new()),
        )
        .expect("the machine");
    for path in ["", "bad\0.html", "missing.html", ".", "private/secret.html"] {
        assert!(matches!(prepare(&machine, path), Err(Error::Read(_))), "{path}");
    }
    assert!(matches!(
        prepare(&machine, "large.html"),
        Err(Error::Read(siphon::machine::Error::TooLarge { .. }))
    ));
}
