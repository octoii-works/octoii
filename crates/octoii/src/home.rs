//! The node's Home, laid out as a system's folders are: `etc/` the person's own configuration, for every project —
//! their instructions, `AGENTS.md`, their skills, `skills/`, their MCP servers, `mcp.json` — and the providers'
//! credentials, `etc/credstore/`; `var/lib/` the node's state — the volume, the key its tunnel sessions show, the
//! address its account gave the machine, the browser's profile; `var/cache/` what is fetched again at need; `var/log/` its logs; and `threads/`, where a thread's
//! calls read the threads. The Home is the person's alone. Its control socket is kept in the person's runtime folder,
//! as tmux keeps its sockets, since a socket's path is bounded and a Home may lie anywhere.

use std::path::{Path, PathBuf};

use sha2::Digest as _;

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

    /// The address the machine's account gave it, where it has one: the relay devices reach it through.
    #[must_use]
    pub fn address(&self) -> PathBuf {
        self.state().join("address")
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

    /// The control socket the node's commands reach it by: in the person's runtime folder — `$XDG_RUNTIME_DIR/octoii`,
    /// else `octoii-<uid>` in the system's temporary folder — named by the Home it serves.
    ///
    /// # Errors
    /// Where the folder cannot be made, or is not the person's alone — another may have made it to listen in their
    /// place.
    pub fn socket(&self) -> std::io::Result<PathBuf> {
        let uid = rustix::process::getuid().as_raw();
        let folder = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
            || std::env::temp_dir().join(format!("octoii-{uid}")),
            |runtime| PathBuf::from(runtime).join("octoii"),
        );
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&folder)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let found = std::fs::symlink_metadata(&folder)?;
            if !found.is_dir() || found.uid() != uid || found.mode() & 0o077 != 0 {
                let why = format!("{} is not a folder of this person's alone", folder.display());
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, why));
            }
        }
        let root = std::fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let digest = sha2::Sha256::digest(root.as_os_str().as_encoded_bytes());
        let name = u64::from_be_bytes(digest[..8].try_into().unwrap_or_default());
        Ok(folder.join(format!("{name:016x}.sock")))
    }

    /// Where a thread's calls read the threads.
    #[must_use]
    pub fn threads(&self) -> PathBuf {
        self.root.join("threads")
    }
}
