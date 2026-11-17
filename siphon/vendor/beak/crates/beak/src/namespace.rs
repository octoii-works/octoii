//! A namespace: a table of mounts read and changed through as one file system from any thread, the access a command
//! started in it is held to, and what a kernel registers by a name a process names.

use beak_sys as sys;
use bytes::Bytes;

use crate::Result;
use crate::ffi::{check, copied, given, lent, object};
use crate::kernel::{Kernel, buffer};
use crate::snapshot::Range;

/// The namespace as Beak holds it.
struct Raw(*mut sys::beak_namespace);

// SAFETY: a namespace is read and changed from any thread at once, behind its own lock.
unsafe impl Send for Raw {}
// SAFETY: as above.
unsafe impl Sync for Raw {}

impl Drop for Raw {
    fn drop(&mut self) {
        // SAFETY: the namespace Beak gave, let go of once.
        unsafe { sys::beak_namespace_free(self.0) };
    }
}

/// A namespace.
pub struct Namespace(Raw);

impl std::fmt::Debug for Namespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Namespace")
    }
}

/// How a label's directories are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelView<'a> {
    OneValue(&'a [u8]),
    ByValue,
    AnyValue,
}

/// A file system to mount.
#[derive(Debug, Clone, Copy)]
pub enum FileSystem<'a> {
    /// The machine's directory at the absolute path.
    Host(&'a str),
    /// A place nothing reaches.
    Inaccessible,
    /// The directories of `kind` under `under` carrying `label`, laid out as `view` says, read alone.
    Label {
        kernel: &'a Kernel,
        kind: &'a str,
        label: &'a str,
        view: LabelView<'a>,
        under: &'a [u8],
    },
    /// The kernel's volume from `base` down, read alone.
    Tree { kernel: &'a Kernel, base: &'a [u8] },
}

impl FileSystem<'_> {
    fn lower(&self) -> sys::beak_file_system {
        use sys::beak_file_system__bindgen_ty_1 as Body;
        use sys::beak_file_system_Tag as Tag;
        let (tag, body) = match *self {
            FileSystem::Host(path) => (
                Tag::BEAK_FILE_SYSTEM_HOST,
                Body {
                    HOST: sys::beak_file_system_Host_Body {
                        path: lent(path.as_bytes()),
                    },
                },
            ),
            FileSystem::Inaccessible => (
                Tag::BEAK_FILE_SYSTEM_INACCESSIBLE,
                Body {
                    HOST: sys::beak_file_system_Host_Body { path: lent(b"") },
                },
            ),
            FileSystem::Label {
                kernel,
                kind,
                label,
                view,
                under,
            } => {
                let (view, value) = match view {
                    LabelView::OneValue(value) => (sys::beak_label_view::BEAK_LABEL_VIEW_ONE_VALUE, value),
                    LabelView::ByValue => (sys::beak_label_view::BEAK_LABEL_VIEW_BY_VALUE, &b""[..]),
                    LabelView::AnyValue => (sys::beak_label_view::BEAK_LABEL_VIEW_ANY_VALUE, &b""[..]),
                };
                (
                    Tag::BEAK_FILE_SYSTEM_LABEL,
                    Body {
                        LABEL: sys::beak_file_system_Label_Body {
                            kernel: kernel.raw(),
                            kind: lent(kind.as_bytes()),
                            label: lent(label.as_bytes()),
                            view,
                            value: lent(value),
                            under: lent(under),
                        },
                    },
                )
            }
            FileSystem::Tree { kernel, base } => (
                Tag::BEAK_FILE_SYSTEM_TREE,
                Body {
                    TREE: sys::beak_file_system_Tree_Body {
                        kernel: kernel.raw(),
                        base: lent(base),
                    },
                },
            ),
        };
        sys::beak_file_system {
            tag,
            __bindgen_anon_1: body,
        }
    }
}

/// What a name is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    File,
    Link,
}

fn file_kind(kind: sys::beak_file_kind) -> FileKind {
    match kind {
        sys::beak_file_kind::BEAK_FILE_KIND_DIRECTORY => FileKind::Directory,
        sys::beak_file_kind::BEAK_FILE_KIND_FILE => FileKind::File,
        sys::beak_file_kind::BEAK_FILE_KIND_LINK => FileKind::Link,
    }
}

/// What a name is: its kind, a file's length, whether it is executable, when it last changed, its file system's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub kind: FileKind,
    pub size: u64,
    pub executable: bool,
    pub modified_ms: i64,
    pub file_id: Option<u64>,
}

/// A name in a directory, and what it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirent {
    pub name: Bytes,
    pub kind: FileKind,
}

impl Namespace {
    /// A namespace of no mounts, reaching the network.
    ///
    /// # Errors
    /// None that a namespace gives.
    pub fn new() -> Result<Self> {
        // SAFETY: the call is given where to put the namespace.
        Ok(Self(Raw(object(|out| unsafe { sys::beak_namespace_open(out) })?)))
    }

    pub(crate) fn raw(&self) -> *const sys::beak_namespace {
        self.0.0
    }

