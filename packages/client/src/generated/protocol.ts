// Siphon's protocol, generated from its Rust types — not to be edited by hand.

/**
 * A session or a link, as the access shows it: its id, what it grants, its label, how its client holds it — none for a
 * link — and when it was made and goes.
 */
export type AccessEntry = { id: string, scopes: Array<Scope>, label?: string, holder?: Holder, made_at: number, expires_at: number, };

export type AccessListed = { sessions: Array<AccessEntry>, links: Array<AccessEntry>, };

/**
 * An account, as a list shows it: its id, the host's provider it is another of, and the name it was given.
 */
export type Account = { id: string, provider: string, name: string, };

export type AccountAdded = { account: string, };

/**
 * An account to add: of the provider `provider`, named `name`, once for `key`.
 */
export type AddAccount = { provider: string, name: string, key: string, };

export type AnswerDialog = { page: string, accept: boolean, text?: string, };

/**
 * What a thread asks the person, by its request's key, answered with `answer`, which fits the schema the ask gives.
 */
export type AnswerThread = { thread: string, key: string, answer: JsonValue, };

/**
 * A request a process of the thread waits on a person's answer to: the process, by its path, the request's key and
 * name, what it asks, and the JSON Schema the answer fits.
 */
export type Ask = { process: string, key: string, name: string, payload: JsonValue, answer_schema: JsonValue, };

/**
 * An event of a subscription to the browser's: a thread's pages as they changed; or a frame of a page, the newest the
 * browser painted — its number among the page's, its size in device pixels — sent as a binary frame whose bytes after
 * this event are the frame's JPEG.
 */
export type BrowserEvent = { "event": "pages", subscription: string, pages: Array<PageListed>, } | { "event": "frame", subscription: string, number: number, width: number, height: number, };

/**
 * What a client subscribes to of the browser's: a thread's pages, or a page as it is painted.
 */
export type BrowserTopic = { "type": "pages", thread: string, } | { "type": "page", page: string, };

/**
 * A call a reply made, by the model's id of it and Siphon's key of it — the key it is run, asked of and answered by,
 * minted as the reply is written; the model's id is its data.
 */
export type CallKey = { call_id: string, key: string, };

/**
 * How a call ended.
 */
export type CallStatus = "ok" | "unavailable" | "invalid-arguments" | "tool-error" | "rejected" | "interrupted";

/**
 * A terminal's size, in cells.
 */
export type Cells = { rows: number, cols: number, };

/**
 * What became of a file between two trees; untracked, one in the working tree git does not keep.
 */
export type ChangeStatus = "added" | "deleted" | "modified" | "renamed" | "copied" | "type-changed" | "untracked";

/**
 * A field changed: what it was, and what it is; `None` for not set.
 */
export type Changed<T> = { from: T | null, to: T | null, };

/**
 * A file changed: its path from the repository's root, the one it had, what became of it, the lines it gained and
 * lost — none where they were not counted — and whether it is binary.
 */
export type ChangedFile = { path: string, old_path?: string, status: ChangeStatus, additions?: number, deletions?: number, binary: boolean, };

/**
 * The changes asked for: the repository's root, the trees they are between, the branch they are told against, and
 * the files changed.
 */
export type Changes = { root: string, comparison: Comparison, base?: string, files: Array<ChangedFile>, };

/**
 * Which changes are asked for: those not yet committed; a branch's against where it parted from its base — the
 * repository's, where none is named — the working tree's own included; or one commit's.
 */
export type ChangesScope = { "type": "uncommitted" } | { "type": "branch", base?: string, } | { "type": "commit", commit: string, };

/**
 * How a command ended: it exited with a code, a signal ended it, its time ran out, it was stopped, or it wrote past
 * what a call takes.
 */
export type CommandEnd = { "type": "exited", code: number, } | { "type": "signalled", signal: number, } | { "type": "timed_out", seconds: number, } | { "type": "stopped" } | { "type": "too_large", bytes: number, };

/**
 * A commit of a branch: its id, when it was committed, in seconds since the epoch, and its subject.
 */
export type Commit = { commit: string, time: number, subject: string, };

/**
 * A branch's commits, the newest first, and whether more were left out.
 */
export type Commits = { commits: Array<Commit>, truncated: boolean, };

/**
 * The two trees changes are told between: the older, by its object's id; the newer, a commit's, or the working
 * tree's where none.
 */
export type Comparison = { old: string, new?: string, };

/**
 * What fills a window: its tokens, and each part's share of them — those that hold none left out.
 */
export type Composition = { tokens: number, parts: Array<Share>, };

/**
 * How a thread runs: each setting, or none where it has none.
 */
export type Config = { 
/**
 * The model its turns ask, by its reference.
 */
model?: string | null, 
/**
 * How hard its model reasons; as the model does, where none.
 */
effort?: ReasoningEffort | null, 
/**
 * How its calls go ahead; `default` where none.
 */
mode?: Mode | null, 
/**
 * The folder on the machine its work is done in, by its absolute path.
 */
workspace?: string | null, title?: string | null, 
/**
 * The most its model requests may cost in all, in billionths of a US dollar; no bound, where none.
 */
budget_nanodollars?: number | null, };

/**
 * A change of how a thread runs: each setting given, `Some(None)` to let it go; each left out kept.
 */
