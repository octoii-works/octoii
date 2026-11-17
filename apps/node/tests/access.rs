//! The access on the volume: a link redeemed once for a session its credential holds; that credential, wrong in a
//! byte, holds nothing.

use octoii::access::{self, Access, Holder};
use octoii::home::Home;

#[tokio::test(flavor = "multi_thread")]
async fn a_link_pairs_once_and_a_credential_holds_only_its_own_session() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("a Home");
    let siphon = octoii::serve::siphon(&home, siphon::providers::SignInSetup::default()).expect("Siphon");
    let access = Access::new(siphon.runtime.clone());
    let link = access
        .link(Some("phone".into()), access::ALL.to_vec(), access::LINK_LASTS)
        .await
        .expect("a link");
    let made = access.redeem(&link.credential, Holder::Bearer).await.expect("paired");
    assert!(access.redeem(&link.credential, Holder::Bearer).await.is_err(), "once");
    let found = access.held(&made.credential).expect("read").expect("its session");
    assert_eq!(
        (found.id, found.holder, found.label.as_deref()),
        (made.shown.id.clone(), Some(Holder::Bearer), Some("phone"))
    );
    let mut wrong = made.credential.clone();
    let last = wrong.pop().expect("a byte");
    wrong.push(if last == 'A' { 'B' } else { 'A' });
    assert!(
        access.held(&wrong).expect("read").is_none(),
        "a secret wrong in a byte holds nothing"
    );
    siphon.runtime.close().await;
}
