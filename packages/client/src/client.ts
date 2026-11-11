/**
 * Octoii's client: one connection to a node, what it subscribes to — threads, the list of them, the settings, a
 * thread's terminals and pages, a terminal's output and a page's frames — and the protocol's methods. A thread followed
 * is read first, its latest entries alone, then subscribed to from where the log ended as it was read; older entries
 * are read back only as a reader asks. What it subscribes to is subscribed to again on each connection that opens, from
 * where its reader stands. A method that makes or adds to a thread is done once for a key the client mints, and asked
 * again on the next connection where one drops before its answer, so a person's message is neither lost nor taken twice.
 */
import { Connection, type ConnectionStatus } from "./connection.ts";
import { ENTRIES_KEPT } from "./constants.ts";
import { ClientError, clientError, isConnectionError } from "./errors.ts";
import { type Followable, type Followed, Value } from "./followed.ts";
import { Follows } from "./follows.ts";
import type { Clock, ClientOptions } from "./options.ts";
import { followPage, type PageHandlers, type PageView } from "./pages.ts";
import type {
	Cells,
	Config,
	ConfigChange,
	FileRef,
	Granted,
	InputContent,
	JsonValue,
	Listed,
	MarksChange,
	MethodName,
	Model,
	PageListed,
	ParamsOf,
	ProviderShown,
	ResultOf,
	Settings,
	SettingsChange,
	Standing,
	SubscriptionEvent,
	TerminalListed,
	Topic,
	View,
} from "./protocol.ts";
import { Subscriptions } from "./subscriptions.ts";
import { followTerminal, type TerminalHandlers, type TerminalView } from "./terminals.ts";
import type { ThreadView } from "./thread.ts";
import { type ThreadsView, WatchedThreads } from "./threads.ts";
import { Watches } from "./watch.ts";