export type ConfigChange = { model?: string | null, effort?: ReasoningEffort | null, mode?: Mode | null, workspace?: string | null, title?: string | null, budget_nanodollars?: number | null, };

/**
 * A change of the reasoning effort from where it stands, so what a window began with is never changed within it.
 */
export type ConfigurationUpdate = { effort: ReasoningEffort, };

export type ConfigureThread = { thread: string, change: ConfigChange, };

/**
 * An event of the connection's own: the welcome a client is sent first — its session, what it may do, the machine
 * it reached — or that a subscription fell behind and was let go, with the topic to subscribe to again from where it
 * stood.
 */
export type ConnectionEvent = { "event": "welcome", session: string, scopes: Array<Scope>, machine: Machine, } | { "event": "lagged", subscription: string, topic: Topic, };

/**
 * How a provider is connected: by an API key the person gives; by a sign-in in a browser; through an agent on the
 * machine, signed in to on its own; or as a server of the person's.
 */
export type Connects = "api_key" | "sign_in" | "agent" | "server";

/**
 * What holds a share of a window.
 */
export type ContextPart = "system" | "instructions" | "skills" | "catalog" | "tools" | "recap" | "messages" | "replies" | "results";

export type CreateThread = { config: Config, key: string, };

export type Created = { thread: string, };

/**
 * The kind of credential kept for a provider.
 */
export type CredentialKind = "api_key" | "oauth" | "none";

/**
 * Where a reader of a log stands: the segment it read, and how far.
 */
export type Cursor = { segment: string, offset: number, };

export type DecideTrust = { folder: string, digest: string, trusted: boolean, };

export type Deliver = { thread: string, content: Array<InputContent>, key: string, };

export type Delivered = { delivery: string, };

/**
 * An entry of a thread's log.
 */
export type Entry = { "type": "config", id: string, actor: string, model?: Changed<string> | null, effort?: Changed<ReasoningEffort> | null, mode?: Changed<Mode> | null, workspace?: Changed<string> | null, title?: Changed<string> | null, budget_nanodollars?: Changed<number> | null, } | { "type": "input", id: string, actor: string, delivery: string, content: Array<InputContent>, reply_to?: ReplyTo | null, } | { "type": "system", id: string, sections: Array<Section>, tools?: Array<Tool>, } | { "type": "reply", id: string, request: string, response: Response, calls?: Array<CallKey>, cancelled?: boolean, } | { "type": "result", id: string, call: string, status: CallStatus, output: FunctionCallOutput, structured?: JsonValue | null, } | { "type": "turn-end", id: string, turn?: string | null, outcome: Outcome, reason?: string | null, failure?: Failure | null, } | { "type": "compaction", id: string, request: string, kept?: string | null, from: string, effort?: ReasoningEffort | null, response: Response, } | { "type": "discarded", id: string, request: string, response: Response, };

/**
 * The environment's settings.
 */
export type Environment = { 
/**
 * The mode a thread starts in where it is made with none.
 */
thread_mode: Mode, 
/**
 * The models the person starred, by their references.
 */
favorite_models: Array<string>, 
/**
 * The model that judges calls in auto; with none, no call goes ahead in auto without the person.
 */
review_model: string | null, 
/**
 * How many days a thread keeps what came before its window; forever, where none.
 */
history_retention_days: number | null, };

/**
 * A change of the environment's settings: each given in place of the one it had, each left out kept;
 * `Some(None)` lets an optional one go.
 */
export type EnvironmentChange = { thread_mode?: Mode | null, favorite_models?: Array<string> | null, review_model?: string | null, history_retention_days?: number | null, };

/**
 * Why a call was refused, or what it asked could not be done: a code a client may act on, and words for the person.
 */
export type Error = { code: string, message: string, };

/**
 * Why a request failed: its kind; the provider's code and words; the HTTP status it answered with; how long to wait
 * before trying again, where it says; and, for a quota used up, when it is given again and over what window it is
 * counted, where it says.
 */
export type Failure = { kind: FailureKind, message: string, code?: string | null, status?: number | null, retry_after_ms?: number | null, reset_at_ms?: number | null, window_minutes?: number | null, };

/**
 * What kind of failure ended a request, which decides whether trying again may help.
 */
export type FailureKind = "context_overflow" | "payload_too_large" | "rate_limit" | "quota" | "auth" | "forbidden" | "content_filter" | "invalid_request" | "server" | "transport" | "invalid_response" | "timeout";

/**
 * What sets a text model apart from its provider's others: whether it takes instructions updated after the person's
 * first turn where they stand, and an effort changed where it stands; and the tool choices it takes. Each is unknown
 * where no source says.
 */
export type Features = { instruction_updates: boolean | null, effort_updates: boolean | null, tool_choices: Array<string> | null, };

/**
 * What a name in a folder is.
 */
export type FileKind = "directory" | "file" | "link";

/**
 * What a file holds, with the file, whole and clean, and its length in bytes: lines of text from the first given, and
 * whether more follow; an image or a PDF, by its media type, its bytes at the node's HTTP; or that it is no text.
 */
export type FilePage = { "type": "text", path: string, size: number, from: number, lines: Array<string>, more: boolean, } | { "type": "media", path: string, size: number, media_type: string, } | { "type": "binary", path: string, size: number, };

