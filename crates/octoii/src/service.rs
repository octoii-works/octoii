//! `octoii service`: the node kept running as the person's own service, as their system keeps one — a launchd agent
//! on macOS, a systemd user unit on Linux — started at login, started again where it ends, its output in the Home's
//! `var/log/serve.log`, and its `PATH` the one it was installed from, so the programs it runs are found as the person
//! finds them.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context as _, bail};

use crate::home::Home;

/// The service's name, as the system knows it.
const LABEL: &str = "com.octoii.serve";
const UNIT: &str = "octoii.service";

/// What the person asks of the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Install,
    Uninstall,
    Start,
    Stop,
    Restart,
}

/// Does what was asked of the service of the node of `home`.
///
/// # Errors
/// Where the system's service manager refused, or this system has none the node knows.
pub fn service(home: &Home, asked: Asked) -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        launchd(home, asked)
    } else if cfg!(target_os = "linux") {
        systemd(home, asked)
    } else {
        bail!("no service manager the node knows on this system; run `octoii serve` as the system runs its programs")
    }
}

/// The program as the service runs it: this one, serving the Home `home`.
fn arguments(home: &Home) -> anyhow::Result<Vec<String>> {
    let program = std::env::current_exe().context("the program's path")?;
    let home = home.root().to_string_lossy().into_owned();
    Ok(vec![program.to_string_lossy().into_owned(), "--home".into(), home, "serve".into()])
}

/// Runs `program` with `arguments`, its failure told with what it wrote.
fn run(program: &str, arguments: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(program).args(arguments).output().with_context(|| program.to_owned())?;
    if !output.status.success() {
        bail!(
            "{program} {}: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// A text escaped for a property list's string.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn launchd(home: &Home, asked: Asked) -> anyhow::Result<()> {
    let agents = std::env::home_dir().context("the home folder")?.join("Library/LaunchAgents");
    let plist = agents.join(format!("{LABEL}.plist"));
    let domain = format!("gui/{}", rustix::process::getuid().as_raw());
    let target = format!("{domain}/{LABEL}");
    let plist_path = plist.to_string_lossy().into_owned();
    match asked {
        Asked::Install => {
            std::fs::create_dir_all(&agents)?;
            let program = arguments(home)?.iter().fold(String::new(), |mut program, argument| {
                let _ = write!(program, "\n    <string>{}</string>", escaped(argument));
                program
            });
            let log = escaped(&home.log().join("serve.log").to_string_lossy());
            let path = escaped(&std::env::var("PATH").unwrap_or_default());
            std::fs::write(
                &plist,
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>{LABEL}</string>\n  <key>ProgramArguments</key>\n  <array>{program}\n  </array>\n  <key>EnvironmentVariables</key>\n  <dict>\n    <key>PATH</key>\n    <string>{path}</string>\n  </dict>\n  <key>RunAtLoad</key>\n  <true/>\n  <key>KeepAlive</key>\n  <true/>\n  <key>StandardOutPath</key>\n  <string>{log}</string>\n  <key>StandardErrorPath</key>\n  <string>{log}</string>\n</dict>\n</plist>\n"
                ),
            )?;
            run("launchctl", &["bootstrap", &domain, &plist_path])
        }
        Asked::Uninstall => {
            let _ = run("launchctl", &["bootout", &target]);
            std::fs::remove_file(&plist).or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })?;
            Ok(())
        }
        Asked::Start => run("launchctl", &["bootstrap", &domain, &plist_path]),
        Asked::Stop => run("launchctl", &["bootout", &target]),
        Asked::Restart => run("launchctl", &["kickstart", "-k", &target]),
    }
}

/// The folder of the person's systemd units.
fn units() -> anyhow::Result<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".config")))
        .context("the configuration folder")?;
    Ok(config.join("systemd/user"))
}

fn systemd(home: &Home, asked: Asked) -> anyhow::Result<()> {
    let units = units()?;
    let unit = units.join(UNIT);
    match asked {
        Asked::Install => {
            std::fs::create_dir_all(&units)?;
            let program = arguments(home)?
                .iter()
                .map(|argument| format!("\"{}\"", argument.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(" ");
            let path = std::env::var("PATH").unwrap_or_default();
            let log = home.log().join("serve.log");
            std::fs::write(
                &unit,
                format!(
                    "[Unit]\nDescription=Octoii\n\n[Service]\nExecStart={program}\nEnvironment=\"PATH={path}\"\nRestart=always\nStandardOutput=append:{}\nStandardError=append:{}\n\n[Install]\nWantedBy=default.target\n",
                    log.display(),
                    log.display()
                ),
            )?;
            run("systemctl", &["--user", "daemon-reload"])?;
            run("systemctl", &["--user", "enable", "--now", UNIT])
        }
        Asked::Uninstall => {
            let _ = run("systemctl", &["--user", "disable", "--now", UNIT]);
            std::fs::remove_file(&unit).or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })?;
            run("systemctl", &["--user", "daemon-reload"])
        }
        Asked::Start => run("systemctl", &["--user", "start", UNIT]),
        Asked::Stop => run("systemctl", &["--user", "stop", UNIT]),
        Asked::Restart => run("systemctl", &["--user", "restart", UNIT]),
    }
}
