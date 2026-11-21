import { expect, test } from "bun:test";
import {
	CALL_PATH,
	createClient,
	type Command,
	type CommandReply,
	type Listed,
	type NodeAnswer,
	type Received,
	type Scope,
	type Start,
	type Topic,
} from "@octoii-works/octoii-client";
import { ClientProvider } from "@octoii-works/octoii-client/react";
import { Window } from "happy-dom";
import { act, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { WorktreeStart } from "../src/extensions/git/thread-start.tsx";
import { isSettled, statusOf } from "../src/core/threads.ts";
import { DEVICE_DEFAULTS } from "../src/core/device.ts";
import { ThreadScreen } from "../src/screens/thread.tsx";

const start: Start = {
	request: { owner: "git", payload: { destination: "/task", branch: "task" } },
	token: "attempt",
	previous: null,
	state: { state: "pending" },
};
const unavailable = {
	...start,
	state: { state: "unavailable", directory: { workspace: "/resolved-workspace", project: "/project" } },
} satisfies Start;
const row: Listed = {
	id: "task",
	config: {},
	marks: { pinned: false, archived: false },
	status: {
		turn: "created",
		start,
		inbox: [["input", { type: "input", actor: "person", content: [{ type: "input_text", text: "Saved task" }], joins: "step" }]],
		asks: [],
		compacting: false,
	},
};
const progress = [
	["pending", "Waiting to prepare the worktree…"],
	["preparing", "Preparing worktree…"],
	["checking", "Checking worktree…"],
] as const;

test("held input is separate from Agent work throughout preparation", () => {
	for (const [state] of progress) {
		const waiting = { ...row, status: { ...row.status, start: { ...start, state: { state } } } };
		expect(statusOf(waiting.status)).toBe("preparing");
		expect(isSettled(waiting, { status: "preparing", since: 0 }, 30 * 24 * 60 * 60 * 1000)).toBe(false);
	}
	for (const state of ["failed", "interrupted"] as const) {
		expect(statusOf({ ...row.status, start: { ...start, state: { state, reason: "checkout failed" } } })).toBe("failed");
	}
	expect(statusOf({ ...row.status, start: { ...start, state: { state: "cancelled" } } })).toBe("ready");
	expect(statusOf({ ...row.status, start: unavailable })).toBe("unavailable");
});

test("renders each progress phase with cancellation while sending no RPC on open or update", async () => {
	await withStart(async ({ draw, element, button, sent }) => {
		for (const [state, title] of progress) {
			await draw({ ...start, state: { state } });
			expect(element.textContent).toContain(title);
			expect(element.textContent).toContain("Saved task");
			expect(button("Cancel task")?.disabled).toBe(false);
			expect(element.querySelectorAll("button")).toHaveLength(1);
		}
		expect(sent).toEqual([]);
	});
});

test("cancels the observed attempt and waits for an explicit action after a stale stop", async () => {
	await withStart(async ({ draw, element, click, sent }) => {
		await draw({ ...start, state: { state: "preparing" } });
		await click("Cancel task");
		const first = sent[0];
		if (first === undefined) throw new Error("Missing cancellation");
		expect(first).toMatchObject({ method: "threads.stop", params: { thread: row.id, target: { kind: "preparation", token: start.token } } });
		await first.respond({ error: { code: "stale_stop", message: "The preparation attempt changed." } });
		expect(element.querySelector('[role="alert"]')?.textContent).toBe("The preparation attempt changed.");
		expect(sent).toHaveLength(1);
		await draw({ ...start, state: { state: "checking" }, token: "next", previous: start.token });
		expect(element.querySelector('[role="alert"]')).toBeNull();
		expect(sent).toHaveLength(1);
		await click("Cancel task");
		const second = sent[1];
		if (second === undefined) throw new Error("Missing cancellation of the new token");
		expect(second).toMatchObject({ method: "threads.stop", params: { thread: row.id, target: { kind: "preparation", token: "next" } } });
		await second.respond({ result: {} });
	});
});

test("retries a displayed failure and follows the subscription after the acknowledgement", async () => {
	await withStart(async ({ draw, element, button, click, sent }) => {
		await draw({ ...start, state: { state: "failed", reason: "branch exists" } });
		expect(element.textContent).toContain("branch exists");
		expect(sent).toEqual([]);
		await click("Retry preparation");
		const request = sent[0];
		if (request === undefined) throw new Error("Missing retry request");
		expect(request).toMatchObject({ method: "git.retry-start", params: { thread: row.id, token: start.token } });
		expect(button("Retry preparation")?.disabled).toBe(true);
		const retried = { ...start, token: "retried", previous: start.token };
		await request.respond({ result: retried });
		expect(element.textContent).toContain("branch exists");
		await draw({ ...retried, state: { state: "preparing" } });
		expect(element.textContent).toContain("Preparing worktree…");
		expect(button("Retry preparation")).toBeUndefined();
		expect(button("Cancel task")?.disabled).toBe(false);
		expect(sent).toHaveLength(1);
	});
});

test("shows interruption diagnostics and permits cancellation with thread permission", async () => {
	await withStart(
		async ({ draw, element, button, click, sent }) => {
			await draw({ ...start, state: { state: "interrupted", reason: "checkout outcome unknown" } });
			expect(element.textContent).toContain("Workspace preparation interrupted");
			expect(element.textContent).toContain("checkout outcome unknown");
			expect(button("Retry preparation")).toBeUndefined();
			expect(sent).toEqual([]);
			await click("Cancel task");
			const request = sent[0];
			if (request === undefined) throw new Error("Missing cancellation");
			expect(request).toMatchObject({ method: "threads.stop", params: { thread: row.id, target: { kind: "preparation", token: start.token } } });
			await request.respond({ result: {} });
		},
		["threads.operate"],
	);
});

test("shows the saved message when cancelled and releases the conversation when ready", async () => {
	await withStart(async ({ draw, element, sent }) => {
		await draw({ ...start, state: { state: "cancelled" } });
		expect(element.textContent).toContain("Preparation cancelled");
		expect(element.textContent).toContain("Saved task");
		expect(element.querySelector("button")).toBeNull();
		await draw({ ...start, state: { state: "ready", directory: { workspace: "/resolved-workspace", project: "/project" } } });
		expect(element.querySelector('[aria-label="Worktree preparation"]')).toBeNull();
		expect(element.querySelector("button")).toBeNull();
		expect(sent).toEqual([]);
	});
});

test("shows the unavailable workspace and saved input without preparation actions", async () => {
	await withStart(async ({ draw, element, sent }) => {
		await draw(unavailable);
		expect(element.textContent).toContain("Workspace unavailable");
		expect(element.textContent).toContain("/resolved-workspace");
		expect(element.textContent).toContain("Restore the original worktree");
		expect(element.textContent).toContain("Saved task");
		expect(element.querySelector("button")).toBeNull();
		expect(sent).toEqual([]);
	});
});

test("only offers to stop the active turn in an unavailable workspace", async () => {
	await withStart(async ({ draw, element, button, click, sent }) => {
		await draw(unavailable, "active-turn");
		expect(element.querySelectorAll("button")).toHaveLength(1);
		expect(button("Stop turn")?.disabled).toBe(false);
		await click("Stop turn");
		const request = sent[0];
		if (request === undefined) throw new Error("Missing turn stop request");
		expect(request).toMatchObject({ method: "threads.stop", params: { thread: row.id, target: { kind: "turn", id: "active-turn" } } });
		await request.respond({ result: {} });
		await draw(unavailable);
		expect(element.querySelector("button")).toBeNull();
		expect(sent).toHaveLength(1);
	});
});

test("keeps the thread's history visible and removes the composer while its workspace is unavailable", async () => {
	await withClientView(async ({ draw, element, button, sent, watches, receive }) => {
		await draw(<ThreadScreen thread={row.id} device={DEVICE_DEFAULTS} />);
		expect(sent.find(({ method }) => method === "threads.read")).toBeUndefined();
		const watch = watches.find(({ params }) => params.topic?.type === "thread");
		if (watch === undefined) throw new Error("Missing thread subscription");
		expect(watch.params).toEqual({ topic: { type: "thread", thread: row.id, limit: 500 } });
		for (const { id, method } of watches) {
			if (method === "subscribe") await receive({ id, result: { subscription: id } });
		}
		const ready = { ...row.status, inbox: [], turn: "replied" as const, start: { ...start, state: { ...unavailable.state, state: "ready" as const } } };
		await receive({
			event: "thread_snapshot",
			subscription: watch.id,
			sequence: "1",
			status: ready,
			history: {
				complete: false,
				entries: [
					{ type: "input", id: "input-1", actor: "person", delivery: "first", content: [{ type: "input_text", text: "Earlier message" }] },
					{ type: "turn-end", id: "end-1", turn: "turn-1", outcome: "replied" },
				],
			},
		});
		expect(element.querySelector('textarea[aria-label="Message"]')).not.toBeNull();
		await receive({
			event: "thread_update",
			subscription: watch.id,
			sequence: "2",
			entries: [],
			status: { ...ready, inbox: row.status.inbox, start: unavailable },
		});
		expect(element.querySelector("header h2")?.textContent).toBe("Earlier message");
		const history = button("Show earlier");
		expect(history).toBeDefined();
		expect(history?.closest(".hidden")).toBeNull();
		expect(element.querySelector('[aria-label="Workspace status"]')?.textContent).toContain("Saved task");
		expect(element.querySelector('textarea[aria-label="Message"]')).toBeNull();
		await receive({ event: "thread_update", subscription: watch.id, sequence: "3", entries: [], status: ready });
		expect(element.querySelector('textarea[aria-label="Message"]')).not.toBeNull();
	});
});

type WatchRequest = { id: string; method: string; params: { topic?: Topic } };
type SentCommand = Command & { respond: (reply: CommandReply) => Promise<void> };
type ClientView = {
	readonly element: HTMLElement;
	readonly draw: (children: ReactNode) => Promise<void>;
	readonly button: (text: string) => HTMLButtonElement | undefined;
	readonly click: (text: string) => Promise<void>;
	readonly sent: readonly SentCommand[];
	readonly watches: readonly WatchRequest[];
	readonly receive: (message: Received) => Promise<void>;
};
type StartView = Omit<ClientView, "draw"> & { readonly draw: (value: Start, turn?: string) => Promise<void> };

function withStart(run: (view: StartView) => Promise<void>, scopes?: Scope[]): Promise<void> {
	return withClientView(
		({ draw, ...view }) =>
			run({
				...view,
				draw: (value, turn) => {
					const status = { ...row.status, start: value, ...(turn === undefined ? {} : { turn: "running" as const, turn_id: turn }) };
					return draw(<WorktreeStart thread={row.id} status={status} />);
				},
			}),
		scopes,
	);
}

async function withClientView(run: (view: ClientView) => Promise<void>, scopes: Scope[] = ["threads.operate", "git.operate"]): Promise<void> {
	const dom = new Window({ url: "http://localhost/" });
	const globals = {
		window: dom,
		document: dom.document,
		Element: dom.Element,
		HTMLElement: dom.HTMLElement,
		Node: dom.Node,
		Document: dom.Document,
		ResizeObserver: dom.ResizeObserver,
		MutationObserver: dom.MutationObserver,
		getComputedStyle: dom.getComputedStyle.bind(dom),
		requestAnimationFrame: dom.requestAnimationFrame.bind(dom),
		cancelAnimationFrame: dom.cancelAnimationFrame.bind(dom),
		IS_REACT_ACT_ENVIRONMENT: true,
	};
	const previous = new Map(Object.keys(globals).map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
	for (const [key, value] of Object.entries(globals)) Object.defineProperty(globalThis, key, { configurable: true, writable: true, value });
	const element = document.createElement("div");
	document.body.append(element);
	const root = createRoot(element);
	const sent: SentCommand[] = [];
	const watches: WatchRequest[] = [];
	let serial = 0;
	let receive = (_answer: Received): void => undefined;
	const client = createClient({
		open: () => ({
			binaryType: "arraybuffer",
			send: (data) => watches.push(JSON.parse(data)),
			close: () => undefined,
			addEventListener: (type, listener) => {
				if (type === "message") receive = (answer) => listener({ data: JSON.stringify(answer) });
			},
		}),
		send: (request) => {
			expect(request.path).toBe(CALL_PATH);
			const command: Command = JSON.parse(String(request.body));
			return new Promise<NodeAnswer>((resolve) => {
				sent.push({
					...command,
					respond: async (reply) => {
						await act(async () =>
							resolve({
								status: "error" in reply ? 422 : 200,
								contentType: "application/json",
								bytes: new TextEncoder().encode(JSON.stringify(reply)),
							}),
						);
					},
				});
			});
		},
		randomUUID: () => `fixture-${String(++serial)}`,
		clock: { now: () => 0, setTimer: () => () => undefined },
		report: (error) => {
			throw error;
		},
	});
	try {
		receive({
			event: "welcome",
			session: "session",
			scopes,
			machine: { name: "fixture", workspace: "/work", home: null, scratchpads: null, memory: null },
			extensions: [],
		});
		const button = (text: string): HTMLButtonElement | undefined => [...element.querySelectorAll("button")].find((each) => each.textContent === text);
		await run({
			element,
			sent,
			watches,
			draw: async (children) => {
				await act(() => {
					root.render(
						<ClientProvider
							client={client}
							frame={(publish) => {
								queueMicrotask(publish);
								return () => undefined;
							}}
						>
							{children}
						</ClientProvider>,
					);
				});
			},
			button,
			click: async (text) => {
				const target = button(text);
				if (target === undefined) throw new Error(`No button named ${text}`);
				await act(() => target.click());
			},
			receive: async (message) => {
				await act(async () => receive(message));
			},
		});
	} finally {
		await act(() => {
			root.unmount();
			client.close();
		});
		await dom.happyDOM.close();
		for (const [key, descriptor] of previous) {
			if (descriptor === undefined) Reflect.deleteProperty(globalThis, key);
			else Object.defineProperty(globalThis, key, descriptor);
		}
	}
}
