import { expect, test } from "bun:test";
import { createClient, type Listed, type Received, type Scope, type Start } from "@octoii-works/octoii-client";
import { ClientProvider } from "@octoii-works/octoii-client/react";
import { Window } from "happy-dom";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { WorktreeStart } from "../src/extensions/git/thread-start.tsx";
import { isSettled, statusOf } from "../src/core/threads.ts";

const start: Start = {
	request: { owner: "git.worktree", payload: { destination: "/task", branch: "task" } },
	token: "attempt",
	previous: null,
	state: { state: "pending" },
};
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
		expect(statusOf(waiting)).toBe("preparing");
		expect(isSettled(waiting, { status: "preparing", since: 0 }, 30 * 24 * 60 * 60 * 1000)).toBe(false);
	}
	for (const state of ["failed", "interrupted"] as const) {
		expect(statusOf({ ...row, status: { ...row.status, start: { ...start, state: { state, reason: "checkout failed" } } } })).toBe("failed");
	}
	expect(statusOf({ ...row, status: { ...row.status, start: { ...start, state: { state: "cancelled" } } } })).toBe("ready");
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
	await withStart(async ({ draw, element, click, sent, receive }) => {
		await draw({ ...start, state: { state: "preparing" } });
		await click("Cancel task");
		const first = sent[0];
		if (first === undefined) throw new Error("Missing cancellation");
		expect(first).toMatchObject({ method: "threads.stop", params: { thread: row.id, target: { kind: "preparation", token: start.token } } });
		await receive({ id: first.id, error: { code: "stale_stop", message: "The preparation attempt changed." } });
		expect(element.querySelector('[role="alert"]')?.textContent).toBe("The preparation attempt changed.");
		expect(sent).toHaveLength(1);
		await draw({ ...start, state: { state: "checking" }, token: "next", previous: start.token });
		expect(element.querySelector('[role="alert"]')).toBeNull();
		expect(sent).toHaveLength(1);
		await click("Cancel task");
		const second = sent[1];
		if (second === undefined) throw new Error("Missing cancellation of the new token");
		expect(second).toMatchObject({ method: "threads.stop", params: { thread: row.id, target: { kind: "preparation", token: "next" } } });
		await receive({ id: second.id, result: {} });
	});
});

test("retries a displayed failure and follows the subscription after the acknowledgement", async () => {
	await withStart(async ({ draw, element, button, click, sent, receive }) => {
		await draw({ ...start, state: { state: "failed", reason: "branch exists" } });
		expect(element.textContent).toContain("branch exists");
		expect(sent).toEqual([]);
		await click("Retry preparation");
		const request = sent[0];
		if (request === undefined) throw new Error("Missing retry request");
		expect(request).toMatchObject({ method: "git.retry-start", params: { thread: row.id, token: start.token } });
		expect(button("Retry preparation")?.disabled).toBe(true);
		const retried = { ...start, token: "retried", previous: start.token };
		await receive({ id: request.id, result: retried });
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
		async ({ draw, element, button, click, sent, receive }) => {
			await draw({ ...start, state: { state: "interrupted", reason: "checkout outcome unknown" } });
			expect(element.textContent).toContain("Workspace preparation interrupted");
			expect(element.textContent).toContain("checkout outcome unknown");
			expect(button("Retry preparation")).toBeUndefined();
			expect(sent).toEqual([]);
			await click("Cancel task");
			const request = sent[0];
			if (request === undefined) throw new Error("Missing cancellation");
			expect(request).toMatchObject({ method: "threads.stop", params: { thread: row.id, target: { kind: "preparation", token: start.token } } });
			await receive({ id: request.id, result: {} });
		},
		["threads.operate"],
	);
});

test("shows the saved message when cancelled and the resolved directory when ready", async () => {
	await withStart(async ({ draw, element, sent }) => {
		await draw({ ...start, state: { state: "cancelled" } });
		expect(element.textContent).toContain("Preparation cancelled");
		expect(element.textContent).toContain("Saved task");
		expect(element.querySelector("button")).toBeNull();
		await draw({ ...start, state: { state: "ready", directory: { workspace: "/resolved-workspace", project: "/project" } } });
		expect(element.textContent).toContain("Workspace ready");
		expect(element.textContent).toContain("/resolved-workspace");
		expect(element.querySelector("button")).toBeNull();
		expect(sent).toEqual([]);
	});
});

type Request = { id: string; method: string; params: unknown };
type StartView = {
	readonly element: HTMLElement;
	readonly draw: (value: Start) => Promise<void>;
	readonly button: (text: string) => HTMLButtonElement | undefined;
	readonly click: (text: string) => Promise<void>;
	readonly sent: readonly Request[];
	readonly receive: (message: Received) => Promise<void>;
};

async function withStart(run: (view: StartView) => Promise<void>, scopes: Scope[] = ["threads.operate", "git.operate"]): Promise<void> {
	const dom = new Window({ url: "http://localhost/" });
	const globals = { window: dom, document: dom.document, IS_REACT_ACT_ENVIRONMENT: true };
	const previous = new Map(Object.keys(globals).map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
	for (const [key, value] of Object.entries(globals)) Object.defineProperty(globalThis, key, { configurable: true, writable: true, value });
	const element = document.createElement("div");
	document.body.append(element);
	const root = createRoot(element);
	const sent: Request[] = [];
	let receive = (_answer: Received): void => undefined;
	const client = createClient({
		open: () => ({
			binaryType: "arraybuffer",
			send: (data) => sent.push(JSON.parse(data)),
			close: () => undefined,
			addEventListener: (type, listener) => {
				if (type === "message") receive = (answer) => listener({ data: JSON.stringify(answer) });
			},
		}),
		randomUUID: () => "fixture",
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
			draw: async (value) => {
				await act(() => {
					root.render(
						<ClientProvider client={client} frame={() => () => undefined}>
							<WorktreeStart thread={row.id} start={value} inbox={row.status.inbox} />
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
