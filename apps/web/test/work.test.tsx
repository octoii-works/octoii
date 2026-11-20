/** Work folds preserve the reader's choices, expose delivered files, and keep script source independent of nested tools. */
import { describe, expect, it } from "bun:test";
import { createClient } from "@octoii-works/octoii-client";
import { ClientProvider } from "@octoii-works/octoii-client/react";
import { Window } from "happy-dom";
import { act, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { type Reach, ReachContext } from "../src/components/state/reach.ts";
import { DetailList } from "../src/components/thread/details.tsx";
import { Calls } from "../src/components/thread/calls.tsx";
import { TurnWork } from "../src/components/thread/turn.tsx";
import type { WorkGroup } from "../src/core/rows.ts";
import type { CallLine } from "../src/core/timeline.ts";

const code = "await tools.read({ path: 'SCRIPT_SOURCE_ONLY' });";
const finalOutput = "SCRIPT_FINAL_OUTPUT";
const nestedOutput = "NESTED_TOOL_OUTPUT";

function callOf(id: string, fields: Partial<CallLine> = {}): CallLine {
	return {
		kind: "call",
		id,
		key: `key-${id}`,
		step: "step",
		thread: undefined,
		tool: "custom.report",
		work: "other",
		verb: undefined,
		subject: "Child report",
		code: false,
		aside: undefined,
		trouble: undefined,
		arguments: "{}",
		output: nestedOutput,
		files: [],
		structuredContent: undefined,
		began: undefined,
		ended: undefined,
		state: "done",
		ask: undefined,
		calls: [],
		...fields,
	};
}

function workOf(fields: Partial<WorkGroup> = {}): WorkGroup {
	return { parts: [{ kind: "step", step: { kind: "words", key: "note", text: "Work in progress" } }], working: true, label: "Working", ...fields };
}

const reach: Reach = {
	open: () => {
		throw new Error("A rendered file needs no socket");
	},
	send: async () => ({ status: 200, contentType: "application/octet-stream", bytes: new Uint8Array() }),
	linkExample: "",
	readsLink: () => false,
	pair: async () => undefined,
};

type View = {
	readonly element: HTMLElement;
	readonly draw: (children: ReactNode) => Promise<void>;
	readonly button: (text: string) => HTMLButtonElement;
	readonly click: (button: HTMLButtonElement) => Promise<void>;
};

/** Renders real components and Base UI controls against a local DOM; the client stays disconnected. */
async function withView(run: (view: View) => Promise<void>): Promise<void> {
	const dom = new Window({ url: "http://localhost/" });
	const globals = {
		window: dom,
		document: dom.document,
		Element: dom.Element,
		HTMLElement: dom.HTMLElement,
		Node: dom.Node,
		Document: dom.Document,
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
	const client = createClient({
		open: () => ({ binaryType: "arraybuffer", send: () => undefined, close: () => undefined, addEventListener: () => undefined }),
		send: reach.send,
		randomUUID: () => "fixture",
		clock: { now: () => 0, setTimer: () => () => undefined },
		report: (error) => {
			throw error;
		},
	});
	try {
		await run({
			element,
			draw: async (children) => {
				await act(() => {
					root.render(
						<ClientProvider client={client} frame={() => () => undefined}>
							<ReachContext value={reach}>{children}</ReachContext>
						</ClientProvider>,
					);
				});
			},
			button: (text) => {
				const button = [...element.querySelectorAll("button")].find((each) => each.textContent?.includes(text));
				if (button === undefined) throw new Error(`No button named ${text}`);
				return button;
			},
			click: async (button) => {
				await act(() => button.click());
			},
		});
	} finally {
		await act(() => root.unmount());
		client.close();
		await dom.happyDOM.close();
		for (const [key, descriptor] of previous) {
			if (descriptor === undefined) Reflect.deleteProperty(globalThis, key);
			else Object.defineProperty(globalThis, key, descriptor);
		}
	}
}

describe("a turn's work fold", () => {
	it("opens running work and automatically collapses it when the turn completes", async () => {
		await withView(async ({ draw, element, button }) => {
			await draw(<TurnWork thread="thread" work={workOf()} />);
			expect(button("Working").getAttribute("aria-expanded")).toBe("true");
			expect(element.textContent).toContain("Work in progress");
			await draw(<TurnWork thread="thread" work={workOf({ working: false, label: "Worked for 8s" })} />);
			expect(button("Worked for 8s").getAttribute("aria-expanded")).toBe("false");
		});
	});

	it("keeps a reader's manual choice as more work arrives and as the turn completes", async () => {
		await withView(async ({ draw, element, button, click }) => {
			await draw(<TurnWork thread="thread" work={workOf()} />);
			await click(button("Working"));
			await draw(<TurnWork thread="thread" work={workOf({ parts: [{ kind: "step", step: { kind: "words", key: "note", text: "More work" } }] })} />);
			expect(button("Working").getAttribute("aria-expanded")).toBe("false");
			await click(button("Working"));
			await draw(<TurnWork thread="thread" work={workOf({ working: false, label: "Worked for 8s" })} />);
			expect(button("Worked for 8s").getAttribute("aria-expanded")).toBe("true");
			expect(element.textContent).toContain("Work in progress");
		});
	});

	it("starts completed work closed and keeps delivered files visible exactly once in either state", async () => {
		await withView(async ({ draw, element, button, click }) => {
			const call = callOf("delivery", {
				files: [{ type: "resource_link", file: { ref: "sha256:report", media_type: "application/pdf", filename: "result.pdf" }, size: 42, title: "Report" }],
			});
			await draw(<TurnWork thread="thread" work={workOf({ working: false, label: "Worked for 8s", parts: [{ kind: "step", step: call }] })} />);
			expect(button("Worked for 8s").getAttribute("aria-expanded")).toBe("false");
			expect(element.textContent).not.toContain("Child report");
			expect(element.querySelectorAll('[aria-label="More actions for result.pdf"]')).toHaveLength(1);
			await click(button("Worked for 8s"));
			expect(element.textContent).toContain("Child report");
			expect(element.querySelectorAll('[aria-label="More actions for result.pdf"]')).toHaveLength(1);
		});
	});
});

describe("expanded tool details", () => {
	it("keeps execute output and source collapsed and independently toggled while nested tools use their own guides", async () => {
		await withView(async ({ draw, element, button, click }) => {
			const child = callOf("nested");
			const script = callOf("execute", {
				tool: "execute",
				work: "script",
				subject: undefined,
				arguments: JSON.stringify({ code }),
				output: finalOutput,
				calls: [child],
			});
			await draw(<Calls thread="thread" steps={[script]} calls={[script]} />);
			expect(element.textContent).not.toContain(finalOutput);
			expect(element.textContent).not.toContain("SCRIPT_SOURCE_ONLY");
			await click(button("used 1 tool"));
			expect(button("Output").getAttribute("aria-expanded")).toBe("false");
			expect(element.textContent).not.toContain(finalOutput);
			expect(button("Source code").getAttribute("aria-expanded")).toBe("false");
			expect(element.textContent).not.toContain("SCRIPT_SOURCE_ONLY");
			await click(button("custom.report"));
			expect(element.textContent).toContain(nestedOutput);
			expect(element.querySelectorAll('[data-slot="work-branch"] [data-slot="work-branch"]')).toHaveLength(1);
			await click(button("Source code"));
			expect(element.textContent).toContain("SCRIPT_SOURCE_ONLY");
			expect(button("custom.report").getAttribute("aria-expanded")).toBe("true");
			expect(element.textContent).not.toContain(finalOutput);
			await click(button("Output"));
			expect(element.textContent?.match(new RegExp(finalOutput, "g"))).toHaveLength(1);
			await click(button("Output"));
			expect(button("Output").getAttribute("aria-expanded")).toBe("false");
			await click(button("Source code"));
			expect(button("Source code").getAttribute("aria-expanded")).toBe("false");
			expect(button("custom.report").getAttribute("aria-expanded")).toBe("true");
			expect(element.textContent).toContain(nestedOutput);
		});
	});
});

describe("thread details lists", () => {
	it("keeps extra tasks collapsed, reveals them, and folds them again", async () => {
		await withView(async (view) => {
			const rows = ["Explorer", "Tests", "Review", "Build", "Docs"].map((name) => <button key={name}>{name}</button>);
			await view.draw(<DetailList label="tasks">{rows}</DetailList>);
			expect(view.element.textContent).not.toContain("Build");
			expect(view.button("Show 2 more tasks").getAttribute("aria-expanded")).toBe("false");
			await view.click(view.button("Show 2 more tasks"));
			expect(view.element.textContent).toContain("Build");
			expect(view.element.textContent).toContain("Docs");
			expect(view.button("Show less").getAttribute("aria-expanded")).toBe("true");
			await view.click(view.button("Show less"));
			expect(view.element.textContent).not.toContain("Build");
			await view.draw(<DetailList label="tasks">{rows.slice(0, 2)}</DetailList>);
			expect(view.element.textContent).not.toContain("Show");
			expect(view.element.querySelectorAll("button").length).toBe(2);
		});
	});
});
