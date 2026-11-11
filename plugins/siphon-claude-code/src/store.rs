//! What an account's Claude Code keeps between the node's runs, each a file of its folder: where each prefix of a
//! conversation stands in a session — a cache only, as a replica that lost its offset syncs in full — which session
//! each conversation holds, and the windows Claude Code gives its models. A file of lines is appended to as each
//! changes and written again whole, by a file of its own put in its place, once it holds twice what it keeps.

use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where a prefix of a conversation ends in a session: the session, and the entry of its transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub session: String,
    pub entry: String,
}

/// A file of JSON lines.
#[derive(Debug)]
struct Lines {
    path: PathBuf,
    count: usize,
}

impl Lines {
    /// The file at `path` and the values of its lines, those that are `T`; none where there is no file yet.
    fn open<T: for<'de> Deserialize<'de>>(path: PathBuf) -> std::io::Result<(Self, Vec<T>)> {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        let values: Vec<T> = text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
        let count = text.lines().count();
        Ok((Self { path, count }, values))
    }

    fn append(&mut self, value: &impl Serialize) -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        let mut line = serde_json::to_string(value)?;
        line.push('\n');
        file.write_all(line.as_bytes())?;
        self.count += 1;
        Ok(())
    }

    fn rewrite<T: Serialize>(&mut self, values: impl Iterator<Item = T>) -> std::io::Result<()> {
        let mut text = String::new();
        let mut count = 0;
        for value in values {
            text.push_str(&serde_json::to_string(&value)?);
            text.push('\n');
            count += 1;
        }
        write_whole(&self.path, text.as_bytes())?;
        self.count = count;
        Ok(())
    }
}

/// Writes `bytes` to `path` whole: to a file of its own, then put in its place.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let fresh = path.with_extension("new");
    std::fs::write(&fresh, bytes)?;
    std::fs::rename(fresh, path)
}

/// How many positions are kept: past it, the oldest go.
const POSITIONS_MAX: usize = 20_000;

#[derive(Serialize, Deserialize)]
struct PositionLine {
    hash: String,
    session: String,
    entry: String,
}

/// The positions known, by the hash of the prefix each ends. Each session's tip is where it was last written to,
/// which a conversation goes on from in place; from any earlier, the session is forked, as a conversation that parted
/// from it there is another.
#[derive(Debug)]
pub struct Positions {
    file: Lines,
    known: HashMap<String, (Position, u64)>,
    order: BTreeMap<u64, String>,
    tips: HashMap<String, String>,
    next: u64,
}

impl Positions {
    /// The positions the file at `path` keeps.
    ///
    /// # Errors
    /// Where it cannot be read.
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        let (file, lines) = Lines::open::<PositionLine>(path)?;
        let mut positions = Self {
            file,
            known: HashMap::new(),
            order: BTreeMap::new(),
            tips: HashMap::new(),
            next: 0,
        };
        for line in lines {
            positions.put(
                line.hash,
                Position {
                    session: line.session,
                    entry: line.entry,
                },
            );
        }
        Ok(positions)
    }

    /// Where the prefix of hash `hash` ends; none where no session holds it.
    #[must_use]
    pub fn get(&self, hash: &str) -> Option<&Position> {
        self.known.get(hash).map(|(position, _)| position)
    }

    /// Whether the prefix of hash `hash` ends at its session's tip.
    #[must_use]
    pub fn is_tip(&self, hash: &str) -> bool {
        self.get(hash)
            .is_some_and(|position| self.tips.get(&position.session).is_some_and(|tip| tip == hash))
    }

    fn put(&mut self, hash: String, position: Position) {
        if let Some((_, at)) = self.known.remove(&hash) {
            self.order.remove(&at);
        }
        self.next += 1;
        self.order.insert(self.next, hash.clone());
        self.tips.insert(position.session.clone(), hash.clone());
        self.known.insert(hash, (position, self.next));
        while self.known.len() > POSITIONS_MAX {
            let Some((_, oldest)) = self.order.pop_first() else { break };
            self.known.remove(&oldest);
        }
    }

    /// Keeps that the prefix of hash `hash` ends at `position`.
    ///
    /// # Errors
    /// Where the file cannot be written.
    pub fn set(&mut self, hash: String, position: Position) -> std::io::Result<()> {
        let line = PositionLine {
            hash: hash.clone(),
            session: position.session.clone(),
            entry: position.entry.clone(),
        };
        self.put(hash, position);
        if self.file.count >= 2 * POSITIONS_MAX {
            return self.rewrite();
        }
        self.file.append(&line)
    }

    /// Forgets every position in the session `session`, which is gone.
    ///
    /// # Errors
    /// Where the file cannot be written.
    pub fn forget(&mut self, session: &str) -> std::io::Result<()> {
        let before = self.known.len();
        let order = &mut self.order;
        self.known.retain(|_, (position, at)| {
            let kept = position.session != session;
            if !kept {
                order.remove(at);
            }
            kept
        });
        self.tips.remove(session);
        if self.known.len() == before {
            return Ok(());
        }
        self.rewrite()
    }

    fn rewrite(&mut self) -> std::io::Result<()> {
        let known = &self.known;
        let lines = self.order.values().filter_map(|hash| {
            let (position, _) = known.get(hash)?;
            Some(PositionLine {
                hash: hash.clone(),
                session: position.session.clone(),
                entry: position.entry.clone(),
            })
        });
        self.file.rewrite(lines)
    }
}

