/** A thread its model's quota limits: the window in words, and the other accounts it may go on with. */
import { describe, expect, it } from "bun:test";
import { conversationOf, type Entry, type Failure, type Model } from "@octoii-works/octoii-client";
import { limitedOf, onwardOf, windowWords } from "../src/core/limits.ts";
import type { Directory } from "../src/core/models.ts";
import { catalogued } from "./fixtures.ts";

const model = (provider: string, id: string): Model => catalogued(provider, id);

const DIRECTORY: Directory = {
	models: [
		model("chatgpt", "gpt-6-luna"),
		model("work", "gpt-6-luna"),
		model("spent", "gpt-6-luna"),
		model("signed-out", "gpt-6-luna"),
		model("other", "gpt-6-terra"),
		model("claude", "claude-opus-5-5"),
	],
	providers: [
		{ id: "chatgpt", name: "ChatGPT", connects: "sign_in", credential: "o_auth" },
		{ id: "work", name: "ChatGPT · Work", connects: "sign_in", credential: "o_auth", account: { id: "work", provider: "chatgpt", name: "Work" } },
		{
			id: "spent",
			name: "ChatGPT · Spent",
			connects: "sign_in",
			credential: "o_auth",
			account: { id: "spent", provider: "chatgpt", name: "Spent" },
			used_up: { reset_at_ms: 9 },
		},
		{ id: "signed-out", name: "ChatGPT · Out", connects: "sign_in", account: { id: "signed-out", provider: "chatgpt", name: "Out" } },
		{ id: "other", name: "ChatGPT · Other", connects: "sign_in", credential: "o_auth", account: { id: "other", provider: "chatgpt", name: "Other" } },
		{ id: "claude", name: "Claude", connects: "server" },
	],
};

describe("a limited thread", () => {
	it("names the window its quota is counted over", () => {
		expect(windowWords(300)).toBe("5-hour");
		expect(windowWords(10_080)).toBe("weekly");
		expect(windowWords(90)).toBe("90-minute");
		expect(windowWords(undefined)).toBeUndefined();
	});

	it("goes on with the other accounts of its provider that are connected, have room and serve its model", () => {
		const onward = onwardOf(DIRECTORY, { provider: "chatgpt", reset_at_ms: 9 }, "chatgpt/gpt-6-luna");
		expect(onward.map((each) => each.reference)).toEqual(["work/gpt-6-luna"]);
		// From an added account, the host's own is another account of the same provider.
		expect(onwardOf(DIRECTORY, { provider: "work", reset_at_ms: 9 }, "work/gpt-6-luna").map((each) => each.reference)).toEqual(["chatgpt/gpt-6-luna"]);
		expect(onwardOf(DIRECTORY, { provider: "claude", reset_at_ms: 9 }, "claude/claude-opus-5-5")).toEqual([]);
	});

	it("is told by its last turn's end: its model's account's quota used up, till a time still to come", () => {
		const ended = (failure: Failure | undefined, reason?: string): ReturnType<typeof conversationOf> => {
			const said: Entry = { type: "input", id: "1", actor: "person", delivery: "d1", content: [{ type: "input_text", text: "hi" }] };
			const end: Entry = {
				type: "turn-end",
				id: "2",
				outcome: "failed",
				...(failure === undefined ? {} : { failure }),
				...(reason === undefined ? {} : { reason }),
			};
			return conversationOf({ entries: [said, end], read: true, complete: true, status: undefined, streaming: new Map(), gone: false });
		};
		const quota: Failure = { kind: "quota", message: "the weekly limit is reached", reset_at_ms: 2_000, window_minutes: 10_080 };
		expect(limitedOf(ended(quota), "chatgpt/gpt-6-luna", 1_000)).toEqual({ provider: "chatgpt", reset_at_ms: 2_000, window_minutes: 10_080 });
		// Given again since, or a failure of another kind, or none of a model's: nothing limits it.
		expect(limitedOf(ended(quota), "chatgpt/gpt-6-luna", 3_000)).toBeUndefined();
		expect(limitedOf(ended({ kind: "server", message: "down" }), "chatgpt/gpt-6-luna", 1_000)).toBeUndefined();
		expect(limitedOf(ended(undefined, "its thread's budget is spent"), "chatgpt/gpt-6-luna", 1_000)).toBeUndefined();
	});
});