/**
 * A file the host keeps, by the reference its content gives it, `sha256:` and the hash: what it is, and its name
 * where it has one.
 */
export type FileRef = { ref: string, media_type: string, filename?: string | null, };

export type FinishSignIn = { provider: string, redirect: string, };

/**
 * A name in a folder: what it names, and whether its repository's ignore rules leave it out.
 */
export type FolderEntry = { name: string, kind: FileKind, ignored: boolean, };

/**
 * A page of a folder's names: the folder, whole and clean; its names in order; whether more follow the last.
 */
export type FolderPage = { path: string, entries: Array<FolderEntry>, more: boolean, };

/**
 * A file or a folder found: its path from the folder searched, and what it is.
 */
export type FoundPath = { path: string, kind: FileKind, };

/**
 * The files and folders found under a folder, the best match first; and whether more matched.
 */
export type FoundPaths = { path: string, found: Array<FoundPath>, truncated: boolean, };

/**
 * A model's call of a function by name, its arguments JSON text.
 */
export type FunctionCall = { 
/**
 * The item's id: its provider's, carried as it came, as every id a provider gives is its data.
 */
id?: string | null, 
/**
 * The call's id, which its output carries: its provider's, or one minted where the provider left it empty or gave
 * it to another call. A caller that names the call by an id of its own keeps this one beside it: the conversation
 * goes by this one, and a provider takes back only ids of its own.
 */
call_id: string, name: string, arguments: string, 
/**
 * `incomplete` for a call cut off, whose arguments may be cut and which is never run.
 */
status?: ItemStatus | null, origin?: Origin | null, };

/**
 * What a function call gave back to the model, and whether it failed.
 */
export type FunctionCallOutput = { call_id: string, output: Array<InputContent>, is_error?: boolean, };

export type GitChanges = { path: string, scope: ChangesScope, };

/**
 * The commits to list: of the branch the folder `path` is on, since it parted from `base` — the repository's, where
 * none.
 */
export type GitCommits = { path: string, base?: string, };

/**
 * A file's patch to read: of the repository `path` is in, between `comparison`'s trees, with `context` lines about each
 * change — git's own number, where none.
 */
export type GitPatch = { path: string, comparison: Comparison, file: ChangedFile, context?: number, };

export type GitVersions = { path: string, comparison: Comparison, file: ChangedFile, };

/**
 * A project's grants, as the person is shown them: its folder, and what it was granted for good.
 */
export type Granted = { folder: string, grants: Grants, };

/**
 * What the person granted a project for good: each a command's pattern, as `git push *`, a path's glob, as
 * `/work/app/docs/**`, or a tool's name.
 */
export type Grants = { commands: Array<string>, paths: Array<string>, tools: Array<string>, };

export type GrantsListed = { projects: Array<Granted>, };

/**
 * What the person does to reach a provider, in words, and the commands they run for it, in order.
 */
export type Guidance = { words: string, commands: Array<string>, };

/**
 * How a client holds its session: a browser's cookie, or a bearer token — a program's, or a device's in a tunnel session.
 */
export type Holder = "cookie" | "bearer";

/**
 * How closely a model looks at an image.
 */
export type ImageDetail = "auto" | "high" | "low";

/**
 * A folder of the machine's, as the person names it.
 */
export type InFolder = { path: string, };

/**
 * The repository a folder is in; none where it is in none.
 */
export type InRepository = { repository?: Repository, };

/**
 * What is delivered to a thread.
 */
export type Inbound = { "type": "input", actor: string, content: Array<InputContent>, reply_to?: ReplyTo | null, } | { "type": "config", actor: string, model?: string | null, effort?: ReasoningEffort | null, mode?: Mode | null, workspace?: string | null, title?: string | null, budget_nanodollars?: number | null, };

/**
 * Why a response ended `incomplete`: at its output limit, filtered by the provider's policy, or cut off by its caller,
 * with what had come of it.
 */
export type Incomplete = "max_output_tokens" | "content_filter" | "cancelled";

/**
 * What user, developer and system messages and calls' outputs hold.
 */
export type InputContent = { "type": "input_text", text: string, } | { "type": "input_image", file: FileRef, detail: ImageDetail, } | { "type": "input_file", file: FileRef, };

/**
 * The largest a model's input may be, where its provider says.
 */
export type InputLimits = { request_bytes: number | null, image_bytes: number | null, image_side_pixels: number | null, images: number | null, };

/**
 * An item of a conversation.
 */
export type Item = { "type": "message" } & Message | { "type": "function_call" } & FunctionCall | { "type": "function_call_output" } & FunctionCallOutput | { "type": "reasoning" } & Reasoning | { "type": "web_search_call" } & WebSearchCall | { "type": "configuration_update" } & ConfigurationUpdate;

/**
 * Where an item a model writes stands: still being written, written whole, or cut off.
 */
export type ItemStatus = "in_progress" | "completed" | "incomplete";

export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]: JsonValue } | null;

/**
 * One of a plan's limits as its usage tells: the minutes its window lasts — five hours, or a week — the models it is
 * of, where it is of some alone, how much of it is used, in percent, and when it is given again, each where it is
 * known.
 */
