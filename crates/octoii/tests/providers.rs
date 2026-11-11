//! The providers as the person manages them, through a connection to the node's Siphon: each listed with how it is
//! connected and the kind of credential kept, a key set and forgotten, an account added — a provider beside the host's
//! — renamed and removed, the models listed, a sign-in refused of a provider reached by a key.

use octoii::home::Home;
use serde_json::{Value, json};
use siphon::testing::Client;
use siphon_protocol::Scope;

async fn asked(client: &mut Client, method: &str, params: Value) -> Value {
    let id = client.call(method, params).await;
    client.answer(&id).await
}

fn provider<'a>(listed: &'a Value, id: &str) -> Option<&'a Value> {
    listed["result"]["providers"]
        .as_array()?
        .iter()
        .find(|provider| provider["id"] == id)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_person_manages_providers_keys_and_accounts() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("a Home");
    let siphon = octoii::serve::siphon(&home, siphon::providers::SignInSetup::default()).expect("Siphon");
    let mut client = Client::connect(&siphon, vec![Scope::ThreadsRead, Scope::ProvidersManage], 64);
    client.message().await;

    let listed = asked(&mut client, "providers.list", json!({})).await;
    let zai = provider(&listed, "zai").expect("Z.ai");
    assert_eq!((zai["connects"].as_str(), zai.get("credential")), (Some("api_key"), None));
    let chatgpt = provider(&listed, "chatgpt").expect("ChatGPT");
    assert_eq!(chatgpt["connects"], "sign_in");

    let status = asked(&mut client, "providers.status", json!({ "provider": "zai" })).await;
    assert!(
        status["result"].is_object(),
        "a provider's standing is asked of its driver: {status}"
    );

    asked(&mut client, "credentials.set", json!({ "provider": "zai", "api_key": "sk-test" })).await;
    let listed = asked(&mut client, "providers.list", json!({})).await;
    assert_eq!(
        provider(&listed, "zai").expect("Z.ai")["credential"],
        "api_key",
        "a key kept, never shown"
    );
    assert!(!listed.to_string().contains("sk-test"));
    asked(&mut client, "credentials.remove", json!({ "provider": "zai" })).await;
    let listed = asked(&mut client, "providers.list", json!({})).await;
    assert!(provider(&listed, "zai").expect("Z.ai").get("credential").is_none());

    let added = asked(
        &mut client,
        "accounts.add",
        json!({ "provider": "zai", "name": "Work", "key": "a1" }),
    )
    .await;
    let account = added["result"]["account"].as_str().expect("an account").to_owned();
    let listed = asked(&mut client, "providers.list", json!({})).await;
    let instance = provider(&listed, &account).expect("an instance beside the host's");
    assert_eq!(
        (instance["name"].as_str(), instance["account"]["provider"].as_str()),
        (Some("Work"), Some("zai"))
    );
    asked(&mut client, "accounts.rename", json!({ "account": account, "name": "Home" })).await;
    let listed = asked(&mut client, "providers.list", json!({})).await;
    assert_eq!(provider(&listed, &account).expect("renamed")["name"], "Home");
    asked(&mut client, "accounts.remove", json!({ "account": account })).await;
    let listed = asked(&mut client, "providers.list", json!({})).await;
    assert!(provider(&listed, &account).is_none(), "gone");

    let models = asked(&mut client, "models.list", json!({})).await;
    assert!(
        models["result"]["models"].as_array().is_some_and(|models| !models.is_empty()),
        "the catalogue's models"
    );
    let refused = asked(&mut client, "providers.sign-in", json!({ "provider": "zai" })).await;
    assert_eq!(
        refused["error"]["code"], "refused_by_provider",
        "a provider reached by a key takes no sign-in"
    );
    let unknown = asked(&mut client, "providers.sign-in", json!({ "provider": "nowhere" })).await;
    assert_eq!(unknown["error"]["code"], "no_such_provider");
    siphon.runtime.close().await;
}