/** What a client gives its app. */
export type Client = {
	readonly status: Followed<ConnectionStatus>;
	readonly clock: Clock;
	/** Calls a method as it is, on the open connection. */
	call<Name extends MethodName>(method: Name, params: ParamsOf<Name>): Promise<ResultOf<Name>>;
	/** The threads, each as it stands. */
	threads(): Promise<readonly Listed[]>;
	/** Makes a thread that runs as `config` says, once: its id. */
	create(config: Config): Promise<string>;
	/** Changes how the thread runs: each setting given in place of the one it had, or let go where it is given null. */
	configure(thread: string, change: ConfigChange): Promise<void>;
	/** Changes how the person keeps the thread: pinned, archived, snoozed, seen, settled — each given in place of the one it had. */
	mark(thread: string, change: MarksChange): Promise<void>;
	/** Delivers `content` to the thread, once: the id it was delivered by, which withdraws it while it waits and which its entry in the log records. */
	deliver(thread: string, content: readonly InputContent[]): Promise<string>;
	/** Takes back what was delivered by `delivery` while it waits in the thread's inbox: whether it was still there. */
	withdraw(thread: string, delivery: string): Promise<boolean>;
	/** Deletes the thread, whatever it was doing: its log, its processes, its terminals and its pages gone for good. */
	delete(thread: string): Promise<void>;
	/** Stops the thread for the person: what waits in its inbox withdrawn, its turn asked to stop. */
	stop(thread: string): Promise<void>;
	/** Gives the person's answer to what the thread asks them, by the ask's key. */
	answer(thread: string, key: string, answer: JsonValue): Promise<void>;
	/** Follows the thread: one followed for all who read it, its latest entries first, let go after the last. */
	follow(thread: string): Followable<ThreadView>;
	/** Reads a page further back of a thread followed, before the entries it holds: it keeps them too. */
	readOlder(thread: string): Promise<void>;
	/** Watches the list of threads: one watch for all who read it, let go after the last. */
	watchThreads(): Followable<ThreadsView>;
	/** Watches the settings — the environment's and this device's — none until the node told them: one watch for all who read them. */
	watchSettings(): Followable<Settings | undefined>;
	/** Changes the settings as `change` says. */
	setSettings(change: SettingsChange): Promise<void>;
	/** Watches the thread's terminals, in the order they were opened: none until the node told them. */
	watchTerminals(thread: string): Followable<readonly TerminalListed[] | undefined>;
	/** Opens a terminal for the thread, at the size of the view that opens it: its id. */
	openTerminal(thread: string, size: Cells): Promise<string>;
	/** Follows the terminal `id` for a view `size` big, told through `handlers`, which types into it, resizes it and lets it go. */
	terminal(id: string, size: Cells, handlers: TerminalHandlers): TerminalView;
	/** Closes the terminal `id`: its program hung up on, and the terminal gone from its thread. */
	closeTerminal(id: string): Promise<void>;
	/** Watches the thread's pages in the browser, in the order they were opened: none until the node told them. */
	watchPages(thread: string): Followable<readonly PageListed[] | undefined>;
	/** Opens a page in the browser for the thread, at `url` or blank, the size of the view that opens it: its id. */
	openPage(thread: string, url: string | undefined, view: View): Promise<string>;
	/** Follows the page `id` for a view `view` big, told through `handlers`, which clicks it, types into it, resizes it and lets it go. */
	page(id: string, view: View, handlers: PageHandlers): PageView;
	/** Closes the page `id`: gone from its thread. */
	closePage(id: string): Promise<void>;
	/** What the person granted each project. */
	grants(): Promise<readonly Granted[]>;
	/** Takes back from the project at its folder what `granted` names of its grants. */
	revokeGrant(granted: Granted): Promise<void>;
	/** The node's providers: how each is connected, and the kind of credential the node keeps for it. */
	providers(): Promise<readonly ProviderShown[]>;
	/** How the provider `id`'s account stands, as its driver tells it; none where it tells nothing. */
	providerStatus(id: string): Promise<Standing | undefined>;
	/** Begins a sign-in to the provider `id`: where the person signs in. */
	signIn(id: string): Promise<string>;
	/** Finishes the sign-in to the provider `id`, from the address the browser was sent back to. */
	finishSignIn(id: string, redirect: string): Promise<void>;
	/** Lists the providers' models again. */
	refreshModels(): Promise<void>;
	setKey(provider: string, key: string): Promise<void>;
	removeKey(provider: string): Promise<void>;
	/** Adds an account of the node's provider `provider`, named `name`, once: the id it is a provider by. */
	addAccount(provider: string, name: string): Promise<string>;
	/** Gives the account `account` the name `name`. */
	renameAccount(account: string, name: string): Promise<void>;
	/** Removes the account `account`, and what the node kept for it. */
	removeAccount(account: string): Promise<void>;
	/** The models a thread may run with. */
	models(): Promise<readonly Model[]>;
	/** Has the thread keep a file its model is given, named `name`: the file as the thread keeps it; rejects where the app sends no files. */
	keepFile(thread: string, bytes: Blob, name: string): Promise<FileRef>;
	close(): void;
};

