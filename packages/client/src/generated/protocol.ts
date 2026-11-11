// Siphon's protocol, generated from its Rust types — not to be edited by hand.

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
 * How a call ended.
 */
export type CallStatus = "ok" | "unavailable" | "invalid-arguments" | "tool-error" | "rejected" | "interrupted";

/**
 * A terminal's size, in cells.
 */
export type Cells = { rows: number, cols: number, };

/**
 * A field changed: what it was, and what it is; `None` for not set.
 */
export type Changed<T> = { from: T | null, to: T | null, };

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
export type ConfigChange = { model?: string | null | null, effort?: ReasoningEffort | null | null, mode?: Mode | null | null, workspace?: string | null | null, title?: string | null | null, budget_nanodollars?: number | null | null, };

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

export type CreateThread = { config: Config, key: string, };

export type Created = { thread: string, };

/**
 * Where a reader of a log stands: the segment it read, and how far.
 */
export type Cursor = { segment: string, offset: number, };

export type Deliver = { thread: string, content: Array<InputContent>, key: string, };

export type Delivered = { delivery: string, };

/**
 * An entry of a thread's log.
 */
export type Entry = { "type": "config", id: string, actor: string, model?: Changed<string> | null, effort?: Changed<ReasoningEffort> | null, mode?: Changed<Mode> | null, workspace?: Changed<string> | null, title?: Changed<string> | null, budget_nanodollars?: Changed<number> | null, } | { "type": "input", id: string, actor: string, delivery: string, content: Array<InputContent>, reply_to?: ReplyTo | null, } | { "type": "system", id: string, sections: Array<Section>, tools?: Array<Tool>, } | { "type": "reply", id: string, request: string, response: Response, cancelled?: boolean, } | { "type": "result", id: string, call: string, status: CallStatus, output: FunctionCallOutput, } | { "type": "turn-end", id: string, turn?: string | null, outcome: Outcome, reason?: string | null, } | { "type": "compaction", id: string, request: string, kept?: string | null, from: string, effort?: ReasoningEffort | null, response: Response, } | { "type": "discarded", id: string, request: string, response: Response, };

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
export type EnvironmentChange = { thread_mode?: Mode | null, favorite_models?: Array<string> | null, review_model?: string | null | null, history_retention_days?: number | null | null, };

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
 * A file the host keeps, by the reference its content gives it, `sha256:` and the hash: what it is, and its name
 * where it has one.
 */
export type FileRef = { ref: string, media_type: string, filename?: string | null, };

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

/**
 * How closely a model looks at an image.
 */
export type ImageDetail = "auto" | "high" | "low";

/**
 * What is delivered to a thread.
 */
export type Inbound = { "type": "input", actor: string, content: Array<InputContent>, reply_to?: ReplyTo | null, } | { "type": "config", actor: string, model?: string | null | null, effort?: ReasoningEffort | null | null, mode?: Mode | null | null, workspace?: string | null | null, title?: string | null | null, budget_nanodollars?: number | null | null, };

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
 * An item of a conversation.
 */
export type Item = { "type": "message" } & Message | { "type": "function_call" } & FunctionCall | { "type": "function_call_output" } & FunctionCallOutput | { "type": "reasoning" } & Reasoning | { "type": "web_search_call" } & WebSearchCall | { "type": "configuration_update" } & ConfigurationUpdate;

/**
 * Where an item a model writes stands: still being written, written whole, or cut off.
 */
export type ItemStatus = "in_progress" | "completed" | "incomplete";

export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]: JsonValue } | null;

/**
 * A thread as a list of them shows it: its id, its project and parent, how it runs, how the person keeps it, and how
 * it stands.
 */
export type Listed = { id: string, project?: string | null, parent?: Parent | null, config: Config, marks: Marks, status: Status, };

/**
 * The machine a client reached: its name.
 */
