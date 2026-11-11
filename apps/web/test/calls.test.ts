/**
 * A call as its tool told it, read: its text and the notes after it; how a command ended, what an edit or a write
 * changed and the lines a read held, as its tool told them typed; an edit's diff, what a search found, and its
 * arguments as fields.
 */
import { describe, expect, it } from "bun:test";
import type { Structured } from "@octoii-works/octoii-client";
import { changeOf, endOf, fieldsOf, foundOf, hunksOf, prettyArguments, spanOf, structuredOf, type ToolCall, toldOf } from "../src/core/calls.ts";

/** A call of `tool` that ran, its tool having told `output`, and `structured` typed. */
const ran = (tool: string, output: string, structured?: Structured): ToolCall => ({ tool, arguments: "{}", output, structured, state: "done" });

describe("a call as its tool told it", () => {
	it("tells a tool's text from the notes after it, a line each in brackets, the text left as it is", () => {
		expect(toldOf("a\n[b] c\n[exit 1]\n[timed out after 5 s; killed]")).toEqual({ text: "a\n[b] c", notes: ["exit 1", "timed out after 5 s; killed"] });
		expect(toldOf("[no match]")).toEqual({ text: "", notes: ["no match"] });
		expect(toldOf("plain\n")).toEqual({ text: "plain\n", notes: [] });
	});

	it("takes what one of Siphon's own tools told typed, none of an MCP server's", () => {
		const ended: Structured = { type: "ran", ended: { type: "exited", code: 0 } };
		expect(structuredOf("shell", ended)).toEqual(ended);
		expect(structuredOf("github.create_issue", { type: "ran" })).toBeUndefined();
		expect(structuredOf("shell", null)).toBeUndefined();
	});

	it("tells how a command ended as its tool told it; nothing while it runs", () => {
		expect(endOf(ran("shell", "built\n[exit 0]", { type: "ran", ended: { type: "exited", code: 0 } }))).toEqual({ type: "exited", code: 0 });
		expect(endOf(ran("shell", "[ended by signal 9]", { type: "ran", ended: { type: "signalled", signal: 9 } }))).toEqual({ type: "signalled", signal: 9 });
		expect(endOf({ ...ran("shell", ""), output: undefined, state: "running" })).toBeUndefined();
	});

	it("tells what an edit or a write changed as its tool told it, and gives an edit's diff from its first hunk", () => {
		const diff = "--- a.sql\n+++ a.sql\n@@ -1,3 +1,2 @@\n--- an old comment\n-select 1;\n+select 2;\n kept";
		const changed = (additions: number, deletions: number): Structured => ({ type: "changed", path: "a.sql", created: false, additions, deletions });
		expect(changeOf(ran("edit", diff, changed(1, 2)))).toEqual({ added: 1, removed: 2 });
		expect(hunksOf(ran("edit", diff))).toBe("@@ -1,3 +1,2 @@\n--- an old comment\n-select 1;\n+select 2;\n kept");
		expect(changeOf(ran("write", "made b.md: +12 -0", changed(12, 0)))).toEqual({ added: 12, removed: 0 });
		expect(changeOf({ ...ran("edit", "not-found", changed(1, 1)), state: "failed" })).toBeUndefined();
	});

	it("reads the lines a read held, and what a glob and a grep found", () => {
		expect(spanOf(ran("read", "…", { type: "read", path: "a.md", from: 1, to: 200 }))).toBe("lines 1–200");
		expect(spanOf(ran("read", "…", { type: "read", path: "a.md", from: 201, to: 300, total: 300 }))).toBe("lines 201–300");
		expect(spanOf(ran("read", "whole", { type: "read", path: "a.md", from: 1, to: 12, total: 12 }))).toBeUndefined();
		expect(foundOf(ran("glob", "src/a.ts\nsrc/b.ts"))).toEqual([
			{ path: "src/a.ts", line: undefined, text: undefined },
			{ path: "src/b.ts", line: undefined, text: undefined },
		]);
		expect(foundOf(ran("grep", "src/a.ts:3: let x\nsrc/a.ts:4- context\n[more matches than shown]"))).toEqual([{ path: "src/a.ts", line: 3, text: "let x" }]);
	});

	it("gives arguments as their fields, a value not a string as JSON, and as they were laid out where they are no object", () => {
		expect(fieldsOf('{"query":"bugs","limit":5,"open":true}')).toEqual([
			{ name: "query", value: "bugs" },
			{ name: "limit", value: "5" },
			{ name: "open", value: "true" },
		]);
		expect(fieldsOf("[1,2]")).toBeUndefined();
		expect(fieldsOf("{not json")).toBeUndefined();
		expect(prettyArguments('{"path":"a.md","limit":2}')).toBe('{\n  "path": "a.md",\n  "limit": 2\n}');
		expect(prettyArguments("{not json")).toBe("{not json");
	});
});
