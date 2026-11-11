//! The files of a machine as the tools and a person's view read and change them, through its namespace: a page of a
//! text file, read a range at a time till the page is whole; a page of a folder's entries, folders first; an image or a
//! PDF as it is, told by its first bytes as file(1) tells one; a name that is not there found by its canonical form,
//! its spaces and apostrophes as a person types them; a file written, the folders on its way made, as it held its
//! text; and an edit of a text file.

use std::io::{self, BufReader};

use beak::{Dirent, FileKind, Namespace, Range, Status};
use bytes::Bytes;
use siphon_text::{Bounds, Content, Diff, Edit, Edited, Page, Refusal, Text};

use crate::machine::Machine;

/// The bytes a read asks of the namespace at once.
const READ_BYTES: usize = 256 << 10;

/// How a read goes: the page of a text file's lines, or of a folder's entries, it gives; and how many bytes an image or
/// a PDF may take, read whole.
#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    pub bounds: Bounds,
    pub media_bytes_max: u64,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            media_bytes_max: 32 << 20,
        }
    }
}

/// What is at a path: the namespace's path of it, its size, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub path: String,
    pub size: u64,
    pub is: Is,
}

/// What a name is: a folder, an image or a PDF of its media type, or any other file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Is {
    Folder,
    Media(&'static str),
    File,
}

/// What a read gives, each by the namespace's path read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    /// A page of a text file.
    Text { path: String, size: u64, page: Page },
    /// A page of a folder's entries, from the entry `from`, from 1, of `total`.
    Directory {
        path: String,
        entries: Vec<Dirent>,
        from: usize,
        total: usize,
    },
    /// An image or a PDF as it is.
    Media {
        path: String,
        media_type: &'static str,
        bytes: Bytes,
    },
    /// A file that is no text.
    Binary { path: String, size: u64 },
    /// An image or a PDF past the bytes a read takes.
    TooLarge { path: String, size: u64, limit: u64 },
}

/// A change a write or an edit made to a text file: whether it made the file, and its diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub created: bool,
    pub diff: Diff,
}

/// What an edit came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOutcome {
    Made { change: Change, edited: Edited },
    Refused(Refusal),
}

/// Why a file could not be read, written or edited as asked.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("nothing is there")]
    NotFound,
    #[error("it is a folder")]
    Directory,
    #[error("it holds no text")]
    NotText,
    #[error(transparent)]
    Beak(#[from] beak::Error),
}

fn not_found(error: beak::Error) -> Error {
    if error.status == Status::NotFound {
        Error::NotFound
    } else {
        Error::Beak(error)
    }
}

/// What a file is, by its first bytes: a PNG, a JPEG, a GIF, a WebP or a PDF; none for any other.
#[must_use]
pub fn media_type(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if head.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("image/webp")
    } else if head.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else {
        None
    }
}

/// A name's canonical form: its special spaces as plain ones, its curly apostrophes as `'`.
fn canonical(name: &str) -> String {
    name.chars()
        .map(|char| match char {
            '\u{00a0}' | '\u{2007}' | '\u{202f}' | '\u{2009}' => ' ',
            '\u{2018}' | '\u{2019}' => '\'',
            other => other,
        })
        .collect()
}

/// The folder `path` is in, and its name.
fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// The bytes of a file through a namespace, as a reader takes them, a range at a time.
struct Reading<'a> {
    namespace: &'a Namespace,
    path: &'a str,
    offset: u64,
    failed: Option<beak::Error>,
}

impl io::Read for Reading<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let want = out.len().min(READ_BYTES) as u64;
        let bytes = self.namespace.read(self.path, self.offset, want).map_err(|error| {
            self.failed = Some(error.clone());
            io::Error::other(error)
        })?;
        out[..bytes.len()].copy_from_slice(&bytes);
        self.offset += bytes.len() as u64;
        Ok(bytes.len())
    }
}

impl Machine {
    /// The namespace's `path` where something is there, else the path of a name in its folder whose canonical form is
    /// its own, as a person types a name the system holds with other spaces; none where neither is.
    fn found(&self, path: &str) -> Result<Option<(String, beak::Stat)>, Error> {
        if let Some(stat) = self.namespace.stat(path)? {
            return Ok(Some((path.to_owned(), stat)));
        }
        let (folder, name) = split(path);
        let wanted = canonical(name);
        let Ok(entries) = self.namespace.list(folder, &all()) else {
            return Ok(None);
        };
        for entry in entries {
            let entry_name = String::from_utf8_lossy(&entry.name);
            if canonical(&entry_name) == wanted {
                let other = if folder.is_empty() {
                    entry_name.into_owned()
                } else {
                    format!("{folder}/{entry_name}")
                };
                if let Some(stat) = self.namespace.stat(&other)? {
                    return Ok(Some((other, stat)));
                }
            }
        }
        Ok(None)
    }

    /// What is at the namespace's `path`, as `file(1)` tells it: its path, its size, and whether it is a folder, an
    /// image or a PDF — by its first bytes — or any other file.
    ///
    /// # Errors
    /// `NotFound` where neither it nor its canonical form is there; the namespace's failure.
    pub fn stat(&self, path: &str) -> Result<Stat, Error> {
        let (path, stat) = self.found(path)?.ok_or(Error::NotFound)?;
        let is = if stat.kind == FileKind::Directory {
            Is::Folder
        } else {
            media_type(&self.namespace.read(&path, 0, 16)?).map_or(Is::File, Is::Media)
        };
        Ok(Stat { path, size: stat.size, is })
    }