export type Machine = { name: string, };

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
 * What the mouse did: pressed, let go, moved, or its wheel turned.
 */
export type MouseMotion = "down" | "up" | "move" | "wheel";

export type Navigate = { page: string, url: string, };

/**
 * Nothing: what a method takes or answers where it takes or answers nothing.
 */
export type Nothing = Record<symbol, never>;

/**
 * A page, by its id.
 */
export type OfPage = { page: string, };

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
 * A page of a thread's: its id, where it is and what it is called, its view, and the dialog that holds it.
 */
export type PageListed = { id: string, url: string, title: string, view: View, dialog?: PageDialog, };

/**
 * The mouse over a page, at a point of its CSS pixels: what it did, the button pressed or let go, the buttons held
 * and the modifiers, as the DOM counts them, how many clicks a press makes, and how far the wheel turned.
 */
export type PageMouse = { motion: MouseMotion, x: number, y: number, button: PageButton, buttons: number, clicks: number, modifiers: number, delta_x: number, delta_y: number, };

export type PageOpened = { page: string, };

/**
 * Where a sub-thread came from: the thread that delegated it, and its call that did.
 */
export type Parent = { thread: string, call: string, };

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

/**
 * Who is told how the turn that takes an input ends, by mail of `key`: the process at `process`, by its path, as the
 * commit `started` started it — whichever lives there, where none — as `wait(2)` tells a parent its child's status.
 */
export type ReplyTo = { process: string, started?: number | null, key: string, };

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
 * What a session lets its client do: each scope opens some of the methods.
 */
export type Scope = "threads.read" | "threads.operate" | "files.read" | "terminals.operate" | "browser.operate" | "providers.manage" | "access.read" | "access.write";

/**
 * A section of what the model is told, by its name, with its text as it is sent; none for a section no longer told.
 */
export type Section = { name: string, text: string | null, };

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
 * An event of a subscription to Siphon's, by the subscription's id: a thread's entries appended, with where its
 * reader then stands; its status as it changed; what streams of a request as its model answers, by its offset in the
 * request's stream; a thread of the list as it changed, or gone; the settings as they changed.
 */
export type SiphonEvent = { "event": "entries", subscription: string, entries: Array<Entry>, cursor: Cursor, } | { "event": "status", subscription: string, status: Status, } | { "event": "stream", subscription: string, request: string, offset: number, data: JsonValue, } | { "event": "thread", subscription: string, thread: Listed, } | { "event": "thread_gone", subscription: string, thread: string, } | { "event": "settings", subscription: string, settings: Settings, };

/**
 * What a client subscribes to of Siphon's: a thread — its entries from where the client stood, none for from the start, then its
 * status and what streams of its requests — the list of threads, or the settings.
 */
export type SiphonTopic = { "type": "thread", thread: string, from?: Cursor, } | { "type": "threads" } | { "type": "settings" };

/**
 * How a thread stands.
 */
export type Status = { turn: Turn, began_at?: number | null, ended_at?: number | null, usage?: ThreadUsage | null, 
/**
 * What waits in its inbox, by its id, in the order it came.
 */
inbox: Array<[string, Inbound]>, asks: Array<Ask>, };

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
 * The entries read, oldest first, and whether they reach the thread's first.
 */
export type ThreadRead = { entries: Array<Entry>, complete: boolean, };

/**
 * What a thread's model requests used, as Beak counts it on its main process for every process under it, as cgroup v2
 * charges a group and its ancestors.
 */
export type ThreadUsage = { input_tokens: number, cached_tokens: number, output_tokens: number, reasoning_tokens: number, cost_nanodollars: number, };

export type ThreadsListed = { threads: Array<Listed>, };

/**
 * A tool a model may call: a function of the caller's, its arguments' JSON Schema given; or the provider's own web
 * search, which the provider runs.
 */
export type Tool = { "type": "function", name: string, description: string, parameters: JsonValue, } | { "type": "web_search" };

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
