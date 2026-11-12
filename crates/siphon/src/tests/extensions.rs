//! The extensions reach Siphon by its interface alone: none depends on Beak or Siphon's runtime, which write anything.

use std::path::Path;

/// The crates below Siphon, which an extension never names.
const BELOW: [&str; 2] = ["vendor/beak/", "crates/siphon-runtime"];

#[test]
fn no_extension_depends_on_what_is_below_siphon() {
    let extensions = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions");
    let mut named = Vec::new();
    for entry in std::fs::read_dir(&extensions).expect("the extensions") {
        let manifest = entry.expect("an extension").path().join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        for line in text.lines().filter(|line| BELOW.iter().any(|below| line.contains(below))) {
            named.push(format!("{}: {line}", manifest.display()));
        }
    }
    assert!(named.is_empty(), "extensions depend on what is below Siphon:\n{}", named.join("\n"));
}
