/**
 * A thread's timeline, read from the conversation the client puts together: each turn's words, its work — calls by
 * their tools, words on the way — its answer, the files it changed, what streams while it works, and how and when it
 * ended; a turn that ended read once, a call's line kept while its call is; and its work as the page lays it out.
 */
import { describe, expect, it } from "bun:test";
import { type Ask, conversationOf, type Entry, type Item, type Streamed, type Structured, type ThreadView } from "@octoii-works/octoii-client";
import {
	type CallLine,
	declinedText,
	doingOf,
	firstSaid,
	laidOut,
	leadOf,
	runsOf,
	thoughtCalls,
	replyOf,
	itemsOf,
	type TimelineTurn,
	waitsOnPerson,
	type Work,
	workedText,
} from "../src/core/timeline.ts";

/** The turns among the items of `conversation`, in their order. */
const turnsOf = (conversation: Parameters<typeof itemsOf>[0]): readonly TimelineTurn[] =>
	itemsOf(conversation).flatMap((item) => (item.kind === "turn" ? [item.turn] : []));

/** A call's line of `tool`, made at the step `step`, done. */
const callOf = (id: string, tool: string, step: string): CallLine => ({
	kind: "call",
	id,
	key: `k${id}`,
	step,
	thread: undefined,
	tool,
	work: "other",
	verb: undefined,
	subject: undefined,
	code: true,
	aside: undefined,
	trouble: undefined,
	arguments: "{}",
	output: undefined,
	structured: undefined,
	began: undefined,
	ended: undefined,
	state: "done",
	ask: undefined,
	calls: [],
});

/** An id minted at `ms`, as a UUIDv7 holds its time in its first 48 bits. */
function idAt(ms: number, serial: number): string {
	const time = ms.toString(16).padStart(12, "0");
	return `${time.slice(0, 8)}-${time.slice(8)}-7000-8000-${String(serial).padStart(12, "0")}`;
}

const START = 1_700_000_000_000;
const id = (seconds: number, serial: number): string => idAt(START + seconds * 1000, serial);
const text = (words: string) => [{ type: "input_text" as const, text: words }];
const said = (at: string, words: string): Entry => ({ id: at, type: "input", delivery: `d${at}`, actor: "person", content: text(words) });
const words = (written: string): Item => ({ type: "message", role: "assistant", content: [{ type: "output_text", text: written }] });
const call = (callId: string, name: string, args: unknown): Item => ({ type: "function_call", call_id: callId, name, arguments: JSON.stringify(args) });
/** The keys Siphon gives the calls `output` made: `k` and the model's id, as the tests' results name them. */
const keysOf = (output: Item[]) => output.flatMap((item) => (item.type === "function_call" ? [{ call_id: item.call_id, key: `k${item.call_id}` }] : []));
const reply = (at: string, request: string, output: Item[]): Entry => ({
	id: at,
	type: "reply",
	request,
	response: { id: "r", model: "m", status: "completed", output },
	calls: keysOf(output),
});
const result = (at: string, callId: string, status: "ok" | "tool-error" | "rejected", output = "…", structured?: Structured): Entry => ({
	id: at,
	type: "result",
	call: `k${callId}`,
	status,
	output: { call_id: callId, output: text(output) },
	...(structured === undefined ? {} : { structured }),
});
const end = (at: string, outcome: "replied" | "failed" | "stopped", reason?: string): Entry => ({
	id: at,
	type: "turn-end",
	outcome,
	...(reason === undefined ? {} : { reason }),
});
/** What a call asks the person, by the model's id of it, minted at `key`. */
const askOf = (key: string, name: string, payload: Record<string, unknown>): Ask => ({
	process: "p",
	key,
	name,
	payload: payload as Ask["payload"],
	answer_schema: {},
});

function viewOf(entries: Entry[], working = false, asks: Ask[] = [], streaming: [string, Streamed[]][] = []): ThreadView {
	return {
		entries,
		read: true,
		complete: true,
		status: { turn: working ? "running" : "replied", inbox: [], asks, compacting: false },
		streaming: new Map(streaming),
		gone: false,
	};
}

