//! HTML preparation preserves source files and makes local images durable through the machine's namespace and bounds.

use base64::Engine as _;
use octoii::machine::{Base, Host, Writes};
use octoii::testing::Runtime;
use siphon_html::{Error, prepare};

#[tokio::test(flavor = "multi_thread")]
async fn html_text_is_returned_unchanged_and_other_formats_are_refused() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let fixture = Runtime::memory().expect("Runtime");
    let machine = octoii::testing::commands(&fixture)
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
        .set_len(octoii::threads::files::BYTES_MAX as u64 + 1)
        .expect("past the limit");
    let fixture = Runtime::memory().expect("Runtime");
    let machine = octoii::testing::commands(&fixture)
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
        Err(Error::Read(octoii::machine::Error::TooLarge { .. }))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn quoted_images_and_css_urls_are_embedded_relative_to_the_source_with_their_actual_media_type() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    std::fs::create_dir_all(root.join("reports/assets")).expect("asset folder");
    let png = b"\x89PNG\r\n\x1a\nimage bytes";
    let svg =
        b"\xef\xbb\xbf<?xml version='1.0'?><!--before--><!DOCTYPE svg [<!ENTITY name 'a > b'>]><svg xmlns='http://www.w3.org/2000/svg'/>";
    std::fs::write(root.join("reports/assets/picture.jpg"), png).expect("a PNG with a misleading extension");
    std::fs::write(root.join("icon.svg"), svg).expect("an SVG");
    let source = format!(
        "<!doctype html><img src=\"assets/picture.jpg?version=1:2\">\
         <style>.a{{background:url(../icon.svg#symbol)}}.b{{background:url('assets/picture.jpg')}}</style>\
         <script>const image = `{}/icon.svg`;</script>\
         <img src='~/icon.svg'><img src='https://example.test/remote.png'>\
         <img src='//example.test/remote.png'><img src='data:image/svg+xml,<svg/>'>",
        root.display()
    );
    std::fs::write(root.join("reports/report.html"), &source).expect("HTML source");
    let fixture = Runtime::memory().expect("Runtime");
    let machine = octoii::testing::commands(&fixture)
        .open(
            &Host::default(),
            Base {
                workspace: root.clone(),
                home: Some(root.clone()),
            },
            &Writes::Within(Vec::new()),
        )
        .expect("the machine");
    let png_uri = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png));
    let svg_uri = format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    );
    let expected = source
        .replace("assets/picture.jpg?version=1:2", &png_uri)
        .replace("assets/picture.jpg", &png_uri)
        .replace("../icon.svg#symbol", &format!("{svg_uri}#symbol"))
        .replace(&format!("{}/icon.svg", root.display()), &svg_uri)
        .replace("~/icon.svg", &svg_uri);
    assert_eq!(prepare(&machine, "reports/report.html").expect("embedded"), expected);
    assert_eq!(std::fs::read_to_string(root.join("reports/report.html")).expect("source"), source);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_hidden_and_renamed_secret_images_are_refused() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    std::fs::create_dir(root.join("private")).expect("private folder");
    std::fs::write(root.join("private/hidden.png"), b"\x89PNG\r\n\x1a\nhidden").expect("hidden image");
    std::fs::write(root.join("secret.png"), "API_KEY=private").expect("a renamed secret");
    std::fs::write(root.join("secret.svg"), "<!-- <svg/> -->API_KEY=private").expect("a fake SVG");
    std::fs::write(root.join("prefix.svg"), "<svg-secret>private</svg-secret>").expect("a misleading root");
    std::fs::write(
        root.join("doctype.svg"),
        "<!DOCTYPE secret [<!-- ]><svg/> -->]><secret>API_KEY=private</secret>",
    )
    .expect("a fake root inside a doctype comment");
    let fixture = Runtime::memory().expect("Runtime");
    let machine = octoii::testing::commands(&fixture)
        .open(
            &Host {
                hidden: vec![root.join("private")],
                ..Host::default()
            },
            Base {
                workspace: root.clone(),
                home: None,
            },
            &Writes::Within(Vec::new()),
        )
        .expect("the machine");
    for reference in ["secret.png", "secret.svg", "prefix.svg", "doctype.svg"] {
        std::fs::write(root.join("report.html"), format!("<img src='{reference}'>")).expect("HTML");
        assert!(matches!(prepare(&machine, "report.html"), Err(Error::NotImage(_))), "{reference}");
    }
    for reference in ["missing.png", "private/hidden.png"] {
        std::fs::write(root.join("report.html"), format!("<img src='{reference}'>")).expect("HTML");
        assert!(
            matches!(prepare(&machine, "report.html"), Err(Error::ImageRead { .. })),
            "{reference}"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("private/hidden.png"), root.join("linked.png")).expect("a hidden image alias");
        std::fs::write(root.join("report.html"), "<img src='linked.png'>").expect("HTML");
        assert!(matches!(prepare(&machine, "report.html"), Err(Error::ImageRead { .. })));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_size_limit_counts_base64_expansion_and_every_repeated_image() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let mut png = vec![0; 13 << 20];
    png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    std::fs::write(root.join("large.png"), &png).expect("an image below the raw file limit");
    std::fs::write(root.join("report.html"), "<img src='large.png'><img src='large.png'>").expect("HTML");
    let fixture = Runtime::memory().expect("Runtime");
    let machine = octoii::testing::commands(&fixture)
        .open(
            &Host::default(),
            Base {
                workspace: root,
                home: None,
            },
            &Writes::Within(Vec::new()),
        )
        .expect("the machine");
    assert!(matches!(prepare(&machine, "report.html"), Err(Error::TooLarge)));
}
