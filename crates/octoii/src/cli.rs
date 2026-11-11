//! The `octoii` program's commands: `serve` runs the node; every other asks the running node through its control
//! socket, so none needs to be told where the node listens.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use hyper::Method;
use serde_json::{Value, json};
use siphon_protocol::Scope;

use crate::control::{self, Asked};
use crate::home::Home;
use crate::serve::{self, Served};
use crate::service;

/// Octoii: the durable coding agent, on this computer, reached from anywhere.
#[derive(Debug, Parser)]
#[command(name = "octoii", version)]
struct Cli {
    /// The node's Home; `~/.octoii` where none is given.
    #[arg(long, env = "OCTOII_HOME", global = true)]
    home: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Runs the node: Siphon on this computer, served on the loopback address.
    Serve {
        /// The loopback port.
        #[arg(long, default_value_t = 7600)]
        port: u16,
        /// The web app's folder; the one installed beside the program, where none is given.
        #[arg(long)]
        app: Option<PathBuf>,
        /// A relay the node's tunnel is at home on, by its URL; again for another.
        #[arg(long)]
        relay: Vec<octoii_tunnel::RelayUrl>,
    },
    /// How the running node stands.
    Status {
        /// As JSON.
        #[arg(long)]
        json: bool,
    },
    /// A link that pairs a browser with the node, once, within five minutes.
    Pair {
        /// What the paired client is called.
        #[arg(long)]
        label: Option<String>,
        /// What it may do, comma-separated; everything, where none is given.
        #[arg(long, value_delimiter = ',')]
        scopes: Option<Vec<String>>,
    },
    /// The web app opened in this computer's browser, paired by a link made for it.
    Open {
        /// What the browser is called.
        #[arg(long)]
        label: Option<String>,
    },
    /// The node kept running as the person's own service: started at login, started again where it ends.
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// A session for a program: its bearer token, shown this once.
    Token {
        /// What the program is called.
        #[arg(long)]
        label: Option<String>,
        /// What it may do, comma-separated; everything, where none is given.
        #[arg(long, value_delimiter = ',')]
        scopes: Option<Vec<String>>,
    },
    /// The sessions the node's clients hold and the links not yet used.
    Access {
        #[command(subcommand)]
        command: Option<AccessCommand>,
        /// As JSON.
        #[arg(long)]
        json: bool,
    },
    /// The program's version.
    Version,
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    /// Installs the service and starts it.
    Install,
    /// Stops the service and removes it.
    Uninstall,
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Subcommand)]
enum AccessCommand {
    /// Revokes a session or a link, by its id.
    Revoke { id: String },
}

