//! `octoii serve`: Octoii and its core extensions on this computer — the volume in the Home, the providers' keys from the
//! credstore and the login environment, the machine as the person's own with what the node keeps for itself hidden
//! from it — served to clients on the loopback address and to the devices that reach the node by its key through the
//! tunnel, at home on its relays, and its control socket opened to the node's commands, till it is asked to stop:
//! then every listener closes, the tunnel's sessions with them, and Octoii last.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use octoii::protocol::Machine;
use octoii::providers::SignInSetup;
use octoii::{Octoii, Setup};
use octoii_ai::drivers;
use octoii_ai::models::Provider;
use siphon_base::BaseExtension;
use siphon_browser::BrowserExtension;
use siphon_claude_code::ClaudeCodeExtension;
use siphon_delegate::DelegateExtension;
use siphon_files::FilesExtension;
use siphon_git::GitExtension;
use siphon_mcp::McpExtension;
use siphon_memory::MemoryExtension;
use siphon_question::QuestionExtension;
use siphon_skills::SkillsExtension;
use siphon_terminals::TerminalsExtension;
use siphon_thread::ThreadExtension;
use siphon_web_search::WebSearchExtension;

use crate::control::{self, Control};
use crate::credentials::Credstore;
use crate::home::Home;
use crate::http::{self, Node};
use crate::login;
use crate::pairing::{Links, with_access};

/// What the node is served with: its Home, the loopback port, the web app's folder, and the relays its tunnel is at
/// home on — where none, the one at the address the machine's account gave it, or none, reached directly alone.
#[derive(Debug, Clone)]
pub struct Served {
    pub home: Home,
    pub port: u16,
    pub app: Option<PathBuf>,
    pub relays: Vec<octoii_tunnel::RelayUrl>,
}

/// The providers the node reaches models through.
fn providers() -> Vec<Provider> {
    let drivers: [Arc<dyn octoii_ai::driver::Driver>; 11] = [
        Arc::new(drivers::OPENAI),
        Arc::new(drivers::ChatGpt::new()),
        Arc::new(drivers::ANTHROPIC),
        Arc::new(drivers::GOOGLE),
        Arc::new(drivers::OPENROUTER),
        Arc::new(drivers::DEEPSEEK),
        Arc::new(drivers::XAI),
        Arc::new(drivers::META),
        Arc::new(drivers::ZAI),
        Arc::new(drivers::OLLAMA),
        Arc::new(drivers::LMSTUDIO),
    ];
    drivers.into_iter().map(Provider::of).collect()
}

/// The sign-in host's stable UUID URI, named from the node's persisted public key.
fn sign_in_host_id(public_key: &str) -> String {
    let name = format!("urn:octoii:node:{public_key}");
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, name.as_bytes()).urn().to_string()
}

/// The machine as the person's own: their home folder and shell, the system's temporary folder, the threads shown at
/// the Home's `threads/`, what the node keeps for itself — its credentials and its state — hidden from what reaches the
/// machine through Octoii, `~/octoii`, where a thread of no folder works, the person's memory in the Home, and the
/// threads' scratchpads in the Home's `temporary` folder.
fn machine(home: &Home, temporary: &Path) -> octoii::machine::Host {
    let person = std::env::home_dir();
    octoii::machine::Host {
        workspace: person.as_ref().map(|person| person.join("octoii")),
        home: person,
        shell: std::env::var_os("SHELL").map(PathBuf::from),
        excludes: None,
        temporary: vec![std::env::temp_dir()],
        hidden: vec![home.credstore(), home.state()],
        threads: Some(home.threads()),
        memory: Some(home.memory()),
        scratchpads: Some(temporary.join("threads")),
    }
}

/// Octoii on this computer, started with its core extensions: a provider's sign-in in a browser come back to `sign_in`.
///
/// # Errors
/// Where the host runtime or volume cannot be opened.
pub fn siphon(home: &Home, sign_in: SignInSetup) -> anyhow::Result<Octoii> {
    building(home, sign_in)?.start().context("Octoii")
}

/// Octoii on this computer being built, its core extensions added — the browser, the terminals, the MCP servers, Claude
/// Code — for the node to add its own and start it.
///
/// # Errors
/// Where the host runtime or volume cannot be opened.
pub fn building(home: &Home, sign_in: SignInSetup) -> anyhow::Result<octoii::Building> {
    let providers = providers();
    let temporary = home.temporary().context("the Home's temporary folder")?;
    let setup = Setup {
        machine: machine(home, &temporary),
        sign_in,
        credentials: Arc::new(Credstore::new(home.credstore(), &providers)),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        providers,
        catalogue: octoii_ai::catalogue::bundled(),
        directory: Some(home.volume()),
    };
    let browser = BrowserExtension {
        program: None,
        profile: Some(home.browser().join("profile")),
        downloads: Some(temporary.join("downloads")),
    };
    let built = Octoii::builder(setup).context("the volume")?;
    let config = McpExtension {
        config: Some(home.config()),
    };
    let claude = ClaudeCodeExtension {
        program: None,
        folder: home.state().join("claude-code"),
    };
    Ok(built
        .extension(BaseExtension {
            config: Some(home.config()),
        })
        .extension(browser)
        .extension(TerminalsExtension)
        .extension(QuestionExtension)
        .extension(DelegateExtension)
        .extension(ThreadExtension)
        .extension(FilesExtension)
        .extension(MemoryExtension)
        .extension(GitExtension {
            worktrees: std::path::absolute(home.worktrees())?,
        })
        .extension(WebSearchExtension)
        .extension(SkillsExtension {
            config: Some(home.config()),
        })
        .extension(config)
        .extension(claude))
}