/** Connects to the node, as `options` says how, and keeps on connecting until it is closed. */
export function createClient(options: ClientOptions): Client {
	// What a lost or closed connection cuts off is subscribed to again as the next one opens: only another failure is told.
	const told = (error: unknown): void => {
		if (!isConnectionError(error) && !(error instanceof ClientError && error.code === "closed")) options.report(error);
	};
	const connection: Connection = new Connection(options, {
		opened: () => {
			subscriptions.again();
			follows.again();
		},
		event: (event, bytes) => {
			if ("subscription" in event) subscriptions.take(event, bytes);
		},
	});
	const subscriptions = new Subscriptions(connection, told);
	const follows = new Follows(connection, subscriptions, options.entriesKept ?? ENTRIES_KEPT, told);
	const watches = new Watches(subscriptions);
	connection.start();
	const done = async (calling: Promise<unknown>): Promise<void> => {
		await calling;
	};
	return {
		status: connection.status,
		clock: options.clock,
		call: (method, params) => connection.call(method, params),
		threads: async () => (await connection.call("threads.list", {})).threads,
		create: async (config) => (await connection.callOpen("threads.create", { config, key: options.randomUUID() })).thread,
		configure: (thread, change) => done(connection.call("threads.configure", { thread, change })),
		mark: (thread, change) => done(connection.call("threads.mark", { thread, change })),
		deliver: async (thread, content) => (await connection.callOpen("threads.deliver", { thread, content: [...content], key: options.randomUUID() })).delivery,
		withdraw: async (thread, delivery) => (await connection.call("threads.withdraw", { thread, delivery })).withdrawn,
		delete: (thread) => done(connection.call("threads.delete", { thread })),
		stop: (thread) => done(connection.call("threads.stop", { thread })),
		answer: (thread, key, answer) => done(connection.call("threads.answer", { thread, key, answer })),
		follow: (id) => follows.follow(id),
		readOlder: (id) => follows.readOlder(id),
		watchThreads: () =>
			watches.hold("threads", () => ({
				make: () => new WatchedThreads(),
				view: (list: WatchedThreads): ThreadsView => list.view,
				subscribe: (list, listener) => list.subscribe(listener),
				topic: { type: "threads" },
				take: (list, event) => list.take(event),
			})),
		watchSettings: () => watched<Settings>(watches, "settings", { type: "settings" }, (event) => (event.event === "settings" ? event.settings : undefined)),
		setSettings: (change) => done(connection.call("settings.set", change)),
		watchTerminals: (thread) =>
			watched<readonly TerminalListed[]>(watches, `terminals ${thread}`, { type: "terminals", thread }, (event) =>
				event.event === "terminals" ? event.terminals : undefined,
			),
		openTerminal: async (thread, size) => (await connection.call("terminals.open", { thread, size })).terminal,
		terminal: (id, size, handlers) => followTerminal(connection, subscriptions, told, id, size, handlers),
		closeTerminal: (id) => done(connection.call("terminals.close", { terminal: id })),
		watchPages: (thread) =>
			watched<readonly PageListed[]>(watches, `pages ${thread}`, { type: "pages", thread }, (event) => (event.event === "pages" ? event.pages : undefined)),
		openPage: async (thread, url, view) => (await connection.call("browser.open", { thread, view, ...(url === undefined ? {} : { url }) })).page,
		page: (id, view, handlers) => followPage(connection, subscriptions, told, id, view, handlers),
		closePage: (id) => done(connection.call("browser.close", { page: id })),
		grants: async () => (await connection.call("grants.list", {})).projects,
		revokeGrant: (granted) => done(connection.call("grants.revoke", granted)),
		providers: async () => (await connection.call("providers.list", {})).providers,
		providerStatus: async (id) => (await connection.call("providers.status", { provider: id })).standing,
		signIn: async (id) => (await connection.call("providers.sign-in", { provider: id })).url,
		finishSignIn: (id, redirect) => done(connection.call("providers.finish-sign-in", { provider: id, redirect })),
		refreshModels: () => done(connection.call("models.refresh", {})),
		setKey: (provider, key) => done(connection.call("credentials.set", { provider, api_key: key })),
		removeKey: (provider) => done(connection.call("credentials.remove", { provider })),
		addAccount: async (provider, name) => (await connection.callOpen("accounts.add", { provider, name, key: options.randomUUID() })).account,
		renameAccount: (account, name) => done(connection.call("accounts.rename", { account, name })),
		removeAccount: (account) => done(connection.call("accounts.remove", { account })),
		models: async () => (await connection.call("models.list", {})).models,
		keepFile: (thread, bytes, name) => {
			if (options.keepFile === undefined) return Promise.reject(clientError("unsupported", "This client sends the node no files."));
			return options.keepFile(thread, bytes, name);
		},
		close: () => connection.close(),
	};
}

/** A watch of `key` whose value is what `of` makes of its topic's events, the last it made: none until it made one. */
function watched<T>(watches: Watches, key: string, topic: Topic, of: (event: SubscriptionEvent) => T | undefined): Followable<T | undefined> {
	return watches.hold(key, () => ({
		make: () => new Value<T | undefined>(undefined),
		view: (value: Value<T | undefined>) => value.value,
		subscribe: (value, listener) => value.subscribe(listener),
		topic,
		take: (value, event) => {
			const made = of(event);
			if (made !== undefined) value.set(made);
		},
	}));
}
