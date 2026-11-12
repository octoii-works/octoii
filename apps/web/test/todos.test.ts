/**
 * The model's todo list in a thread: the todos of its latest todo call, a call made under a script among them, as its
 * result told them; and how many are done.
 */
import { describe, expect, it } from "bun:test";
import type { Todo } from "@octoii-works/octoii-client";
import type { CallLine, TimelineItem, TimelineTurn, Work } from "../src/core/timeline.ts";
import { completed, latestTodos } from "../src/core/todos.ts";

/** A call of `tool` that told `todos`, where it is the todo tool, with `calls` made under it. */
function call(tool: string, todos?: readonly Todo[], calls: readonly CallLine[] = []): CallLine {
	return {
		kind: "call",
		id: tool,
		key: tool,
		step: "step",
		thread: undefined,
		tool,
		work: "other",
		verb: undefined,
		subject: undefined,
		code: false,
		aside: undefined,
		trouble: undefined,
		arguments: "{}",
		output: undefined,
		structured: todos === undefined ? undefined : { type: "todos", todos: [...todos] },
		began: undefined,
		ended: undefined,
		state: "done",
		ask: undefined,
		calls,
	};
}

/** A turn whose work is `work`. */
function turn(key: string, work: readonly Work[]): TimelineItem {
	const turned: TimelineTurn = {
		key,
		said: [],
		work,
		answer: undefined,
		changes: [],
		writing: { request: undefined, words: "", calls: [] },
		asking: undefined,
		asks: [],
		ending: { state: "replied" },
		startedAt: undefined,
		endedAt: undefined,
	};
	return { kind: "turn", key, turn: turned };
}

const first: readonly Todo[] = [{ content: "Read", status: "in_progress" }];
const later: readonly Todo[] = [
	{ content: "Read", status: "completed" },
	{ content: "Change", status: "in_progress" },
];

describe("the model's todo list", () => {
	it("is the latest turn's last todo call's, a script's call among them, and none where the latest turn wrote none", () => {
		expect(latestTodos([])).toBeUndefined();
		expect(latestTodos([turn("a", [call("read")])])).toBeUndefined();
		expect(latestTodos([turn("a", [call("todo", first), call("read"), call("todo", later), call("read")])])).toEqual(later);
		expect(latestTodos([turn("a", [call("todo", first), call("execute", undefined, [call("todo", later)])])])).toEqual(later);
		expect(latestTodos([turn("a", [call("todo", later)]), turn("b", [call("read")])])).toBeUndefined();
		expect(completed(later)).toBe(1);
	});
});