/// Serves the node as `served` says, till it is asked to stop: an interrupt, or a terminate.
///
/// # Errors
/// Where its Home cannot be made, its volume opened, or its listeners bound.
pub async fn serve(served: Served) -> anyhow::Result<()> {
    serve_until(served, stopped()).await
}

/// Serves the node as `served` says, till `stop` is done.
///
/// # Errors
/// Where its Home cannot be made, its volume opened, or its listeners bound.
pub async fn serve_until(served: Served, stop: impl Future<Output = ()> + Send + 'static) -> anyhow::Result<()> {
    let Served { home, port, app, relays } = served;
    home.prepare().context("the Home")?;
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .with_context(|| format!("the loopback port {port}"))?;
    let port = listener.local_addr()?.port();
    let loopback = format!("http://127.0.0.1:{port}");
    let key = octoii_tunnel::key::load_or_make(&home.key()).context("the node's key")?;
    let sign_in = SignInSetup {
        callback: format!("{loopback}/auth/callback"),
        host_id: sign_in_host_id(&key.public().to_string()),
    };
    let relays = if relays.is_empty() {
        login::address_of(&home).context("the machine's address")?.into_iter().collect()
    } else {
        relays
    };
    let relays = relays
        .into_iter()
        .map(|relay| octoii_tunnel::RelayConfig::new(relay, None))
        .collect();
    let tunnel = octoii_tunnel::Tunnel::builder(key)
        .relays(relays)
        .bind()
        .await
        .context("the tunnel")?;
    let links = Links {
        loopback: loopback.clone(),
        tunnel: tunnel.clone(),
    };
    let siphon = with_access(building(&home, sign_in)?, links.clone()).start().context("Octoii")?;
    // The models of each provider that lists its own — Claude Code's — asked for as the node starts, as a thread finds
    // its model, and the person picks one, in the directory alone.
    let models = std::sync::Arc::clone(&siphon.models);
    tokio::spawn(async move { models.refresh().await });
    // Where a thread of no folder works is there before any thread works in it.
    let workspace = siphon.machine.workspace_of(None);
    std::fs::create_dir_all(&workspace).with_context(|| format!("the folder of the threads of none, {}", workspace.display()))?;
    let node = Node {
        siphon: siphon.clone(),
        access: siphon.access(),
        machine: Machine::of(gethostname::gethostname().to_string_lossy().into_owned(), &siphon.machine),
        origins: vec![loopback.clone(), format!("http://localhost:{port}")],
        cookie: http::cookie_of(port),
        app,
    };
    let socket = home.socket().context("the control socket's folder")?;
    let _ = std::fs::remove_file(&socket);
    let controls = tokio::net::UnixListener::bind(&socket).context("the control socket")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    }
    let control = Control {
        access: node.access.clone(),
        machine_name: node.machine.name.clone(),
        links,
        home: home.root().to_owned(),
    };
    let router = http::router(node);
    let (serving, remote_router) = (tunnel.clone(), router.clone());
    tokio::spawn(async move { serving.serve(remote_router).await });
    tracing::info!(%loopback, "serving");
    let stopping = tokio_util::sync::CancellationToken::new();
    let (stop_loopback, stop_control) = (stopping.clone(), stopping.clone());
    let loopback_served = axum::serve(listener, router).with_graceful_shutdown(async move { stop_loopback.cancelled().await });
    let control_served =
        axum::serve(controls, control::router(control)).with_graceful_shutdown(async move { stop_control.cancelled().await });
    tokio::spawn(async move {
        stop.await;
        stopping.cancel();
    });
    let (served, controlled) = tokio::join!(loopback_served.into_future(), control_served.into_future());
    let _ = std::fs::remove_file(&socket);
    tunnel.close().await;
    siphon.close().await;
    served.context("the loopback")?;
    controlled.context("the control socket")?;
    Ok(())
}

/// Waits till the node is asked to stop: an interrupt, or a terminate.
async fn stopped() {
    #[cfg(unix)]
    {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = async { if let Some(terminate) = terminate.as_mut() { terminate.recv().await; } else { std::future::pending::<()>().await } } => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::sign_in_host_id;

    #[test]
    fn a_sign_in_host_uuid_follows_the_persisted_node_key() {
        let folder = tempfile::tempdir().expect("a folder");
        let path = folder.path().join("key");
        let key = octoii_tunnel::key::load_or_make(&path).expect("a node key");
        let first = sign_in_host_id(&key.public().to_string());
        let reloaded = octoii_tunnel::key::load_or_make(&path).expect("the persisted key");
        assert_eq!(
            first,
            sign_in_host_id(&reloaded.public().to_string()),
            "a restart keeps the host identity"
        );
        let uuid = uuid::Uuid::parse_str(&first).expect("a UUID URI");
        assert_eq!(uuid.get_version(), Some(uuid::Version::Sha1));
        assert_eq!(first, uuid.urn().to_string());
        let other = octoii_tunnel::key::load_or_make(&folder.path().join("other")).expect("another node key");
        assert_ne!(
            first,
            sign_in_host_id(&other.public().to_string()),
            "another node has its own host identity"
        );
    }
}
