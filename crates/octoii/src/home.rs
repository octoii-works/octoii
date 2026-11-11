//! The node's Home, laid out as a system's folders are: `etc/` the person's own configuration, for every project —
//! their instructions, `AGENTS.md`, their skills, `skills/`, their MCP servers, `mcp.json` — and the providers'
//! credentials, `etc/credstore/`; `var/lib/` the node's state — the volume, the key its tunnel sessions show, the
//! browser's profile; `var/cache/` what is fetched again at need; `var/log/` its logs; `run/` what lasts while it runs,
//! its control socket; and `threads/`, where a thread's calls read the threads. The Home is the person's alone.

use std::path::{Path, PathBuf};

/// The node's Home, by its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// The Home at `root`.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The person's Home where none is named: `.octoii` in their home folder.
    #[must_use]
    pub fn of_person() -> Option<Self> {
        std::env::home_dir().map(|home| Self::at(home.join(".octoii")))
    }

    /// Makes the Home's folders where they are not, the Home readable by the person alone.
    ///
    /// # Errors
    /// The system's failure.
    pub fn prepare(&self) -> std::io::Result<()> {
        for folder in [
            self.credstore(),
            self.skills(),
            self.volume(),
            self.browser(),
            self.cache(),
            self.log(),
            self.run(),
            self.threads(),
        ] {
            std::fs::create_dir_all(folder)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The person's own configuration, for every project.
    #[must_use]
    pub fn config(&self) -> PathBuf {
        self.root.join("etc")
    }

    /// The person's skills.
    #[must_use]
    pub fn skills(&self) -> PathBuf {
        self.config().join("skills")
    }

    /// The providers' credentials, a file each.
    #[must_use]
    pub fn credstore(&self) -> PathBuf {
        self.config().join("credstore")
    }

    /// The node's state.
    #[must_use]
    pub fn state(&self) -> PathBuf {
        self.root.join("var/lib")
    }

    /// The volume Siphon keeps.
    #[must_use]
    pub fn volume(&self) -> PathBuf {
        self.state().join("volume")
    }

    /// The key the node's tunnel sessions show.
    #[must_use]
    pub fn key(&self) -> PathBuf {
        self.state().join("key")
    }

    /// The browser's profile, and where its pages' downloads go.
    #[must_use]
    pub fn browser(&self) -> PathBuf {
        self.state().join("browser")
    }

    #[must_use]
    pub fn cache(&self) -> PathBuf {
        self.root.join("var/cache")
    }

    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.root.join("var/log")
    }

    /// What lasts while the node runs.
    #[must_use]
    pub fn run(&self) -> PathBuf {
        self.root.join("run")
    }

    /// The control socket the node's commands reach it by.
    #[must_use]
    pub fn socket(&self) -> PathBuf {
        self.run().join("octoii.sock")
    }

    /// Where a thread's calls read the threads.
    #[must_use]
    pub fn threads(&self) -> PathBuf {
        self.root.join("threads")
    }
}