export type Limit = { window_minutes?: number, scope?: string, used_percent?: number, resets_at_ms?: number, };

/**
 * A link made: what it grants, its credential — shown this once — and the addresses that open it: on this computer,
 * and on another device where the node has a home relay.
 */
export type LinkMade = { link: AccessEntry, credential: string, url: string, device?: string, };

/**
 * A page of a folder's entries: the folder, as the person names it, those after the entry `after` — from its first,
 * where none — `limit` at most.
 */
export type ListFolder = { path: string, after?: string, limit: number, };

export type ListServers = { workspace?: string, };

/**
 * A thread as a list of them shows it: its id, its project and parent, how it runs, how the person keeps it, and how
 * it stands.
 */
export type Listed = { id: string, project?: string | null, parent?: Parent | null, config: Config, marks: Marks, status: Status, };

/**
 * The machine a client reached: its name, and the folder a thread of no folder of its own works in.
 */
export type Machine = { name: string, workspace: string, };

export type MarkThread = { thread: string, change: MarksChange, };

/**
 * How the person keeps a thread; times in milliseconds since the epoch.
 */
export type Marks = { pinned: boolean, archived: boolean, 
/**
 * Till when it is snoozed, kept out of their threads till then.
 */
snoozed_until?: number | null, 
/**
 * Till when, by the thread's own times, the person has seen what it brought them.
 */
seen_until?: number | null, 
/**
 * When they put it with what is done: settled till something comes of it after.
 */
settled_at?: number | null, 
/**
 * When they brought it back from what is done.
 */
unsettled_at?: number | null, };

/**
 * A change of how the person keeps a thread: each mark given in place of the one it had, each left out kept; a snooze
 * till a time past ends it.
 */
export type MarksChange = { pinned?: boolean | null, archived?: boolean | null, snoozed_until?: number | null, seen_until?: number | null, settled_at?: number | null, unsettled_at?: number | null, };

/**
 * A message: the person's, a developer's or the system's, which update the instructions from where they stand; or
 * the model's.
 */
export type Message = { "role": "user", content: Array<InputContent>, } | { "role": "developer", content: Array<InputContent>, } | { "role": "system", content: Array<InputContent>, } | { "role": "assistant", id?: string | null, content: Array<OutputContent>, status?: ItemStatus | null, origin?: Origin | null, };

/**
 * How a thread's calls go ahead: in `default`, what goes beyond the workspace and the sandbox asks the person; in
 * `auto`, what would ask is judged by a second model, the person asked where it doubts; in `full-access`, everything.
 */
export type Mode = "default" | "auto" | "full-access";

/**
 * A text model: its provider's id of it and its provider, which make its reference `provider/id`; its name for
 * people; and its facts.
 */
export type Model = { id: string, provider: string, name: string, 
/**
 * The tokens a request and its response may take together.
 */
context_length: number | null, 
/**
 * The tokens a response may take.
 */
max_output_tokens: number | null, 
/**
 * Whether it reads images, and files such as PDFs, besides text.
 */
reads_images: boolean, reads_files: boolean, 
/**
 * How it reasons; none for a model that does not.
 */
reasoning: ModelReasoning | null, 
/**
 * The request's fields it takes, by their names, such as `structured_outputs`; none, it takes every one.
 */
parameters: Array<string> | null, 
/**
 * Its prices; none for a model that cannot be priced, as one a subscription pays for.
 */
pricing: Pricing | null, 
/**
 * When its provider released it, as YYYY-MM-DD.
 */
release_date: string | null, 
/**
 * Whether its provider runs a web search for it.
 */
searches_web: boolean, limits: InputLimits, features: Features, };

/**
 * How a model reasons: the efforts it takes, highest first — every one where none is listed — the one it reasons at
 * where a request names none, and whether it always reasons, taking no effort `none`.
 */
export type ModelReasoning = { efforts: Array<ReasoningEffort> | null, default_effort: ReasoningEffort | null, mandatory: boolean, };

export type ModelsListed = { models: Array<Model>, };

/**
 * What the mouse did: pressed, let go, moved, or its wheel turned.
 */
export type MouseMotion = "down" | "up" | "move" | "wheel";

export type Navigate = { page: string, url: string, };

/**
 * Nothing: what a method takes or answers where it takes or answers nothing.
 */
export type Nothing = Record<symbol, never>;

/**
 * A session or a link, by its id.
 */
export type OfAccessEntry = { id: string, };

/**
 * An account, by its id.
 */
export type OfAccount = { account: string, };

/**
 * A page, by its id.
 */
export type OfPage = { page: string, };

/**
 * A provider, by its id.
 */
export type OfProvider = { provider: string, };

/**
 * A terminal, by its id.
 */
export type OfTerminal = { terminal: string, };

/**
 * A thread, by its id.
 */
export type OfThread = { thread: string, };

/**
 * What the person did on a page.
 */
export type OnPage<E> = { page: string, event: E, };

export type OpenPage = { thread: string, url?: string, view: View, };

export type OpenTerminal = { thread: string, size: Cells, };

/**
 * Which model wrote an item, by its reference, and what its provider attached to it that no field holds — a
 * signature on a call, say — an object of objects, each under its provider's id, which goes back to that provider
 * alone.
 */
