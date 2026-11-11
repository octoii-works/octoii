/** Where a thread's money and tokens went: its requests as its log tells them, and the waterfall they are laid as. */
import { describe, expect, it } from "bun:test";
import { conversationOf, type Entry, type Item, type ThreadView, type Usage } from "@octoii-works/octoii-client";
import { barsOf, mostOf, spentOf } from "../src/core/spending.ts";

/** An id minted at `ms`, as a UUIDv7 holds its time in its first 48 bits. */
function idAt(ms: number, serial: number): string {
	const time = ms.toString(16).padStart(12, "0");
	return `${time.slice(0, 8)}-${time.slice(8)}-7000-8000-${String(serial).padStart(12, "0")}`;
}

const START = 1_700_000_000_000;
const id = (seconds: number, serial: number): string => idAt(START + seconds * 1000, serial);
const said = (at: string, text: string): Entry => ({ id: at, type: "input", delivery: `d${at}`, actor: "person", content: [{ type: "input_text", text }] });
const call = (callId: string, name: string, args: unknown): Item => ({ type: "function_call", call_id: callId, name, arguments: JSON.stringify(args) });
const words = (text: string): Item => ({ type: "message", role: "assistant", content: [{ type: "output_text", text }] });
const usage = (input: number, cached: number, output: number, dollars: number): Usage => ({
	input_tokens: input,
	cached_tokens: cached,
	cache_write_tokens: 0,
	output_tokens: output,
	reasoning_tokens: 0,
	web_searches: 0,
	cost_nanodollars: Math.round(dollars * 1e9),
});
const result = (at: string, callId: string): Entry => ({ id: at, type: "result", call: `k${callId}`, status: "ok", output: { call_id: callId, output: [] } });
const reply = (at: string, request: string, output: Item[], used: Usage): Entry => ({
	calls: output.flatMap((item) => (item.type === "function_call" ? [{ call_id: item.call_id, key: `k${item.call_id}` }] : [])),
	id: at,
	type: "reply",
	request,
	response: { id: "r", status: "completed", model: "openrouter/m", output, usage: used },
});

function viewOf(entries: Entry[]): ThreadView {
	return {
		entries,
		read: true,
		complete: true,
		status: { turn: "replied", inbox: [], asks: [], compacting: false },
		streaming: new Map(),
		gone: false,
	};
}

describe("a thread's spending", () => {
	it("is each request its log tells of, by what it did, its model, what it read and wrote, cost and took", () => {
		const conversation = conversationOf(
			viewOf([
				said(id(0, 1), "Fix the build"),
				reply(id(3, 3), id(1, 2), [call("c1", "read", { path: "build.zig" }), call("c2", "shell", { command: "zig build" })], usage(1000, 800, 50, 0.002)),
				result(id(4, 4), "c1"),
				result(id(5, 5), "c2"),
				reply(id(7, 7), id(6, 6), [words("Fixed it.")], usage(1200, 1000, 20, 0.001)),
				{ id: id(8, 8), type: "turn-end", turn: "t1", outcome: "replied" },
			]),
		);
		const spent = spentOf(conversation.turns, conversation.compactions);
		expect(spent).toEqual([
			{ key: id(1, 2), turn: 1, words: "Read build.zig +1", model: "openrouter/m", input: 1000, cached: 800, output: 50, cost: 0.002, took: 2000 },
			{ key: id(6, 6), turn: 1, words: "Answered", model: "openrouter/m", input: 1200, cached: 1000, output: 20, cost: 0.001, took: 1000 },
		]);
		// Laid by cost, each starts where those before it left the total; by tokens, likewise.
		expect(barsOf(spent, "cost").map((bar) => [bar.start, bar.width].map((share) => Math.round(share * 1000) / 1000))).toEqual([
			[0, 0.667],
			[0.667, 0.333],
		]);
		expect(barsOf(spent, "tokens")[1]?.start).toBeCloseTo(1050 / 2270, 5);
		expect(mostOf(spent, "cost")?.key).toBe(id(1, 2));
		expect(mostOf(spent, "tokens")?.key).toBe(id(6, 6));
		expect(mostOf([], "cost")).toBeUndefined();
	});

	it("tells a compaction's request among the turns', by the turn its id falls in, a thread's at rest by the turn before", () => {
		const recap = (at: string, request: string): Entry => ({
			id: at,
			type: "compaction",
			request,
			response: { id: "r", model: "openrouter/m", status: "completed", output: [], usage: usage(500, 0, 30, 0.0005) },
			kept: at,
			from: at,
		});
		const conversation = conversationOf(
			viewOf([
				said(id(0, 1), "Fix the build"),
				reply(id(2, 3), id(1, 2), [words("Fixed it.")], usage(1000, 800, 50, 0.002)),
				{ id: id(3, 4), type: "turn-end", turn: "t1", outcome: "replied" },
				recap(id(5, 6), id(4, 5)),
			]),
		);
		expect(conversation.turns).toHaveLength(1);
		expect(spentOf(conversation.turns, conversation.compactions).map((each) => [each.key, each.turn, each.words])).toEqual([
			[id(1, 2), 1, "Answered"],
			[id(4, 5), 1, "Summed up the thread"],
		]);
	});
});
