//! Bounded pages of a thread's history from one fixed snapshot.

use super::log::{self, Cursor, Entry};
use super::thread;
use crate::volume::Volume;

/// A thread's history as it stood when opened. Pages remain consistent while new entries arrive.
#[derive(Debug)]
pub struct History {
    snapshot: std::sync::Arc<octoii_siphon_lib::OwnedSnapshot>,
    thread: String,
}

impl History {
    /// Opens a thread's history, or returns none if the thread does not exist.
    ///
    /// # Errors
    /// Storage failures.
    pub fn open(volume: &Volume, thread: &str) -> anyhow::Result<Option<Self>> {
        let snapshot = volume.snapshot()?;
        if snapshot.stat(&thread::path(thread))?.is_none() {
            return Ok(None);
        }
        Ok(Some(Self {
            snapshot: snapshot.into(),
            thread: thread.to_owned(),
        }))
    }

    /// The latest `limit` entries strictly before `entry`, or the newest page if none is given, oldest first.
    ///
    /// # Errors
    /// Storage failures or malformed log entries.
    pub fn before(&self, entry: Option<&str>, limit: usize) -> anyhow::Result<Vec<Entry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let (lines, _) = log::read_before(&self.snapshot, &self.thread, entry, limit)?;
        Ok(lines.iter().map(|line| log::entry_of(line)).collect::<Result<_, _>>()?)
    }

    /// The first `limit` entries strictly after `entry`, or from the beginning if none is given, oldest first.
    /// Reads in small batches, seeking to the segment that can contain the anchor first.
    ///
    /// # Errors
    /// Storage failures or malformed log entries.
    pub fn after(&self, entry: Option<&str>, limit: usize) -> anyhow::Result<Vec<Entry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let segments = log::segments(&self.snapshot, &self.thread)?;
        let start = entry.map_or(0, |entry| {
            segments
                .partition_point(|(name, _)| name.strip_suffix(log::SEGMENT_EXTENSION).unwrap_or(name) <= entry)
                .saturating_sub(1)
        });
        let mut cursor = segments.get(start).map(|(segment, _)| Cursor {
            segment: segment.clone(),
            offset: 0,
        });
        let mut entries = Vec::new();
        loop {
            let read = log::read_after(&self.snapshot, &self.thread, cursor.as_ref(), 64 << 10)?;
            for line in &read.lines {
                let item = log::entry_of(line)?;
                if entry.is_none_or(|entry| item.id() > entry) {
                    entries.push(item);
                    if entries.len() == limit {
                        return Ok(entries);
                    }
                }
            }
            if !read.more {
                return Ok(entries);
            }
            cursor = read.cursor;
        }
    }
}

/// Entries from newest to oldest, buffered a small part of one segment at a time.
#[derive(Debug)]
pub struct Entries {
    snapshot: std::sync::Arc<octoii_siphon_lib::OwnedSnapshot>,
    thread: String,
    segments: Vec<(String, u64)>,
    lines: Vec<bytes::Bytes>,
}

impl History {
    /// Scan from newest to oldest without rereading a segment for every result page.
    ///
    /// # Errors
    /// Storage failures while listing the segments. Iteration reports read or decoding failures.
    pub fn reverse(&self) -> anyhow::Result<Entries> {
        Ok(Entries {
            snapshot: std::sync::Arc::clone(&self.snapshot),
            thread: self.thread.clone(),
            segments: log::segments(&self.snapshot, &self.thread)?,
            lines: Vec::new(),
        })
    }
}

impl Entries {
    fn read_next(&mut self) -> anyhow::Result<Option<Entry>> {
        loop {
            if let Some(line) = self.lines.pop() {
                return Ok(Some(log::entry_of(&line)?));
            }
            let Some((segment, end)) = self.segments.last_mut() else {
                return Ok(None);
            };
            if *end == 0 {
                self.segments.pop();
                continue;
            }
            let mut budget = 64 << 10;
            loop {
                let start = end.saturating_sub(budget);
                let bytes = self
                    .snapshot
                    .read_range(&log::segment(&self.thread, segment), start, *end - start)?
                    .ok_or_else(|| anyhow::anyhow!("missing history segment {segment}"))?;
                let skip = if start == 0 {
                    Some(0)
                } else {
                    // The first line may start in the previous chunk. Leave it for that read.
                    bytes
                        .iter()
                        .position(|&byte| byte == b'\n')
                        .map(|at| at + 1)
                        .filter(|&at| at < bytes.len())
                };
                if let Some(skip) = skip {
                    self.lines = bytes[skip..]
                        .split(|&byte| byte == b'\n')
                        .filter(|line| !line.is_empty())
                        .map(|line| bytes.slice_ref(line))
                        .collect();
                    *end = start + skip as u64;
                    break;
                }
                // A single entry can exceed the usual batch size; read that entry whole.
                budget = budget.saturating_mul(2);
            }
        }
    }
}

impl Iterator for Entries {
    type Item = anyhow::Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_next() {
            Ok(entry) => entry.map(Ok),
            Err(error) => {
                self.segments.clear();
                self.lines.clear();
                Some(Err(error))
            }
        }
    }
}
