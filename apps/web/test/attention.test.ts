/** What the person is told of their threads as it happens. */
import { describe, expect, it } from "bun:test";
import type { Listed, Status } from "@octoii-works/octoii-client";
import { attentionOf, waitingCount } from "../src/core/attention.ts";

const ask = { process: "x", key: "k1", name: "permission", payload: null, answer_schema: null };
const row = (id: string, turn: Status["turn"], asks = 0, parent?: string): Listed => ({
	id,
	...(parent === undefined ? {} : { parent: { thread: parent, call: "c" } }),
	config: { mode: "default" },
	marks: { pinned: false, archived: false },
	status: { turn, inbox: [], asks: Array.from({ length: asks }, () => ask), compacting: false },
});

describe("what the person is told", () => {
	it("is a thread that begins to wait on them, and one of their own whose turn ended", () => {
		const before = [row("a", "running"), row("b", "running"), row("c", "running"), row("s", "running", 0, "a")];
		const after = [row("a", "running", 1), row("b", "replied"), row("c", "failed"), row("s", "replied", 0, "a"), row("n", "running")];
		expect(attentionOf(before, after)).toEqual([
			{ thread: "a", kind: "waiting" },
			{ thread: "b", kind: "done" },
			{ thread: "c", kind: "failed" },
		]);
		expect(attentionOf(after, after)).toEqual([]);
		expect(waitingCount(after)).toBe(1);
	});
});
