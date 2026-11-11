//! Siphon's own methods, events and topics: the threads — each made, changed, delivered to, stopped, deleted, answered
//! and read — and the settings; a subscription to a thread, to the list of threads, or to the settings.

use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_machine::git::{ChangedFile, Changes, ChangesScope, Commit, Comparison, Repository};
use siphon_settings::{Environment, SettingsChange};
use siphon_threads::log::Cursor;
use siphon_threads::{Config, ConfigChange, Entry, Listed, MarksChange, Status};
use ts_rs::TS;

use crate::{Nothing, Scope};

crate::methods! {
    events: SiphonEvent, topics: SiphonTopic;
    /// The threads, each as a list shows it.
    "threads.list" ListThreads(Nothing) -> ThreadsListed, Some(Scope::ThreadsRead);
    /// A thread made as `config` says, once for its idempotency key `key`: the id it has.
    "threads.create" CreateThread(CreateThread) -> Created, Some(Scope::ThreadsOperate);
    /// How a thread runs changed, from its next model request.
    "threads.configure" ConfigureThread(ConfigureThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// How the person keeps a thread changed.
    "threads.mark" MarkThread(MarkThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// What a message gives a thread delivered to its inbox, once for its idempotency key: its delivery's id.
    "threads.deliver" Deliver(Deliver) -> Delivered, Some(Scope::ThreadsOperate);
    /// A delivery taken back, where its thread has not taken it yet: whether it was.
    "threads.withdraw" Withdraw(Withdraw) -> Withdrawn, Some(Scope::ThreadsOperate);
    /// A thread's turn stopped.
    "threads.stop" StopThread(OfThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// A thread deleted, and its sub-threads with it.
    "threads.delete" DeleteThread(OfThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// What a thread asks the person answered.
    "threads.answer" AnswerThread(AnswerThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// A page of a thread's entries, the latest before an entry, or the latest of all.
    "threads.read" ReadThread(ReadThread) -> ThreadRead, Some(Scope::ThreadsRead);
    /// A page of a folder of the machine's: its entries in order by name, after the one named, each with whether git
    /// leaves it out.
    "files.list" ListFolder(ListFolder) -> FolderPage, Some(Scope::FilesRead);
    /// A file of the machine's: a page of its lines, where it is text; else what it is — an image or a PDF, whose
    /// bytes its HTTP gives, or any other.
    "files.read" ReadFile(ReadFile) -> FilePage, Some(Scope::FilesRead);
    /// The files under a folder of the machine's whose paths match a query, as a fuzzy finder matches them, the best
    /// first.
    "files.search" SearchFiles(SearchFiles) -> FoundPaths, Some(Scope::FilesRead);
    /// The repository a folder of the machine's is in, as git tells of it; none where it is in none.
    "git.repository" GitRepository(InFolder) -> InRepository, Some(Scope::FilesRead);
    /// The changes asked for of the repository a folder is in: each file with the lines it gained and lost.
    "git.changes" GitChanges(GitChanges) -> Changes, Some(Scope::FilesRead);
    /// A file's patch between two trees.
    "git.patch" GitPatch(GitPatch) -> Patch, Some(Scope::FilesRead);
    /// A file's two versions between two trees.
    "git.versions" GitVersions(GitVersions) -> Versions, Some(Scope::FilesRead);
    /// The commits of a branch since it parted from its base.
    "git.commits" GitCommits(GitCommits) -> Commits, Some(Scope::FilesRead);
    /// The providers, each as the person manages it.
    "providers.list" ListProviders(Nothing) -> ProvidersListed, Some(Scope::ProvidersManage);
    /// How a provider's account stands, as its driver tells it.
    "providers.status" StatusOfProvider(OfProvider) -> ProviderStatus, Some(Scope::ProvidersManage);
    /// A sign-in of a provider begun in a browser: where the person signs in.
    "providers.sign-in" SignIn(OfProvider) -> SignInBegun, Some(Scope::ProvidersManage);
    /// A sign-in finished from the address the person's browser was sent back to, as they paste it.
    "providers.finish-sign-in" FinishSignIn(FinishSignIn) -> Nothing, Some(Scope::ProvidersManage);
    /// A provider's API key set.
    "credentials.set" SetCredential(SetCredential) -> Nothing, Some(Scope::ProvidersManage);
    /// A provider's credential forgotten; one the host reads elsewhere, as from its environment, read as before.
    "credentials.remove" RemoveCredential(OfProvider) -> Nothing, Some(Scope::ProvidersManage);
    /// An account added of a provider, once for its idempotency key: its id, by which it is a provider.
    "accounts.add" AddAccount(AddAccount) -> AccountAdded, Some(Scope::ProvidersManage);
    "accounts.rename" RenameAccount(RenameAccount) -> Nothing, Some(Scope::ProvidersManage);
    /// An account removed, its credential with it, and what its provider's driver kept of it.
    "accounts.remove" RemoveAccount(OfAccount) -> Nothing, Some(Scope::ProvidersManage);
    /// The models a thread may run with, as the catalogue and the providers' listings describe them.
    "models.list" ListModels(Nothing) -> ModelsListed, Some(Scope::ThreadsRead);
    /// The providers' listings of their models fetched anew.
    "models.refresh" RefreshModels(Nothing) -> Nothing, Some(Scope::ProvidersManage);
    /// What each project was granted for good, of each granted anything.
    "grants.list" ListGrants(Nothing) -> GrantsListed, Some(Scope::ThreadsRead);
    /// What a project was granted, taken back in part.
    "grants.revoke" RevokeGrants(siphon_tools::projects::Granted) -> Nothing, Some(Scope::ThreadsOperate);
    /// The settings changed: the environment's each given, the device's merged.
    "settings.set" SetSettings(SettingsChange) -> Nothing, Some(Scope::ThreadsOperate);
}

/// A thread, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfThread {
    pub thread: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ThreadsListed {
    pub threads: Vec<Listed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateThread {
    pub config: Config,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Created {
    pub thread: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigureThread {
    pub thread: String,
    pub change: ConfigChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MarkThread {
    pub thread: String,
    pub change: MarksChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Deliver {
    pub thread: String,
    pub content: Vec<InputContent>,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Delivered {
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Withdraw {
    pub thread: String,
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Withdrawn {
    pub withdrawn: bool,
}

/// What a thread asks the person, by its request's key, answered with `answer`, which fits the schema the ask gives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AnswerThread {
    pub thread: String,
    pub key: String,
    pub answer: serde_json::Value,
}

/// A page of a thread's entries: at most `limit`, the latest before the entry `before`, or the latest of all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReadThread {
    pub thread: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<String>,
    pub limit: u32,
}

/// The entries read, oldest first, and whether they reach the thread's first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ThreadRead {
    pub entries: Vec<Entry>,
    pub complete: bool,
}

/// A page of a folder's entries: the folder, as the person names it, those after the entry `after` — from its first,
/// where none — `limit` at most.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListFolder {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub after: Option<String>,
    pub limit: u32,
}

/// What a name in a folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Directory,
    File,
    Link,
}

/// A name in a folder: what it names, and whether its repository's ignore rules leave it out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FolderEntry {
    pub name: String,
    pub kind: FileKind,
    pub ignored: bool,
}

/// A page of a folder's names: the folder, whole and clean; its names in order; whether more follow the last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FolderPage {
    pub path: String,
    pub entries: Vec<FolderEntry>,
    pub more: bool,
}

/// A file to read: as the person names it, its lines from `offset` — from 1, where none — `limit` at most.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReadFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// What a file holds, with the file, whole and clean, and its length in bytes: lines of text from the first given, and
/// whether more follow; an image or a PDF, by its media type, its bytes at the node's HTTP; or that it is no text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilePage {
    Text {
        path: String,
        size: u64,
        from: u32,
        lines: Vec<String>,
        more: bool,
    },
    Media {
        path: String,
        size: u64,
        media_type: String,
    },
    Binary {
        path: String,
        size: u64,
    },
}

/// Files to find: under the folder `path`, as the person names it, whose paths match `query`, `limit` at most.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SearchFiles {
    pub path: String,
    pub query: String,
    pub limit: u32,
}

/// The files found under a folder, by their paths from it, the best match first; and whether more matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FoundPaths {
    pub path: String,
    pub found: Vec<String>,
    pub truncated: bool,
}

/// A folder of the machine's, as the person names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InFolder {
    pub path: String,
}

/// The repository a folder is in; none where it is in none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InRepository {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repository: Option<Repository>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitChanges {
    pub path: String,
    pub scope: ChangesScope,
}

/// A file's patch to read: of the repository `path` is in, between `comparison`'s trees, with `context` lines about each
/// change — git's own number, where none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitPatch {
    pub path: String,
    pub comparison: Comparison,
    pub file: ChangedFile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub context: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Patch {
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitVersions {
    pub path: String,
    pub comparison: Comparison,
    pub file: ChangedFile,
}

/// A file's two versions: the older, none where it came; the newer, none where it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Versions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub old: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub new: Option<String>,
}

/// The commits to list: of the branch the folder `path` is on, since it parted from `base` — the repository's, where
/// none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitCommits {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<String>,
}