/// Runs the command the program was given.
#[must_use]
pub fn main() -> ExitCode {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();
    let Some(home) = cli.home.map(Home::at).or_else(Home::of_person) else {
        eprintln!("octoii: no home folder to keep the node's Home in; give one with --home");
        return ExitCode::FAILURE;
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("octoii: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(home, cli.command)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("octoii: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// The web app installed beside the program, as a system's programs keep their data: `../share/octoii/app`.
fn installed_app() -> Option<PathBuf> {
    let app = std::env::current_exe().ok()?.parent()?.parent()?.join("share/octoii/app");
    app.join("index.html").exists().then_some(app)
}

/// The scopes named, as the protocol names them.
fn scopes(named: Option<Vec<String>>) -> anyhow::Result<Option<Vec<Scope>>> {
    named
        .map(|named| {
            named
                .into_iter()
                .map(|name| serde_json::from_value(Value::String(name.clone())).map_err(|_| anyhow::anyhow!("no scope {name}")))
                .collect()
        })
        .transpose()
}

async fn run(home: Home, command: Command) -> anyhow::Result<()> {
    let socket = home.socket()?;
    match command {
        Command::Serve { port, app, relay } => {
            let served = Served {
                home,
                port,
                app: app.or_else(installed_app),
                relays: relay,
            };
            serve::serve(served).await
        }
        Command::Status { json } => status(&socket, json).await,
        Command::Pair { label, scopes: named } => pair(&socket, label, named).await,
        Command::Open { label } => {
            let asked = Asked {
                label: label.or_else(|| Some("this computer".into())),
                scopes: None,
            };
            let made = control::ask(&socket, Method::POST, "/links", Some(json!(asked))).await?;
            let url = made["url"].as_str().unwrap_or_default();
            webbrowser::open(url).map_err(|error| anyhow::anyhow!("the browser: {error}; open {url} in it"))
        }
        Command::Service { command } => {
            let asked = match command {
                ServiceCommand::Install => service::Asked::Install,
                ServiceCommand::Uninstall => service::Asked::Uninstall,
                ServiceCommand::Start => service::Asked::Start,
                ServiceCommand::Stop => service::Asked::Stop,
                ServiceCommand::Restart => service::Asked::Restart,
            };
            service::service(&home, asked)
        }
        Command::Token { label, scopes: named } => {
            let asked = Asked {
                label,
                scopes: scopes(named)?,
            };
            let made = control::ask(&socket, Method::POST, "/tokens", Some(json!(asked))).await?;
            println!("{}", made["token"].as_str().unwrap_or_default());
            eprintln!("Send it as `Authorization: Bearer <token>`; it is shown this once.");
            Ok(())
        }
        Command::Access { command, json } => access(&socket, command, json).await,
        Command::Version => {
            println!("octoii {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

/// How the running node stands, as text or as JSON.
async fn status(socket: &std::path::Path, json: bool) -> anyhow::Result<()> {
    let status = control::ask(socket, Method::GET, "/status", None).await?;
    if json {
        println!("{status:#}");
        return Ok(());
    }
    let field = |name: &str| status[name].as_str().unwrap_or("?").to_owned();
    println!("running {} on {}", field("version"), field("machine"));
    println!("home      {}", field("home"));
    println!("loopback  {}", field("loopback"));
    println!("key       {}", field("key"));
    println!("relay     {}", status["relay"].as_str().unwrap_or("none: reached directly alone"));
    Ok(())
}

/// A link that pairs one client: its address on this computer, and — where the node has a home relay — on another
/// device, drawn as a QR code too.
async fn pair(socket: &std::path::Path, label: Option<String>, named: Option<Vec<String>>) -> anyhow::Result<()> {
    let asked = Asked {
        label,
        scopes: scopes(named)?,
    };
    let made = control::ask(socket, Method::POST, "/links", Some(json!(asked))).await?;
    println!("{}", made["url"].as_str().unwrap_or_default());
    eprintln!("Open it in a browser on this computer within five minutes; it pairs one client, once.");
    if let Some(device) = made["device"].as_str() {
        eprintln!("\nOr, on another device, scan this or open the link under it:\n");
        if let Some(drawn) = crate::qr::drawn(device) {
            eprint!("{drawn}");
        }
        println!("{device}");
    }
    Ok(())
}

/// The sessions and the links, as text or as JSON; or one of them revoked.
async fn access(socket: &std::path::Path, command: Option<AccessCommand>, json: bool) -> anyhow::Result<()> {
    if let Some(AccessCommand::Revoke { id }) = command {
        let revoked = control::ask(socket, Method::DELETE, &format!("/access/{id}"), None).await?;
        println!("revoked the {} {id}", revoked["revoked"].as_str().unwrap_or("?"));
        return Ok(());
    }
    let listed = control::ask(socket, Method::GET, "/access", None).await?;
    if json {
        println!("{listed:#}");
        return Ok(());
    }
    for (what, items) in [("session", &listed["sessions"]), ("link", &listed["links"])] {
        for item in items.as_array().into_iter().flatten() {
            let label = item["label"].as_str().unwrap_or("-");
            let holder = item["holder"].as_str().unwrap_or("-");
            println!("{what:7}  {}  {holder:6}  {label}", item["id"].as_str().unwrap_or("?"));
        }
    }
    Ok(())
}