export type Origin = { model: string, provider_metadata?: JsonValue | null, };

/**
 * How a turn ended.
 */
export type Outcome = "replied" | "failed" | "stopped";

/**
 * What an assistant message holds: text the model wrote, with the pages it cites, or its refusal in its words.
 */
export type OutputContent = { "type": "output_text", text: string, annotations?: Array<UrlCitation>, } | { "type": "refusal", refusal: string, };

/**
 * Prices that replace a model's base prices for a request that meets every condition given: an input of more
 * tokens than `min_prompt_tokens`; a time of day by UTC from `utc_start` to before `utc_end`, as HHMM, a window that
 * may wrap past midnight; days by UTC, Monday 0.
 */
export type Override = { min_prompt_tokens: number | null, utc_start: number | null, utc_end: number | null, utc_days: Array<number> | null, prices: Prices, };

/**
 * A mouse button, as the DOM names them.
 */
export type PageButton = "none" | "left" | "middle" | "right";

/**
 * A dialog that holds a page till it is answered: its kind — alert, confirm, prompt, beforeunload — its message, and
 * the text a prompt offers.
 */
export type PageDialog = { kind: string, message: string, offered: string, };

/**
 * A key pressed or let go on a page: its name and the key it is on, as the DOM names them, the text it types, the
 * modifiers held, its key code, and the editing commands it stands for as the person's platform binds its shortcuts.
 */
export type PageKey = { pressed: boolean, key: string, code: string, text?: string, modifiers: number, key_code: number, commands: Array<string>, };

/**
 * A page of a thread's: its id, where it is and what it is called, its view, the dialog that holds it, whether it
 * loads, and whether its history goes back and forward from where it is.
 */
export type PageListed = { id: string, url: string, title: string, view: View, dialog?: PageDialog, loading: boolean, back: boolean, forward: boolean, };

/**
 * The mouse over a page, at a point of its CSS pixels: what it did, the button pressed or let go, the buttons held
 * and the modifiers, as the DOM counts them, how many clicks a press makes, and how far the wheel turned.
 */
export type PageMouse = { motion: MouseMotion, x: number, y: number, button: PageButton, buttons: number, clicks: number, modifiers: number, delta_x: number, delta_y: number, };

export type PageOpened = { page: string, };

/**
 * What a link is made with: its label, and what it grants — everything, where none is given.
 */
export type PairWith = { label?: string, scopes?: Array<Scope>, };

/**
 * Where a sub-thread came from: the thread that delegated it, and its call that did, by Siphon's key of it.
 */
export type Parent = { thread: string, call: string, };

/**
 * What text is added to: an answer's words, a refusal's, the reasoning, its summary, or a call's arguments.
 */
export type Part = "text" | "refusal" | "reasoning" | "summary" | "arguments";

export type Patch = { patch: string, };

/**
 * The prices a model lists for a text request, each a unit's: a token read, read from the prompt cache, written to
 * it, written, spent reasoning; a web search; the request itself. None, the meter is not charged.
 */
export type Prices = { prompt: Rate | null, completion: Rate | null, cache_read: Rate | null, cache_write: Rate | null, reasoning: Rate | null, web_search: Rate | null, request: Rate | null, };

/**
 * A model's prices: its base prices, the overrides a later one of which wins, and the fraction taken off every
 * price, in millionths.
 */
export type Pricing = { base: Prices, overrides: Array<Override>, discount_millionths: number, };

/**
 * A project's configuration of servers as the person decides on it: what it names, and whether they trust it — none
 * where they decided nothing on it as it is.
 */
export type ProjectServers = { named: Trust, trusted?: boolean, };

/**
 * A provider, as the person is shown it: its id and name, how it is connected and the agent it is reached through,
 * the kind of credential kept for it — none where none is — the account it is, where the person added it, and its
 * quota, where it is used up.
 */
export type ProviderShown = { id: string, name: string, connects: Connects, through?: string, credential?: CredentialKind, account?: Account, used_up?: UsedUp, };

/**
 * How a provider's account stands, where its driver tells more of it than its credential does.
 */
export type ProviderStatus = { standing?: Standing, };

export type ProvidersListed = { providers: Array<ProviderShown>, };

/**
 * A price a unit: in picodollars, or one that varies, as a router's `-1` says, which no cost can be made of.
 */
export type Rate = { "picodollars": number } | "varies";

/**
 * A file to read: as the person names it, its lines from `offset` — from 1, where none — `limit` at most.
 */
export type ReadFile = { path: string, offset?: number, limit?: number, };

/**
 * A page of a thread's entries: at most `limit`, the latest before the entry `before`, or the latest of all.
 */
export type ReadThread = { thread: string, before?: string, limit: number, };

/**
 * What a model thought before it answered, as far as its provider shows it, with what it needs to go on from it —
 * which only the model that wrote it reads.
 */
export type Reasoning = { id?: string | null, summary?: Array<string>, content?: Array<string>, 
/**
 * The reasoning as its provider keeps it.
 */
encrypted_content?: string | null, 
/**
 * What the provider signed the reasoning with.
 */
signature?: string | null, status?: ItemStatus | null, origin: Origin, };

/**
 * How hard a model reasons, from `none` to `max`.
 */
export type ReasoningEffort = "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";

