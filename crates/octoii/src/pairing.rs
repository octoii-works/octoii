//! Pairing, and the access the node's clients hold, as the node's own methods of the protocol — the node a host that
//! adds what is its own to Siphon as any plugin does: the sessions and the links not yet used; a link made to pair a
//! client, its addresses — on this computer, at the loopback; on another device, at the web app, where the node has a
//! home relay, with the node's key and relay by which the page dials it — made in one place for the protocol and the
//! node's commands alike; a session or a link revoked.

use serde::{Deserialize, Serialize};
use siphon::{Builder, Caller, Plugin};
use siphon_protocol::{Error, Nothing, Scope};
use ts_rs::TS;

use crate::access::{self, Access, AccessEntry, Refused};

/// Where the web app is served to a device that reaches the node through the tunnel.
pub const APP: &str = "https://app.octoii.com";

/// What a pairing link is made with: the node's loopback address, and its tunnel.
#[derive(Debug, Clone)]
pub struct Links {
    pub loopback: String,
    pub tunnel: octoii_tunnel::Tunnel,
}

/// A link made: what it grants, its credential — shown this once — and the addresses that open it: on this computer,
/// and on another device where the node has a home relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LinkMade {
    pub link: AccessEntry,
    pub credential: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub device: Option<String>,
}

impl Links {
    /// A link that pairs one client with `scopes` — everything, where none — labelled `label`.
    ///
    /// # Errors
    /// The volume's failure.
    pub async fn made(&self, access: &Access, label: Option<String>, scopes: Option<Vec<Scope>>) -> Result<LinkMade, Refused> {
        let scopes = scopes.unwrap_or_else(|| access::ALL.to_vec());
        let made = access.link(label, scopes, access::LINK_LASTS).await?;
        let url = format!("{}/pair#{}", self.loopback, made.credential);
        let addr = self.tunnel.addr();
        let device = addr
            .relay_urls()
            .next()
            .map(|relay| format!("{APP}/pair#key={}&relay={relay}&link={}", self.tunnel.key(), made.credential));
        Ok(LinkMade {
            link: made.shown,
            credential: made.credential,
            url,
            device,
        })
    }
}

siphon_protocol::methods! {
    /// The sessions the node's clients hold and the links not yet used.
    "access.list" ListAccess(Nothing) -> AccessListed, Some(Scope::AccessRead);
    /// A link that pairs one client, once, while it lasts.
    "access.pair" Pair(PairWith) -> LinkMade, Some(Scope::AccessWrite);
    /// A session or a link revoked.
    "access.revoke" Revoke(OfAccessEntry) -> Revoked, Some(Scope::AccessWrite);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AccessListed {
    pub sessions: Vec<AccessEntry>,
    pub links: Vec<AccessEntry>,
}

/// What a link is made with: its label, and what it grants — everything, where none is given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PairWith {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scopes: Option<Vec<Scope>>,
}

/// A session or a link, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfAccessEntry {
    pub id: String,
}

/// What was revoked: a session, or a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Revoked {
    pub revoked: String,
}

/// The node's access as a plugin of Siphon's, its links made as `links` makes them.
#[derive(Debug, Clone)]
pub struct AccessPlugin {
    pub links: Links,
}

impl Plugin for AccessPlugin {
    fn name(&self) -> &'static str {
        "access"
    }

    fn build(self: Box<Self>, builder: &mut Builder) {
        let links = self.links;
        builder.methods(set(), Method::read, move |caller: Caller, method| {
            let links = links.clone();
            async move { answered(&links, &caller, method).await }
        });
    }
}

fn failed(error: &Refused) -> Error {
    let code = match error {
        Refused::NoSuch | Refused::NoSuchLink => "no_such_access",
        _ => "failed",
    };
    Error {
        code: code.to_owned(),
        message: error.to_string(),
    }
}

async fn answered(links: &Links, caller: &Caller, method: Method) -> Result<serde_json::Value, Error> {
    let access = Access::new(caller.siphon.runtime.clone());
    let json = |value: serde_json::Result<serde_json::Value>| {
        value.map_err(|error| Error {
            code: "failed".into(),
            message: error.to_string(),
        })
    };
    match method {
        Method::ListAccess(Nothing {}) => {
            let (sessions, links) = access.list().map_err(|error| failed(&error))?;
            json(serde_json::to_value(AccessListed { sessions, links }))
        }
        Method::Pair(with) => {
            let made = links.made(&access, with.label, with.scopes).await.map_err(|error| failed(&error))?;
            json(serde_json::to_value(made))
        }
        Method::Revoke(of) => {
            let what = access.revoke(&of.id).await.map_err(|error| failed(&error))?;
            json(serde_json::to_value(Revoked { revoked: what.to_owned() }))
        }
    }
}