    /// The file or folder at the namespace's `path`, read as `options` say.
    ///
    /// # Errors
    /// `NotFound` where neither it nor its canonical form is there; the namespace's failure.
    pub fn read(&self, path: &str, options: &ReadOptions) -> Result<Read, Error> {
        let (path, stat) = self.found(path)?.ok_or(Error::NotFound)?;
        if stat.kind == FileKind::Directory {
            return self.folder_page(path, &options.bounds);
        }
        let head = self.namespace.read(&path, 0, 16)?;
        if let Some(media_type) = media_type(&head) {
            if stat.size > options.media_bytes_max {
                return Ok(Read::TooLarge {
                    path,
                    size: stat.size,
                    limit: options.media_bytes_max,
                });
            }
            let bytes = self.namespace.read(&path, 0, stat.size)?;
            return Ok(Read::Media { path, media_type, bytes });
        }
        let mut reading = Reading {
            namespace: &self.namespace,
            path: &path,
            offset: 0,
            failed: None,
        };
        let content = Page::read(BufReader::with_capacity(READ_BYTES, &mut reading), &options.bounds);
        match content {
            Ok(Content::Text(page)) => Ok(Read::Text {
                path,
                size: stat.size,
                page,
            }),
            Ok(Content::Binary) => Ok(Read::Binary { path, size: stat.size }),
            Err(error) => Err(reading.failed.map_or_else(|| Error::Beak(io_failure(&error)), Error::Beak)),
        }
    }

    /// A page of the entries of the folder at `path`, folders first, each in order by name.
    fn folder_page(&self, path: String, bounds: &Bounds) -> Result<Read, Error> {
        let mut entries = self.entries(&path)?;
        entries.sort_by(|a, b| (a.kind != FileKind::Directory, &a.name).cmp(&(b.kind != FileKind::Directory, &b.name)));
        let total = entries.len();
        // From the entry `offset`, from 1, as a page of lines is.
        let from = bounds.offset.max(1).min(total + 1);
        let entries = entries.into_iter().skip(from - 1).take(bounds.limit).collect();
        Ok(Read::Directory {
            path,
            entries,
            from,
            total,
        })
    }

    /// Every entry of the folder at `path`, in order by name.
    ///
    /// # Errors
    /// `NotFound` where nothing lists there; the namespace's failure.
    pub fn entries(&self, path: &str) -> Result<Vec<Dirent>, Error> {
        self.namespace.list(path, &all()).map_err(not_found)
    }

    /// The text of the file at `path`, as the tools that change one read it; none where nothing is there.
    fn text_of(&self, path: &str) -> Result<Option<(String, Bytes)>, Error> {
        let Some((path, stat)) = self.found(path)? else { return Ok(None) };
        if stat.kind == FileKind::Directory {
            return Err(Error::Directory);
        }
        let bytes = self.namespace.read(&path, 0, stat.size)?;
        Ok(Some((path, bytes)))
    }

    /// Writes `content` as the file at `path`, the folders on its way made: a text file it replaces keeps its
    /// byte-order mark, and `content`'s lines end `\r\n` where the file's first line did; a file that was no text is
    /// replaced as given. Its diff names it `name`.
    ///
    /// # Errors
    /// `Directory` where a folder is there; the namespace's failure.
    pub fn write(&self, path: &str, content: &str, name: &str) -> Result<Change, Error> {
        let before = self.text_of(path)?;
        let (path, old) = match &before {
            Some((found, bytes)) => (found.clone(), Text::of(bytes)),
            None => (path.to_owned(), None),
        };
        let (bytes, written) = match &old {
            Some(text) => {
                let written = text.with_line_endings(content);
                (text.encode(&written), written.into_owned())
            }
            None => (content.as_bytes().to_vec(), content.to_owned()),
        };
        self.make_folders(split(&path).0)?;
        self.namespace.write(&path, &bytes)?;
        Ok(Change {
            created: before.is_none(),
            diff: Diff::between(old.as_ref().map(|text| text.text), &written, name),
        })
    }

    /// Edits the text file at `path` as `edit` asks, its byte-order mark and line endings kept; nothing written where
    /// it is refused. Its diff names it `name`.
    ///
    /// # Errors
    /// `NotFound`, `Directory`, `NotText`; the namespace's failure.
    pub fn edit(&self, path: &str, edit: &Edit<'_>, name: &str) -> Result<EditOutcome, Error> {
        let (path, bytes) = self.text_of(path)?.ok_or(Error::NotFound)?;
        let text = Text::of(&bytes).ok_or(Error::NotText)?;
        let edited = match edit.apply(text.text) {
            Ok(edited) => edited,
            Err(refusal) => return Ok(EditOutcome::Refused(refusal)),
        };
        self.namespace.write(&path, &text.encode(&edited.text))?;
        Ok(EditOutcome::Made {
            change: Change {
                created: false,
                diff: Diff::between(Some(text.text), &edited.text, name),
            },
            edited,
        })
    }

    /// Makes the folder at `path` and each on its way to it, those there left as they are.
    fn make_folders(&self, path: &str) -> Result<(), Error> {
        let mut made = String::new();
        for name in path.split('/').filter(|name| !name.is_empty()) {
            if !made.is_empty() {
                made.push('/');
            }
            made.push_str(name);
            if self.namespace.stat(&made)?.is_none() {
                self.namespace.mkdir(&made)?;
            }
        }
        Ok(())
    }
}

/// A range that takes every entry.
fn all() -> Range<'static> {
    Range {
        limit: usize::MAX,
        ..Range::default()
    }
}

/// An I/O failure as Beak's.
fn io_failure(error: &io::Error) -> beak::Error {
    beak::Error {
        status: Status::Failed,
        reason: error.to_string(),
    }
}
