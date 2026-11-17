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

#[tokio::test(flavor = "multi_thread")]
async fn stored_scopes_do_not_gain_new_git_permission() {
    use siphon::protocol::Scope;
    let folder = tempfile::tempdir().unwrap();
    let home = Home::at(folder.path());
    home.prepare().unwrap();
    let siphon = octoii::serve::siphon(&home, siphon::providers::SignInSetup::default()).unwrap();
    let access = Access::new(siphon.runtime.clone());
    let scopes: Vec<_> = access::ALL.iter().copied().filter(|scope| *scope != Scope::GitOperate).collect();
    let link = access
        .link(Some("existing client".into()), scopes.clone(), access::LINK_LASTS)
        .await
        .unwrap();
    let session = access.redeem(&link.credential, Holder::Bearer).await.unwrap();
    siphon.runtime.close().await;
    drop(access);
    drop(siphon);
    let siphon = octoii::serve::siphon(&home, siphon::providers::SignInSetup::default()).unwrap();
    let access = Access::new(siphon.runtime.clone());
    assert_eq!(access.held(&session.credential).unwrap().unwrap().scopes, scopes);
    siphon.runtime.close().await;
}
