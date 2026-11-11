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
    let socket = home.socket();
    match command {
        Command::Serve { port, app } => {
            serve::serve(Served {
                home,
                port,
                app: app.or_else(installed_app),
            })
            .await
        }
        Command::Status { json } => {
            let status = control::ask(&socket, Method::GET, "/status", None).await?;
            if json {
                println!("{status:#}");
            } else {
                println!(
                    "running {} on {}",
                    status["version"].as_str().unwrap_or("?"),
                    status["machine"].as_str().unwrap_or("?")
                );
                println!("home      {}", status["home"].as_str().unwrap_or("?"));
                println!("loopback  {}", status["loopback"].as_str().unwrap_or("?"));
            }
            Ok(())
        }
        Command::Pair { label, scopes: named } => {
            let asked = Asked {
                label,
                scopes: scopes(named)?,
            };
            let made = control::ask(&socket, Method::POST, "/links", Some(json!(asked))).await?;
            println!("{}", made["url"].as_str().unwrap_or_default());
            eprintln!("Open it in a browser on this computer within five minutes; it pairs one browser, once.");
            Ok(())
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
        Command::Access {
            command: Some(AccessCommand::Revoke { id }),
            ..
        } => {
            let revoked = control::ask(&socket, Method::DELETE, &format!("/access/{id}"), None).await?;
            println!("revoked the {} {id}", revoked["revoked"].as_str().unwrap_or("?"));
            Ok(())
        }
        Command::Access { command: None, json } => {
            let listed = control::ask(&socket, Method::GET, "/access", None).await?;
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
        Command::Version => {
            println!("octoii {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