    /// Mounts `file_system` at `at`, written through where `writable`; "" is the root.
    ///
    /// # Errors
    /// A directory that cannot be opened.
    pub fn mount(&self, at: &str, file_system: &FileSystem<'_>, writable: bool) -> Result<()> {
        let lowered = file_system.lower();
        // SAFETY: the call is given what it asks, alive for it.
        check(unsafe { sys::beak_namespace_mount(self.raw(), lent(at.as_bytes()), &raw const lowered, writable) })
    }

    /// Takes out what was mounted at `at`.
    ///
    /// # Errors
    /// `NotFound` where nothing was.
    pub fn unmount(&self, at: &str) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_unmount(self.raw(), lent(at.as_bytes())) })
    }

    /// Allows what works in it to reach the network, or not.
    ///
    /// # Errors
    /// None that a namespace gives.
    pub fn allow_network(&self, allowed: bool) -> Result<()> {
        // SAFETY: the namespace is open.
        check(unsafe { sys::beak_namespace_allow_network(self.raw(), allowed) })
    }

    /// Registers a copy of it with `kernel` as `name`, the commands of a process naming it to start in it.
    ///
    /// # Errors
    /// None that a kernel gives.
    pub fn register(&self, kernel: &Kernel, name: &[u8]) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_kernel_register_namespace(kernel.raw(), lent(name), self.raw()) })
    }

    /// What `path` names; `None` where nothing does.
    ///
    /// # Errors
    /// What the file system holding it refuses.
    pub fn stat(&self, path: &str) -> Result<Option<Stat>> {
        // SAFETY: as above.
        match given(|out: *mut sys::beak_stat| unsafe { sys::beak_namespace_stat(self.raw(), lent(path.as_bytes()), out) }) {
            Ok(stat) => Ok(Some(Stat {
                kind: file_kind(stat.kind),
                size: stat.size,
                executable: stat.executable,
                modified_ms: stat.modified_ms,
                file_id: stat.has_file_id.then_some(stat.file_id),
            })),
            Err(err) if err.status == crate::Status::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// The bytes of the file at `path` from `offset`, at most `length`.
    ///
    /// # Errors
    /// `NotFound` where there is no file; what the file system refuses.
    pub fn read(&self, path: &str, offset: u64, length: u64) -> Result<Bytes> {
        // SAFETY: as above.
        buffer(|out| unsafe { sys::beak_namespace_read(self.raw(), lent(path.as_bytes()), offset, length, out) })
    }

    /// The file at `path` given `bytes` whole.
    ///
    /// # Errors
    /// `ReadOnly` through a read-only mount; what the file system refuses.
    pub fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_write(self.raw(), lent(path.as_bytes()), lent(bytes)) })
    }

    /// `bytes` written over the file at `path` from `offset`.
    ///
    /// # Errors
    /// As `write`.
    pub fn write_at(&self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_write_at(self.raw(), lent(path.as_bytes()), offset, lent(bytes)) })
    }

    /// The file at `path` cut or grown to `length`.
    ///
    /// # Errors
    /// As `write`.
    pub fn truncate(&self, path: &str, length: u64) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_truncate(self.raw(), lent(path.as_bytes()), length) })
    }

    /// The file at `path` made executable or not.
    ///
    /// # Errors
    /// As `write`.
    pub fn chmod(&self, path: &str, executable: bool) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_chmod(self.raw(), lent(path.as_bytes()), executable) })
    }

    /// The directory at `path` made, and those on the way.
    ///
    /// # Errors
    /// As `write`.
    pub fn mkdir(&self, path: &str) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_mkdir(self.raw(), lent(path.as_bytes())) })
    }

    /// The name at `path` deleted, with everything under it.
    ///
    /// # Errors
    /// As `write`.
    pub fn delete(&self, path: &str) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_delete(self.raw(), lent(path.as_bytes())) })
    }

    /// What `from` names given the name `to`.
    ///
    /// # Errors
    /// As `write`.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_namespace_rename(self.raw(), lent(from.as_bytes()), lent(to.as_bytes())) })
    }

    /// A page of the directory at `path`.
    ///
    /// # Errors
    /// `NotFound` where nothing lists there; what the file system refuses.
    pub fn list(&self, path: &str, range: &Range<'_>) -> Result<Vec<Dirent>> {
        let lowered = range.lower();
        // SAFETY: the call is given what it asks; the names are read, then let go of once.
        let dirents = object(|out| unsafe { sys::beak_namespace_list(self.raw(), lent(path.as_bytes()), &raw const lowered, out) })?;
        // SAFETY: as above.
        unsafe {
            let items = if (*dirents).len == 0 {
                &[][..]
            } else {
                std::slice::from_raw_parts((*dirents).items, (*dirents).len)
            };
            let read = items
                .iter()
                .map(|dirent| Dirent {
                    name: copied(&dirent.name),
                    kind: file_kind(dirent.kind),
                })
                .collect();
            sys::beak_dirents_free(dirents);
            Ok(read)
        }
    }
}