describe("a thread's timeline", () => {
	it("stands every row where its id puts it: a compaction within a turn among its work, one between turns and an ask a row of its own", () => {
		const recap = (at: string): Entry => ({
			id: at,
			type: "compaction",
			request: "qr",
			response: { id: "r", model: "m", status: "completed", output: [words("Recap.")] },
			kept: at,
			from: at,
		});
		const items = itemsOf(
			conversationOf(
				viewOf(
					[
						said(id(0, 1), "Build it"),
						recap(id(1, 2)),
						reply(id(2, 3), "q1", [words("Built.")]),
						end(id(3, 4), "replied"),
						recap(id(4, 5)),
						said(id(5, 6), "And test it"),
						reply(id(6, 7), "q2", [words("Tested.")]),
						end(id(7, 8), "replied"),
						recap(id(9, 10)),
					],
					false,
					[askOf(id(8, 9), "siphon.trust", { folder: "/w", servers: [] })],
				),
			),
		);
		expect(items.map((item) => item.kind)).toEqual(["turn", "summed-up", "turn", "ask", "summed-up"]);
		const [within] = items;
		expect(within?.kind === "turn" ? within.turn.work.map((each) => each.kind) : []).toEqual(["compaction"]);
		// A turn is known by the entry it began with, which it has from the first.
		expect(items.map((item) => item.key).slice(1)).toEqual([id(4, 5), id(5, 6), id(8, 9), id(9, 10)]);
	});

	it("gives an ended turn's words, its work by its tools, the words on the way, its answer, and how long it took", () => {
		const [turn] = turnsOf(
			conversationOf(
				viewOf([
					said(id(0, 1), "Fix the build"),
					reply(id(2, 2), "q1", [words("Let me look."), call("c1", "read", { path: "build.zig" }), call("c2", "shell", { command: "zig build" })]),
					result(id(3, 3), "c1", "ok"),
					result(id(4, 4), "c2", "tool-error"),
					reply(id(5, 5), "q2", [words("Fixed it.")]),
					end(id(12, 6), "replied"),
				]),
			),
		);
		expect(turn?.key).toBe(id(0, 1));
		expect(turn?.said.map((each) => each.text)).toEqual(["Fix the build"]);
		expect(turn?.work).toMatchObject([
			{ kind: "words", text: "Let me look." },
			{ kind: "call", tool: "read", work: "read", subject: "build.zig", state: "done" },
			{ kind: "call", tool: "shell", work: "command", subject: "zig build", state: "failed" },
		]);
		expect(turn?.answer?.text).toBe("Fixed it.");
		expect(turn?.ending).toEqual({ state: "replied" });
		expect([turn?.startedAt, turn?.endedAt]).toEqual([START, START + 12_000]);
		expect(turn === undefined ? undefined : workedText(turn)).toBe("Worked for 12s");
		// Each call keeps its step, its arguments and what the model was told, the calls of one reply sharing the step;
		// it began as the reply that made it was written.
		expect(turn?.work).toMatchObject([
			{ kind: "words" },
			{ step: "q1", arguments: JSON.stringify({ path: "build.zig" }), output: "…", began: START + 2_000, ended: START + 3_000 },
			{ step: "q1", output: "…", ended: START + 4_000 },
		]);
	});

	it("gives a working turn's calls as they run and the reply being written", () => {
		const [turn] = turnsOf(
			conversationOf(
				viewOf(
					[said(id(0, 1), "Run it"), reply(id(1, 2), "q1", [call("c1", "shell", { command: "make" })])],
					true,
					[],
					[
						[
							"q2",
							[
								{ type: "delta", index: 0, part: "text", text: "Hel" },
								{ type: "delta", index: 0, part: "text", text: "lo" },
								{ type: "began", index: 1, item: { type: "function_call", call_id: "c3", name: "grep", arguments: "" } },
							],
						],
					],
				),
			),
		);
		expect(turn?.ending).toEqual({ state: "working" });
		expect(turn?.work).toMatchObject([{ kind: "call", tool: "shell", subject: "make", state: "running" }]);
		expect(turn?.writing).toEqual({ request: "q2", words: "Hello", calls: [{ id: "c3", tool: "grep" }] });
		// The reply shows the words being written as they are.
		expect(turn === undefined ? undefined : replyOf(turn)).toBe("Hello");
	});

	it("gives a stopped turn's calls that came to no result as stopped, and a failed turn's reason", () => {
		const [stopped, failed] = turnsOf(
			conversationOf(
				viewOf([
					said(id(0, 1), "Search"),
					reply(id(1, 2), "q1", [call("c1", "grep", { pattern: "TODO" })]),
					end(id(2, 3), "stopped"),
					said(id(3, 4), "Again"),
					{
						id: id(4, 5),
						type: "reply",
						request: "q2",
						response: { id: "r", model: "m", status: "failed", output: [], failure: { kind: "server", message: "The provider is down." } },
					},
					end(id(5, 6), "failed", "The model could not be asked."),
				]),
			),
		);
		expect(stopped?.work).toMatchObject([{ kind: "call", subject: "TODO", state: "stopped" }]);
		expect(stopped?.ending).toEqual({ state: "stopped" });
		expect(stopped === undefined ? undefined : workedText(stopped)).toBe("Stopped after 2s");
		expect(failed?.work).toMatchObject([{ kind: "failure", text: "The provider is down." }]);
		expect(failed?.ending).toEqual({ state: "failed", why: "The model could not be asked." });
	});

	it("marks a call that waits for the person's approval, one they rejected, and what a question asks about", () => {
		const [turn] = turnsOf(
			conversationOf(
				viewOf(
					[
						said(id(0, 1), "Clean up"),
						reply(id(1, 2), "q1", [
							call("c1", "shell", { command: "rm -rf build" }),
							call("c2", "question", { questions: [{ question: "Which?", header: "Target" }] }),
						]),
						result(id(2, 3), "c1", "rejected"),
						reply(id(3, 4), "q2", [call("c3", "write", { path: "a.txt", content: "a" })]),
					],
					true,
					[askOf(id(4, 5), "siphon.permission", { call: "kc3" })],
				),
			),
		);
		expect(turn?.work).toMatchObject([
			{ kind: "call", tool: "shell", state: "rejected" },
			{ kind: "call", tool: "question", work: "ask", subject: "Target", state: "running" },
			{ kind: "call", tool: "write", state: "waiting" },
		]);
		// A turn whose call waits on the person says so, rather than that it works.
		expect(turn === undefined ? undefined : [waitsOnPerson(turn), doingOf(turn)]).toEqual([true, "Waiting for you"]);
	});

	it("shows a turn the person steered as one from each of their words on, each where it was said", () => {
		const turns = turnsOf(
			conversationOf(
				viewOf([
					said(id(0, 1), "Fix the build"),
					reply(id(2, 2), id(1, 10), [call("c1", "shell", { command: "zig build" })]),
					result(id(3, 3), "c1", "ok"),
					said(id(4, 4), "Also run the tests"),
					reply(id(6, 5), id(5, 11), [words("Built and tested.")]),
					end(id(8, 6), "replied"),
				]),
			),
		);
		expect(turns.map((turn) => turn.said.map((each) => each.text))).toEqual([["Fix the build"], ["Also run the tests"]]);
		expect(turns.map((turn) => turn.ending.state)).toEqual(["steered", "replied"]);
		expect(turns[0]?.work).toMatchObject([{ kind: "call", tool: "shell", state: "done" }]);
		expect(turns[0]?.answer).toBeUndefined();
		expect(turns[0]?.endedAt).toBe(START + 4000);
		expect(turns[1]?.answer?.text).toBe("Built and tested.");
		expect(turns[1]?.startedAt).toBe(START + 4000);
		expect(turns.map((turn) => turn.key)).toEqual([id(0, 1), id(4, 4)]);
	});

	it("gives whose words a turn took: a thread's task by its text, what a sub-thread told by its words after Siphon's note, the person's as theirs", () => {
		const note = { type: "input_text" as const, text: "[the sub-thread C, which you gave a task in the background, finished]" };
		const turns = turnsOf(
			conversationOf(
				viewOf([
					{ id: id(0, 1), type: "input", delivery: "d1", actor: "P", content: text("Check the docs."), reply_to: { process: "p", key: "k" } },
					reply(id(1, 2), "q1", [call("c1", "delegate", { thread: "C", task: "Also the links." })]),
					result(id(2, 3), "c1", "ok"),
					reply(id(3, 4), "q2", [words("Asked.")]),
					end(id(4, 5), "replied"),
					{ id: id(5, 6), type: "input", delivery: "d2", actor: "C", content: [note, { type: "input_text", text: "The docs build." }] },
					{ id: id(5, 7), type: "input", delivery: "d3", actor: "D", content: [note] },
					said(id(5, 8), "Thanks"),
					reply(id(6, 9), "q3", [words("Both done.")]),
					end(id(7, 10), "replied"),
				]),
			),
		);
		expect(turns[0]?.said.map(({ from, text: words }) => ({ from, text: words }))).toEqual([{ from: "P", text: "Check the docs." }]);
		expect(turns[0]?.work).toMatchObject([{ kind: "call", tool: "delegate", thread: "C" }]);
		// What a sub-thread told is shown though it said nothing, as it did not finish.
		expect(turns[1]?.said.map(({ from, text: words }) => ({ from, text: words }))).toEqual([
			{ from: "C", text: "The docs build." },
			{ from: "D", text: "" },
			{ from: undefined, text: "Thanks" },
		]);
	});

	it("marks a call the person declined, and gives a thread's first words where it has no title", () => {
		expect(declinedText({ ...callOf("c", "write", "q"), state: "rejected" })).toBe("Rejected");
		expect(declinedText({ ...callOf("c", "question", "q"), work: "ask", state: "rejected" })).toBe("Dismissed");
		expect(firstSaid(itemsOf(conversationOf(viewOf([said(id(0, 1), "First line\nsecond")]))))).toBe("First line");
		expect(firstSaid([])).toBeUndefined();
	});

	it("reads a turn that ended once, and keeps a call's line while its call is the same; and gives the files a turn changed", () => {
		const diff = "--- a.md\n+++ a.md\n@@ -1,2 +1,2 @@\n-old\n+new\n same";
		const entries: Entry[] = [
			said(id(0, 1), "Tidy"),
			reply(id(1, 2), "q1", [call("c1", "edit", { path: "a.md" }), call("c2", "write", { path: "b.md" })]),
			result(id(2, 3), "c1", "ok", diff, { type: "changed", path: "a.md", created: false, additions: 1, deletions: 1 }),
			result(id(2, 4), "c2", "ok", "made b.md: +3 -0", { type: "changed", path: "b.md", created: true, additions: 3, deletions: 0 }),
			end(id(3, 5), "replied"),
			said(id(4, 6), "And c"),
			reply(id(5, 7), "q2", [call("c3", "read", { path: "c.md" })]),
		];
		const before = turnsOf(conversationOf(viewOf(entries, true)));
		const after = turnsOf(conversationOf(viewOf(entries, true, [], [["q3", [{ type: "delta", index: 0, part: "text", text: "Hi" }]]])));
		expect(after[0]).toBe(before[0]);
		expect(after[1]?.work[0]).toBe(before[1]?.work[0]);
		expect(before[0]?.changes).toEqual([
			{ path: "a.md", added: 1, removed: 1 },
			{ path: "b.md", added: 3, removed: 0 },
		]);
	});

	it("lays a stretch of calls as one item from its first call on, delegations as sub-threads, every other step on its own", () => {
		const delegation = (id: string): CallLine => ({ ...callOf(id, "delegate", "q4"), work: "delegate" });
		const words: Work = { kind: "words", key: "w", text: "On it." };
		const laid = laidOut([
			{ ...callOf("a", "read", "q1"), work: "read", subject: "shot-1.png" },
			callOf("c", "shell", "q3"),
			callOf("d", "grep", "q3"),
			words,
			callOf("e", "shell", "q3"),
			delegation("f"),
			delegation("g"),
			{ ...callOf("h", "read", "q5"), state: "failed" },
		]);
		const shown = laid.map((each) => (each.kind === "step" ? each.step.kind : [each.kind, ...each.calls.map((call) => call.id)]));
		expect(shown).toEqual([["calls", "a", "c", "d"], "words", ["calls", "e"], ["delegations", "f", "g"], ["calls", "h"]]);
		// What was thought before a call is held with it, in the stretch's steps; what was thought after its last call stands on its own.
		const thought: Work = { kind: "thought", key: "t", text: "Hmm.", took: 1 };
		const after: Work = { kind: "thought", key: "u", text: "Done.", took: 1 };
		expect(laidOut([thought, callOf("x", "read", "q1"), after]).map((each) => (each.kind === "calls" ? each.steps.length : each.kind))).toEqual([2, "step"]);
		const gathered = laidOut([thought, callOf("x", "read", "q1"), callOf("y", "read", "q2")]);
		expect(gathered.map((each) => (each.kind === "calls" ? each.steps.length : each.kind))).toEqual([3]);
		const [stretch] = gathered;
		expect(stretch?.kind === "calls" ? thoughtCalls(stretch.steps).map(({ call, thoughts }) => [call.id, thoughts.length]) : []).toEqual([
			["x", 1],
			["y", 0],
		]);
	});

	it("holds calls one after another that change one file as one run, each after one that went well", () => {
		const write = (id: string, path: string, state: CallLine["state"] = "done"): CallLine => ({
			...callOf(id, "write", "q1"),
			work: "edit",
			subject: path,
			state,
		});
		const shown = [write("a", "x.sh"), write("b", "x.sh"), write("c", "x.sh", "failed"), write("d", "x.sh"), write("e", "y.sh")].map((call) => ({
			call,
			thoughts: [],
		}));
		expect(runsOf(shown).map((run) => run.map(({ call }) => call.id))).toEqual([["a", "b", "c"], ["d"], ["e"]]);
	});

	it("leads a stretch's line with the call going on, else the latest of the kind it did most", () => {
		const read = (id: string): CallLine => ({ ...callOf(id, "read", "q1"), work: "read" });
		const shell = (id: string): CallLine => ({ ...callOf(id, "shell", "q1"), work: "command" });
		expect(leadOf([read("a"), shell("b"), read("c"), shell("d"), read("e")])?.id).toBe("e");
		expect(leadOf([read("a"), shell("b"), read("c"), { ...shell("d"), state: "running" }])?.id).toBe("d");
		expect(leadOf([])).toBeUndefined();
	});
});
