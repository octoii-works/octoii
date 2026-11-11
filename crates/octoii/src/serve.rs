//! `octoii serve`: Siphon and its core plugins on this computer — the volume in the Home, the providers' keys from the
//! credstore and the login environment, the machine as the person's own with what the node keeps for itself hidden
//! from it — served to clients on the loopback address and to the devices that reach the node by its key through the
//! tunnel, at home on its relays, and its control socket opened to the node's commands, till it is asked to stop:
//! then every listener closes, the tunnel's sessions with them, and Siphon last.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use octoii_ai::drivers;
use octoii_ai::models::Provider;
use siphon::providers::SignInSetup;
use siphon::{Setup, Siphon};
use siphon_browser::BrowserPlugin;
use siphon_mcp::McpPlugin;
use siphon_protocol::Machine;
use siphon_runtime::Options;
use siphon_terminals::TerminalsPlugin;

use crate::access::Access;
use crate::control::{self, Control};
use crate::credentials::Credstore;
use crate::home::Home;
use crate::http::{self, Node};
use crate::pairing::{AccessPlugin, Links};

/// What the node is served with: its Home, the loopback port, the web app's folder, and the relays its tunnel is at
/// home on — none, reached directly alone.
#[derive(Debug, Clone)]
pub struct Served {
    pub home: Home,
    pub port: u16,
    pub app: Option<PathBuf>,
    pub relays: Vec<octoii_tunnel::RelayUrl>,
}

/// The providers the node reaches models through.
fn providers() -> Vec<Provider> {
    let drivers: [&'static dyn octoii_ai::driver::Driver; 11] = [
        &drivers::OPENAI,
        &drivers::CHATGPT,
        &drivers::ANTHROPIC,
        &drivers::GOOGLE,
        &drivers::OPENROUTER,
        &drivers::DEEPSEEK,
        &drivers::XAI,
        &drivers::META,
        &drivers::ZAI,
        &drivers::OLLAMA,
        &drivers::LMSTUDIO,
    ];
    drivers.into_iter().map(Provider::of).collect()
}

/// The machine as the person's own: their home folder and shell, the system's temporary folder, the threads shown at
/// the Home's `threads/`, and what the node keeps for itself — its credentials and its state —
/// hidden from what reaches the machine through Siphon.
fn machine(home: &Home) -> siphon_machine::Host {
    siphon_machine::Host {
        home: std::env::home_dir(),
        shell: std::env::var_os("SHELL").map(PathBuf::from),
        excludes: None,
        temporary: vec![std::env::temp_dir()],
        hidden: vec![home.credstore(), home.state()],
        threads: Some(home.threads()),
    }
}

/// Siphon on this computer, started with its core plugins: a provider's sign-in in a browser come back to `sign_in`.
///
/// # Errors
/// Where Beak or the volume cannot be opened.
pub fn siphon(home: &Home, sign_in: SignInSetup) -> anyhow::Result<Siphon> {
    building(home, sign_in)?.start().context("Siphon")
}

/// Siphon on this computer being built, its core plugins added — the browser, the terminals, the MCP servers — for the
/// node to add its own and start it.
///
/// # Errors
/// Where Beak or the volume cannot be opened.
pub fn building(home: &Home, sign_in: SignInSetup) -> anyhow::Result<siphon::Building> {
    let beak = beak::Beak::new().context("Beak")?;
    let providers = providers();
    let setup = Setup {
        machine: machine(home),
        config: Some(home.config()),
        sign_in,
        credentials: Arc::new(Credstore::new(home.credstore(), &providers)),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        providers,
        catalogue: octoii_ai::catalogue::bundled(),
        options: Options {
            directory: Some(home.volume()),
            ..Options::default()
        },
    };
    let browser = BrowserPlugin {
        program: None,
        profile: Some(home.browser().join("profile")),
        downloads: Some(home.browser().join("downloads")),
    };
    let built = Siphon::builder(&beak, setup).context("the volume")?;
    let config = McpPlugin {
        config: Some(home.config()),
    };
    Ok(built.plugin(browser).plugin(TerminalsPlugin).plugin(config))
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
        callback: format!("{loopback}/sign-in/{{provider}}/callback"),
        host_id: key.public().to_string(),
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
    let access_plugin = AccessPlugin { links: links.clone() };
    let siphon = building(&home, sign_in)?.plugin(access_plugin).start().context("Siphon")?;
    let node = Node {
        siphon: siphon.clone(),
        access: Access::new(siphon.runtime.clone()),
        machine: Machine {
            name: gethostname::gethostname().to_string_lossy().into_owned(),
        },
        origins: vec![loopback.clone(), format!("http://localhost:{port}")],
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
        node: node.clone(),
        links,
        home: home.root().to_owned(),
    };
    let (serving, router) = (tunnel.clone(), http::router(node.clone()));
    tokio::spawn(async move { serving.serve(router).await });
    tracing::info!(%loopback, "serving");
    let stopping = tokio_util::sync::CancellationToken::new();
    let (stop_loopback, stop_control) = (stopping.clone(), stopping.clone());
    let loopback_served = axum::serve(listener, http::router(node)).with_graceful_shutdown(async move { stop_loopback.cancelled().await });
    let control_served =
        axum::serve(controls, control::router(control)).with_graceful_shutdown(async move { stop_control.cancelled().await });
    tokio::spawn(async move {
        stop.await;
        stopping.cancel();
    });
    let (served, controlled) = tokio::join!(loopback_served.into_future(), control_served.into_future());
    let _ = std::fs::remove_file(&socket);
    tunnel.close().await;
    siphon.runtime.close().await;
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