/// How many lines the holds' file may hold past twice what it keeps before it is written again.
const SLACK_LINES: usize = 64;

#[derive(Serialize, Deserialize)]
struct HoldLine {
    conversation: String,
    session: Option<String>,
}

/// Which session each conversation holds — the one its last answer settled in. A session no conversation holds is let
/// go of, as a file no name links is removed.
#[derive(Debug)]
pub struct Holds {
    file: Lines,
    held: HashMap<String, String>,
    holders: HashMap<String, usize>,
}

impl Holds {
    /// The holds the file at `path` keeps.
    ///
    /// # Errors
    /// Where it cannot be read.
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        let (file, lines) = Lines::open::<HoldLine>(path)?;
        let mut holds = Self {
            file,
            held: HashMap::new(),
            holders: HashMap::new(),
        };
        for line in lines {
            holds.put(line.conversation, line.session);
        }
        Ok(holds)
    }

    /// Whether a conversation holds `session`.
    #[must_use]
    pub fn is_held(&self, session: &str) -> bool {
        self.holders.contains_key(session)
    }

    /// Keeps that `conversation` holds `session`: the session it held before, where no conversation holds that now.
    ///
    /// # Errors
    /// Where the file cannot be written.
    pub fn hold(&mut self, conversation: &str, session: &str) -> std::io::Result<Option<String>> {
        let before = self.held.get(conversation).cloned();
        if before.as_deref() == Some(session) {
            return Ok(None);
        }
        self.change(conversation, Some(session.to_owned()))?;
        Ok(before.filter(|before| !self.is_held(before)))
    }

    /// Lets `conversation` go, as it ended: the session it held, where no conversation holds that now.
    ///
    /// # Errors
    /// Where the file cannot be written.
    pub fn release(&mut self, conversation: &str) -> std::io::Result<Option<String>> {
        let Some(before) = self.held.get(conversation).cloned() else {
            return Ok(None);
        };
        self.change(conversation, None)?;
        Ok((!self.is_held(&before)).then_some(before))
    }

    /// Lets each conversation that holds `session` go of it, as the session is gone.
    ///
    /// # Errors
    /// Where the file cannot be written.
    pub fn drop_session(&mut self, session: &str) -> std::io::Result<()> {
        let holding: Vec<String> = self
            .held
            .iter()
            .filter(|(_, held)| *held == session)
            .map(|(conversation, _)| conversation.clone())
            .collect();
        for conversation in holding {
            self.change(&conversation, None)?;
        }
        Ok(())
    }

    fn change(&mut self, conversation: &str, session: Option<String>) -> std::io::Result<()> {
        self.put(conversation.to_owned(), session.clone());
        if self.file.count < 2 * self.held.len() + SLACK_LINES {
            return self.file.append(&HoldLine {
                conversation: conversation.to_owned(),
                session,
            });
        }
        let lines = self.held.iter().map(|(conversation, session)| HoldLine {
            conversation: conversation.clone(),
            session: Some(session.clone()),
        });
        self.file.rewrite(lines)
    }

    fn put(&mut self, conversation: String, session: Option<String>) {
        if let Some(before) = self.held.remove(&conversation) {
            match self.holders.get_mut(&before) {
                Some(count) if *count > 1 => *count -= 1,
                _ => {
                    self.holders.remove(&before);
                }
            }
        }
        if let Some(session) = session {
            *self.holders.entry(session.clone()).or_default() += 1;
            self.held.insert(conversation, session);
        }
    }
}

/// The windows Claude Code gives its models, by each model's name as it takes it, and the version of Claude Code that
/// told them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Windows {
    pub version: String,
    pub windows: HashMap<String, u64>,
}

impl Windows {
    /// The windows the file at `path` keeps; none where it keeps none it can read.
    #[must_use]
    pub fn read(path: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    /// Keeps the windows in the file at `path`.
    ///
    /// # Errors
    /// Where it cannot be written.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        write_whole(path, &serde_json::to_vec(self)?)
    }
}