export type RenameAccount = { account: string, name: string, };

/**
 * Who is told how the turn that takes an input ends, by mail of `key`: the process at `process`, by its path, as the
 * commit `started` started it — whichever lives there, where none — as `wait(2)` tells a parent its child's status.
 */
export type ReplyTo = { process: string, started?: number | null, key: string, };

/**
 * A repository, as a folder in it is: where its work tree begins, its branch — none where its head is detached — its
 * head's commit — none before the first — its upstream, how far ahead of it and behind, and the branch its work was
 * taken from, as its changes are told against unless one is asked for.
 */
export type Repository = { root: string, branch?: string, head?: string, upstream?: string, ahead: number, behind: number, base?: string, };

export type ResizePage = { page: string, view: View, };

export type ResizeTerminal = { terminal: string, size: Cells, };

/**
 * What a request came to: its provider's id of it, the model that answered by its reference, its status, what the
 * model wrote, what it used, why it failed or why it stopped short, and the tokens its context took.
 */
export type Response = { id: string, model: string, status: ResponseStatus, output: Array<Item>, usage?: Usage | null, failure?: Failure | null, incomplete?: Incomplete | null, 
/**
 * The input tokens of the response's last request alone, where it was continued across several.
 */
context_tokens?: number | null, };

/**
 * Where a response stands: under way, or ended `completed`, `incomplete` or `failed`.
 */
export type ResponseStatus = "in_progress" | "completed" | "incomplete" | "failed";

/**
 * A thread going on at `at`, in milliseconds since the epoch; none to let go of the time it goes on at.
 */
export type ResumeThread = { thread: string, at?: number, };

/**
 * What was revoked: a session, or a link.
 */
export type Revoked = { revoked: string, };

/**
 * What a session lets its client do: each scope opens some of the methods.
 */
export type Scope = "threads.read" | "threads.operate" | "files.read" | "terminals.operate" | "browser.operate" | "providers.manage" | "access.read" | "access.write";

/**
 * Files to find: under the folder `path`, as the person names it, whose paths match `query`, `limit` at most.
 */
export type SearchFiles = { path: string, query: string, limit: number, };

/**
 * A section of what the model is told, by its name, with its text as it is sent; none for a section no longer told.
 */
export type Section = { name: string, text: string | null, };

/**
 * A server as the person is shown it: its name, the configuration it comes from by its path, what it runs or where it
 * is reached, whether it is connected now, and its tools' names where it is.
 */
export type ServerListed = { name: string, source: string, runs: string, connected: boolean, tools: Array<string>, };

/**
 * The servers a thread working at a place would use: each as it stands, what of their configurations could not be
 * taken, the person's own configuration by its path, and the project's, with what the person decided on it.
 */
export type ServersListed = { servers: Array<ServerListed>, problems: Array<string>, person?: string, project?: ProjectServers, };

export type SetCredential = { provider: string, api_key: string, };

/**
 * The settings, as a client is told of them: the environment's, which every client shares, and its device's own,
 * which Siphon keeps for it and reads none of.
 */
export type Settings = { environment: Environment, device: JsonValue, };

/**
 * A change of the settings: the environment's, and the device's, as a merge patch of its document.
 */
export type SettingsChange = { environment?: EnvironmentChange | null, device?: JsonValue | null, };

/**
 * A part's share of a window, in tokens.
 */
export type Share = { part: ContextPart, tokens: number, };

/**
 * A server as the person is shown it: its name, and what it runs, or where it is reached.
 */
export type ShownServer = { name: string, runs: string, };

export type SignInBegun = { url: string, };

/**
 * An event of a subscription to Siphon's, by the subscription's id: a thread's entries appended, with where its
 * reader then stands; its status as it changed; what streams of a request as its model answers, by its offset in the
 * request's stream; the threads as they stand, first, then a thread of the list as it changed, or gone; the settings
 * as they changed.
 */
export type SiphonEvent = { "event": "entries", subscription: string, entries: Array<Entry>, cursor: Cursor, } | { "event": "status", subscription: string, status: Status, } | { "event": "stream", subscription: string, request: string, offset: number, data: Streamed, } | { "event": "threads", subscription: string, threads: Array<Listed>, } | { "event": "thread", subscription: string, thread: Listed, } | { "event": "thread_gone", subscription: string, thread: string, } | { "event": "settings", subscription: string, settings: Settings, };

/**
 * What a client subscribes to of Siphon's: a thread — its entries from where the client stood, none for from the start, then its
 * status and what streams of its requests — the list of threads, or the settings.
 */
export type SiphonTopic = { "type": "thread", thread: string, from?: Cursor, } | { "type": "threads" } | { "type": "settings" };

/**
 * How a provider's account stands, as its driver tells it: who it is signed in as and their plan, how much of each of
 * the plan's limits is used, and what the person does to reach it, where they must do something.
 */
export type Standing = { account?: string, plan?: string, limits: Array<Limit>, guidance?: Guidance, };

/**
 * How a thread stands.
 */
export type Status = { turn: Turn, began_at?: number | null, ended_at?: number | null, usage?: ThreadUsage | null, 
/**
 * What waits in its inbox, by its id, in the order it came.
 */
inbox: Array<[string, Inbound]>, asks: Array<Ask>, 
/**
 * Whether it compacts its window now.
 */
compacting: boolean, 
/**
 * When it goes on, where the person asked it to at a time and it has not gone on since.
 */
resuming_at?: number | null, };

