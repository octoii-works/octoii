//! The access on the volume: a link redeemed once for a session a device holds by its key, found by that key alone;
//! a session's credential, wrong in a byte, holds nothing.

use octoii::access::{self, Access, Holder};
use octoii::home::Home;

#[tokio::test(flavor = "multi_thread")]
async fn a_device_s_session_is_found_by_its_key_and_a_credential_holds_only_its_own() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("a Home");
    let siphon = octoii::serve::siphon(&home).expect("Siphon");
    let access = Access::new(siphon.runtime.clone());
    let link = access
        .link(Some("phone".into()), access::ALL.to_vec(), access::LINK_LASTS)
        .await
        .expect("a link");
    let made = access
        .redeem(&link.credential, Holder::Key, Some("key-of-the-phone".into()))
        .await
        .expect("paired");
    assert!(
        access.redeem(&link.credential, Holder::Key, Some("another".into())).await.is_err(),
        "once"
    );
    let found = access.held_by_key("key-of-the-phone").expect("read").expect("its session");
    assert_eq!(
        (found.id, found.holder, found.label.as_deref()),
        (made.shown.id.clone(), Some(Holder::Key), Some("phone"))
    );
    assert!(access.held_by_key("another").expect("read").is_none());
    assert!(access.held(&made.credential).expect("read").is_some());
    let mut wrong = made.credential.clone();
    let last = wrong.pop().expect("a byte");
    wrong.push(if last == 'A' { 'B' } else { 'A' });
    assert!(
        access.held(&wrong).expect("read").is_none(),
        "a secret wrong in a byte holds nothing"
    );
    siphon.runtime.close().await;
}
