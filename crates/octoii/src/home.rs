//! The node's Home, laid out as a system's folders are: `etc/` the person's own configuration, for every project —
//! their instructions, `AGENTS.md`, their skills, `skills/`, their MCP servers, `mcp.json`, their memory of each
//! project, `memory/` — and the providers' credentials, `etc/credstore/`; `var/lib/` the node's state — the volume,
//! the key its tunnel sessions show, the address its account gave the machine, the browser's profile; `var/log/` its
//! logs; and `threads/`, where a thread's calls read the threads. The Home is the person's alone. Its control socket is
//! kept in the person's runtime folder, as tmux keeps its sockets, since a socket's path is bounded and a Home may lie
//! anywhere; what its work leaves for a while — the threads' scratchpads, the browser's downloads — in the system's
//! temporary folder, which the system clears.

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

    /// The person's memory, a folder for each project.
    #[must_use]
    pub fn memory(&self) -> PathBuf {
        self.config().join("memory")
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

    /// The browser's profile.
    #[must_use]
    pub fn browser(&self) -> PathBuf {
        self.state().join("browser")
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
        let folder = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
            || std::env::temp_dir().join(format!("octoii-{}", uid())),
            |runtime| PathBuf::from(runtime).join("octoii"),
        );
        private_folder(&folder)?;
        Ok(folder.join(format!("{}.sock", self.name())))
    }

    /// The Home's temporary folder, for what its work leaves for a while: `octoii-<uid>` in the system's temporary
    /// folder, a folder there named by the Home, made where it is not.
    ///
    /// # Errors
    /// Where the folder cannot be made, or is not the person's alone.
    pub fn temporary(&self) -> std::io::Result<PathBuf> {
        let folder = std::env::temp_dir().join(format!("octoii-{}", uid()));
        private_folder(&folder)?;
        let temporary = folder.join(self.name());
        private_folder(&temporary)?;
        Ok(temporary)
    }

    /// The Home's name among the person's Homes: of its path, however spelled, whether or not it is there yet.
    fn name(&self) -> String {
        let digest = sha2::Sha256::digest(real_path(&self.root).as_os_str().as_encoded_bytes());
        format!("{:016x}", u64::from_be_bytes(digest[..8].try_into().unwrap_or_default()))
    }

    /// Where a thread's calls read the threads.
    #[must_use]
    pub fn threads(&self) -> PathBuf {
        self.root.join("threads")
    }
}

/// The person's id on the system.
fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}

/// Makes `folder` where it is not, the person's alone.
///
/// # Errors
/// Where it cannot be made, or is not the person's alone — another may have made it to read or listen in their place.
fn private_folder(folder: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(folder)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let found = std::fs::symlink_metadata(folder)?;
        if !found.is_dir() || found.uid() != uid() || found.mode() & 0o077 != 0 {
            let why = format!("{} is not a folder of this person's alone", folder.display());
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, why));
        }
    }
    Ok(())
}

/// `path` with its links followed, as far as it is there, the rest as given: one Home names one socket whether or not
/// its folder was made yet, and however its path is spelled.
fn real_path(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut there = path;
    loop {
        if let Ok(real) = std::fs::canonicalize(there) {
            return rest.into_iter().rev().fold(real, |path, name| path.join(name));
        }
        match (there.parent(), there.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                there = parent;
            }
            _ => return path.to_owned(),
        }
    }
}