/**
 * What streams of a reply as it is written, as the thread's followers are told it: an item begun at its place in the
 * output, text added to one, and one whole.
 */
export type Streamed = { "type": "began", index: number, item: Item, } | { "type": "delta", index: number, part: Part, text: string, } | { "type": "ended", index: number, item: Item, };

/**
 * What a call of one of Siphon's own tools did, as a client reads it: a command that ran, and how it ended; a file a
 * write or an edit changed, whether it made it, and the lines added and taken out; the lines of a file a read holds,
 * and how many it has where the read reached its end; the todos of a todo list.
 */
export type Structured = { "type": "ran", ended: CommandEnd, } | { "type": "changed", path: string, created: boolean, additions: number, deletions: number, } | { "type": "read", path: string, from: number, to: number, total?: number, } | { "type": "todos", todos: Array<Todo>, };

/**
 * A subscription made: its id, which its events carry.
 */
export type Subscribed = { subscription: string, };

/**
 * A terminal of a thread's: its id, its number in the thread, its size, and the title its program set.
 */
export type TerminalListed = { id: string, number: number, size: Cells, title: string, };

export type TerminalOpened = { terminal: string, };

/**
 * An event of a subscription to the terminals': a thread's terminals as they changed; a terminal's screen drawn again
 * whole, through the offset in its output it shows; or a piece of what it wrote, from an offset. A screen and a piece
 * are binary frames, whose bytes after this event are the screen's sequences, or the piece.
 */
export type TerminalsEvent = { "event": "terminals", subscription: string, terminals: Array<TerminalListed>, } | { "event": "screen", subscription: string, through: number, } | { "event": "output", subscription: string, offset: number, };

/**
 * What a client subscribes to of the terminals': a thread's terminals, or a terminal as it writes — from the offset
 * in its output the client stood at, or its screen drawn first, where none.
 */
export type TerminalsTopic = { "type": "terminals", thread: string, } | { "type": "terminal", terminal: string, from?: number, };

/**
 * The entries read, oldest first; whether they reach the thread's first; and where its log ended as they were read —
 * what a subscription to the thread goes on from, none for a log not begun.
 */
export type ThreadRead = { entries: Array<Entry>, complete: boolean, end?: Cursor, };

/**
 * What a thread's model requests used, as Beak counts it on its main process for every process under it, as cgroup v2
 * charges a group and its ancestors.
 */
export type ThreadUsage = { input_tokens: number, cached_tokens: number, output_tokens: number, reasoning_tokens: number, cost_nanodollars: number, };

export type ThreadsListed = { threads: Array<Listed>, };

/**
 * A step of the work.
 */
export type Todo = { 
/**
 * What the step is, in a few words.
 */
content: string, status: TodoStatus, };

/**
 * Where a step stands.
 */
export type TodoStatus = "pending" | "in_progress" | "completed";

/**
 * A tool a model may call: a function of the caller's, its arguments' JSON Schema given; or the provider's own web
 * search, which the provider runs.
 */
export type Tool = { "type": "function", name: string, description: string, parameters: JsonValue, } | { "type": "web_search" };

/**
 * What the person is asked: whether what the project of `folder` names in its configuration — the file at `path`, of
 * the digest `digest` — may run: the servers it names, each as it would run.
 */
export type Trust = { folder: string, path: string, digest: string, servers: Array<ShownServer>, };

/**
 * How a thread's turns stand: none run yet, one running, or how the last ended.
 */
export type Turn = "created" | "running" | "replied" | "failed" | "stopped";

export type TypeText = { page: string, text: string, };

/**
 * A subscription to let go of.
 */
export type Unsubscribe = { subscription: string, };

/**
 * A web page an answer cites, by the characters of the text that cite it.
 */
export type UrlCitation = { url: string, title: string, start_index: number, end_index: number, };

/**
 * What a request used, in tokens — the input's counting those read from the cache and those written to it — and the
 * web searches its provider ran for it, and what it cost, in billionths of a US dollar — the provider's figure where it
 * gives one, else the catalogue's prices'.
 */
export type Usage = { input_tokens: number, 
/**
 * Of `input_tokens`, those read from the prompt cache.
 */
cached_tokens: number, 
/**
 * Of `input_tokens`, those written into the prompt cache.
 */
cache_write_tokens: number, output_tokens: number, 
/**
 * Of `output_tokens`, those spent reasoning.
 */
reasoning_tokens: number, 
/**
 * The web searches the provider ran for it.
 */
web_searches: number, cost_nanodollars?: number | null, };

/**
 * A provider's quota used up: when it is given again, in milliseconds since the epoch, and how long the window it is
 * counted over lasts, in minutes, where the provider said.
 */
export type UsedUp = { reset_at_ms: number, window_minutes?: number, };

/**
 * A file's two versions: the older, none where it came; the newer, none where it went.
 */
export type Versions = { old?: string, new?: string, };

/**
 * A page's view: CSS pixels across and down, and device pixels to each — one, where none is given.
 */
export type View = { width: number, height: number, scale?: number, };

