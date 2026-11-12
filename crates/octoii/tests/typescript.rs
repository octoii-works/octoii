//! The client's TypeScript of the protocol is what the node's Siphon and its extensions make of it: written anew where
//! `UPDATE_EXPECT` is set, as expect-test's snapshots are, and compared otherwise, so a change of a type the client was
//! not given fails.

use octoii::home::Home;

const MODULE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/client/src/generated/protocol.ts");

#[tokio::test(flavor = "multi_thread")]
async fn the_client_s_typescript_is_what_the_node_s_siphon_and_extensions_make() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("a Home");
    let tunnel = octoii_tunnel::Tunnel::builder(octoii_tunnel::SecretKey::generate())
        .bind()
        .await
        .expect("a tunnel");
    let links = octoii::pairing::Links {
        loopback: "http://127.0.0.1:7600".into(),
        tunnel,
    };
    let building = octoii::serve::building(&home, siphon::providers::SignInSetup::default()).expect("Siphon");
    let siphon = building
        .extension(octoii::pairing::AccessExtension { links })
        .start()
        .expect("started");
    let made = siphon_protocol::typescript(siphon.sets());
    siphon.runtime.close().await;
    if std::env::var_os("UPDATE_EXPECT").is_some() {
        std::fs::write(MODULE, &made).expect("written");
        return;
    }
    let kept = std::fs::read_to_string(MODULE).unwrap_or_default();
    assert!(
        kept == made,
        "{MODULE} is not what the node makes; run this test with UPDATE_EXPECT=1"
    );
}
