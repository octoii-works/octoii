//! The client's TypeScript of the protocol is what the Rust types make of it: written anew where `UPDATE_EXPECT` is
//! set, as expect-test's snapshots are, and compared otherwise, so a change of a type the client was not given fails.

const MODULE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/client/src/generated/protocol.ts");

#[test]
fn the_client_s_typescript_is_what_the_rust_types_make() {
    let made = siphon_protocol::typescript();
    if std::env::var_os("UPDATE_EXPECT").is_some() {
        std::fs::write(MODULE, &made).expect("written");
        return;
    }
    let kept = std::fs::read_to_string(MODULE).unwrap_or_default();
    assert!(
        kept == made,
        "{MODULE} is not what the types make; run this test with UPDATE_EXPECT=1"
    );
}
