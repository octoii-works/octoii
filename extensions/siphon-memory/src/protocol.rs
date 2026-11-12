//! Memory's methods, as its extension adds them to the protocol: the memory of the project a folder is in, listed; and a
//! memory of it deleted.

use serde::{Deserialize, Serialize};
use siphon::protocol::siphon::InFolder;
use siphon::protocol::{Nothing, Scope};
use ts_rs::TS;

siphon::methods! {
    /// The memory of the project a folder is in: where it is kept, and each memory.
    "memory.list" ListMemory(InFolder) -> MemoryListed, Some(Scope::ThreadsRead);
    /// A memory of the project a folder is in deleted: its file, and its line of the index.
    "memory.delete" DeleteMemory(DeleteMemory) -> Nothing, Some(Scope::ThreadsOperate);
}

/// The memory of a project: the folder it is kept in, none where the machine keeps none; and each memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MemoryListed {
    pub folder: Option<String>,
    pub memories: Vec<crate::memory::Memory>,
}

/// A memory of the project the folder `path` is in, by its file's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeleteMemory {
    pub path: String,
    pub file: String,
}