/**
 * What a web search did: searched, opened a page, or looked for a pattern in one.
 */
export type WebSearchAction = { "type": "search", query: string, sources?: Array<string>, } | { "type": "open_page", url?: string | null, } | { "type": "find_in_page", url: string, pattern: string, };

/**
 * A web search the provider ran, whichever provider ran it.
 */
export type WebSearchCall = { id: string, status: WebSearchStatus, action?: WebSearchAction | null, origin?: Origin | null, };

/**
 * Where a web search a provider runs stands.
 */
export type WebSearchStatus = "in_progress" | "searching" | "completed" | "failed";

export type Withdraw = { thread: string, delivery: string, };

export type Withdrawn = { withdrawn: boolean, };

export type WriteTerminal = { terminal: string, data: string, };

/** What a client subscribes to. */
export type Topic = SiphonTopic | BrowserTopic | TerminalsTopic;

/** An event: the connection's, or a subscription's. */
export type Event = ConnectionEvent | SiphonEvent | BrowserEvent | TerminalsEvent;

/** The methods, each by its name: the params it takes and the result it answers with. */
export type Methods = {
  'threads.list': { params: Nothing; result: ThreadsListed };
  'threads.create': { params: CreateThread; result: Created };
  'threads.configure': { params: ConfigureThread; result: Nothing };
  'threads.mark': { params: MarkThread; result: Nothing };
  'threads.deliver': { params: Deliver; result: Delivered };
  'threads.withdraw': { params: Withdraw; result: Withdrawn };
  'threads.stop': { params: OfThread; result: Nothing };
  'threads.delete': { params: OfThread; result: Nothing };
  'threads.answer': { params: AnswerThread; result: Nothing };
  'threads.read': { params: ReadThread; result: ThreadRead };
  'threads.context': { params: OfThread; result: Composition };
  'threads.compact': { params: OfThread; result: Nothing };
  'threads.resume': { params: ResumeThread; result: Nothing };
  'files.list': { params: ListFolder; result: FolderPage };
  'files.read': { params: ReadFile; result: FilePage };
  'files.search': { params: SearchFiles; result: FoundPaths };
  'git.repository': { params: InFolder; result: InRepository };
  'git.changes': { params: GitChanges; result: Changes };
  'git.patch': { params: GitPatch; result: Patch };
  'git.versions': { params: GitVersions; result: Versions };
  'git.commits': { params: GitCommits; result: Commits };
  'providers.list': { params: Nothing; result: ProvidersListed };
  'providers.status': { params: OfProvider; result: ProviderStatus };
  'providers.sign-in': { params: OfProvider; result: SignInBegun };
  'providers.finish-sign-in': { params: FinishSignIn; result: Nothing };
  'credentials.set': { params: SetCredential; result: Nothing };
  'credentials.remove': { params: OfProvider; result: Nothing };
  'accounts.add': { params: AddAccount; result: AccountAdded };
  'accounts.rename': { params: RenameAccount; result: Nothing };
  'accounts.remove': { params: OfAccount; result: Nothing };
  'models.list': { params: Nothing; result: ModelsListed };
  'models.refresh': { params: Nothing; result: Nothing };
  'grants.list': { params: Nothing; result: GrantsListed };
  'grants.revoke': { params: Granted; result: Nothing };
  'settings.set': { params: SettingsChange; result: Nothing };
  'browser.open': { params: OpenPage; result: PageOpened };
  'browser.navigate': { params: Navigate; result: Nothing };
  'browser.back': { params: OfPage; result: Nothing };
  'browser.forward': { params: OfPage; result: Nothing };
  'browser.reload': { params: OfPage; result: Nothing };
  'browser.dialog': { params: AnswerDialog; result: Nothing };
  'browser.close': { params: OfPage; result: Nothing };
  'browser.resize': { params: ResizePage; result: Nothing };
  'browser.mouse': { params: OnPage<PageMouse>; result: Nothing };
  'browser.key': { params: OnPage<PageKey>; result: Nothing };
  'browser.text': { params: TypeText; result: Nothing };
  'terminals.open': { params: OpenTerminal; result: TerminalOpened };
  'terminals.write': { params: WriteTerminal; result: Nothing };
  'terminals.resize': { params: ResizeTerminal; result: Nothing };
  'terminals.close': { params: OfTerminal; result: Nothing };
  'servers.list': { params: ListServers; result: ServersListed };
  'trust.decide': { params: DecideTrust; result: Nothing };
  'access.list': { params: Nothing; result: AccessListed };
  'access.pair': { params: PairWith; result: LinkMade };
  'access.revoke': { params: OfAccessEntry; result: Revoked };
  'subscribe': { params: { topic: Topic }; result: Subscribed };
  'unsubscribe': { params: Unsubscribe; result: Nothing };
};

/** A call: its id, by which its answer comes, its method and the method's params. */
export type Call = { [M in keyof Methods]: { id: string; method: M; params: Methods[M]['params'] } }[keyof Methods];

/** An answer: the call's id and its method's result, or the error it was refused with. */
export type Answer<M extends keyof Methods = keyof Methods> =
  | { id: string; result: Methods[M]['result'] }
  | { id: string | null; error: Error };

/** What Siphon sends: an answer, or an event. */
export type Received = Answer | Event;