/// A branch's commits, the newest first, and whether more were left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Commits {
    pub commits: Vec<Commit>,
    pub truncated: bool,
}

/// How a provider is connected: by an API key the person gives; by a sign-in in a browser; through an agent on the
/// machine, signed in to on its own; or as a server of the person's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Connects {
    ApiKey,
    SignIn,
    Agent,
    Server,
}

/// The kind of credential kept for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    ApiKey,
    OAuth,
    None,
}

/// A provider, as the person is shown it: its id and name, how it is connected and the agent it is reached through,
/// the kind of credential kept for it — none where none is — the account it is, where the person added it, and its
/// quota, where it is used up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProviderShown {
    pub id: String,
    pub name: String,
    pub connects: Connects,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub through: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub credential: Option<CredentialKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account: Option<siphon_accounts::Account>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub used_up: Option<siphon_accounts::UsedUp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProvidersListed {
    pub providers: Vec<ProviderShown>,
}

/// How a provider's account stands, where its driver tells more of it than its credential does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ProviderStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub standing: Option<octoii_ai::driver::Standing>,
}

/// A provider, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfProvider {
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SignInBegun {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FinishSignIn {
    pub provider: String,
    pub redirect: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SetCredential {
    pub provider: String,
    pub api_key: String,
}

/// An account to add: of the provider `provider`, named `name`, once for `key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AddAccount {
    pub provider: String,
    pub name: String,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AccountAdded {
    pub account: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RenameAccount {
    pub account: String,
    pub name: String,
}

/// An account, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfAccount {
    pub account: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ModelsListed {
    pub models: Vec<octoii_ai::catalogue::Model>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GrantsListed {
    pub projects: Vec<siphon_tools::projects::Granted>,
}

/// What a client subscribes to of Siphon's: a thread — its entries from where the client stood, none for from the start, then its
/// status and what streams of its requests — the list of threads, or the settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SiphonTopic {
    Thread {
        thread: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        from: Option<Cursor>,
    },
    Threads,
    Settings,
}

/// The settings, as a client is told of them: the environment's, which every client shares, and its device's own,
/// which Siphon keeps for it and reads none of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Settings {
    pub environment: Environment,
    pub device: serde_json::Value,
}

/// An event of a subscription to Siphon's, by the subscription's id: a thread's entries appended, with where its
/// reader then stands; its status as it changed; what streams of a request as its model answers, by its offset in the
/// request's stream; a thread of the list as it changed, or gone; the settings as they changed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SiphonEvent {
    Entries {
        subscription: String,
        entries: Vec<Entry>,
        cursor: Cursor,
    },
    Status {
        subscription: String,
        status: Status,
    },
    Stream {
        subscription: String,
        request: String,
        offset: u64,
        #[ts(as = "siphon_agent::Streamed<'static>")]
        data: Box<serde_json::value::RawValue>,
    },
    Thread {
        subscription: String,
        thread: Box<Listed>,
    },
    ThreadGone {
        subscription: String,
        thread: String,
    },
    Settings {
        subscription: String,
        settings: Settings,
    },
}
