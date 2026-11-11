/**
 * A thread put together by its ids: turns from the log's runs, each step by its request's id, each call by its id with
 * its result and what it asks the person; what streams of a request a step of its own before its reply comes; a turn
 * that ended the same object from one view to the next, and of the turn that runs, each part nothing is laid over.
 */
import { describe, expect, test } from "bun:test";
import { type Ask, conversationOf, type Entry, type Item, type Status, type Streamed, type ThreadView } from "../src/index.ts";

const response = (output: Item[]) => ({ id: "r", model: "m", status: "completed" as const, output });
const text = (value: string): Item => ({ type: "message", role: "assistant", content: [{ type: "output_text", text: value }] });
const call = (id: string, name: string): Item => ({ type: "function_call", call_id: id, name, arguments: "{}" });
const input = (id: string, said: string): Entry => ({ type: "input", id, actor: "person", delivery: `d${id}`, content: [{ type: "input_text", text: said }] });
const reply = (id: string, request: string, output: Item[]): Entry => ({ type: "reply", id, request, response: response(output) });
const result = (id: string, callId: string, said: string): Entry => ({
	type: "result",
	id,
	call: `k${callId}`,
	status: "ok",
	output: { call_id: callId, output: [{ type: "input_text", text: said }] },
});
const ask = (call: string): Ask => ({
	process: "main/turn",
	key: `k${call}.permission`,
	name: "siphon.permission",
	payload: { call_id: call },
	answer_schema: {},
});

function viewOf(entries: Entry[], asks: Ask[] = [], streaming: [string, Streamed[]][] = []): ThreadView {
	const status: Status = { turn: "running", inbox: [["9", { type: "input", actor: "person", content: [{ type: "input_text", text: "and then?" }] }]], asks };
	return { entries, read: true, complete: true, status, streaming: new Map(streaming), gone: false };
}

const ENDED: Entry[] = [
	{ type: "system", id: "1", sections: [{ name: "base", text: "b" }] },
	input("2", "read a.md"),
	reply("3", "q1", [call("c1", "read")]),
	result("4", "c1", "alpha"),
	reply("5", "q2", [text("It says alpha.")]),
	{ type: "turn-end", id: "6", turn: "t1", outcome: "replied" },
];

describe("a thread's conversation, by its ids", () => {
	test("a turn ended holds its steps and calls with their results; a turn that runs, what its calls ask the person", () => {
		const view = viewOf([...ENDED, input("7", "remove it"), reply("8", "q3", [call("c2", "shell"), call("c3", "read")]), result("9", "c3", "x")], [ask("c2")]);
		const conversation = conversationOf(view);
		const [ended, running] = conversation.turns;
		expect(ended?.told.map((told) => told.id)).toEqual(["1"]);
		expect(ended?.steps.map((step) => [step.request, step.calls.map((each) => [each.id, each.result?.id])])).toEqual([
			["q1", [["c1", "4"]]],
			["q2", []],
		]);
		expect(ended?.end?.outcome).toBe("replied");
		const calls = running?.steps[0]?.calls;
		expect(calls?.map((each) => [each.id, each.result?.id, each.ask?.key])).toEqual([
			["c2", undefined, "kc2.permission"],
			["c3", "9", undefined],
		]);
		expect(conversation.inbox.map(([id]) => id)).toEqual(["9"]);
	});

	test("the request a turn waits on its model for is a step of its own, what streams of it there, before its reply comes", () => {
		const streamed: Streamed[] = [{ type: "delta", index: 0, part: "text", text: "Wor" }];
		const running = conversationOf(viewOf([...ENDED, input("7", "go on")], [], [["q3", streamed]])).turns[1];
		expect(running?.steps.map((step) => [step.request, step.streaming, step.reply])).toEqual([["q3", streamed, undefined]]);
	});

	test("a turn that ended is the same object from one view to the next, and so is each part of the turn that runs that nothing is laid over", () => {
		const entries = [...ENDED, input("7", "remove it"), reply("8", "q3", [call("c2", "shell"), call("c3", "read")])];
		const before = conversationOf(viewOf(entries));
		const after = conversationOf(viewOf(entries, [ask("c2")]));
		expect(after.turns[0]).toBe(before.turns[0] as never);
		expect(after.turns[1]).not.toBe(before.turns[1] as never);
		const [asked, kept] = after.turns[1]?.steps[0]?.calls ?? [];
		expect(asked?.ask?.key).toBe("kc2.permission");
		expect(kept).toBe(before.turns[1]?.steps[0]?.calls[1] as never);
	});

	test("a compaction is the thread's, by its id, none a turn's: one made at rest begins no turn", () => {
		const compaction: Entry = { type: "compaction", id: "7", request: "q9", from: "2", response: response([text("recap")]) };
		const conversation = conversationOf(viewOf([...ENDED, compaction]));
		expect(conversation.turns.length).toBe(1);
		expect(conversation.compactions.map((each) => each.id)).toEqual(["7"]);
	});

	test("a turn whose first entries were read later is put together again from the run it holds now", () => {
		const late = conversationOf(viewOf(ENDED.slice(2)));
		const whole = conversationOf(viewOf(ENDED));
		expect(late.turns[0]?.inputs).toEqual([]);
		expect(whole.turns[0]?.inputs.map((each) => each.id)).toEqual(["2"]);
	});
});
