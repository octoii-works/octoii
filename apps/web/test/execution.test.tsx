/** Execute presents actual completed work and its final output, source and tool details behind folds. */
import { describe, expect, it } from "bun:test";
import { conversationOf, type CallStatus, type Entry, type InputContent, type ThreadView } from "@octoii-works/octoii-client";
import { renderToStaticMarkup } from "react-dom/server";
import { CallDetail, ScriptOutput } from "../src/components/thread/call-detail.tsx";
import { PresentedFiles } from "../src/components/thread/presented-files.tsx";
import { type Reach, ReachContext } from "../src/components/state/reach.ts";
import { type ExecutedCall, executedCallsOf } from "../src/core/execution.ts";
import { itemsOf, laidOut, type Work } from "../src/core/timeline.ts";
import { detailOf, summaryOf } from "../src/core/tools.ts";
import "../src/extensions/contributions.ts";

const code = "await tools.read({ path: 'SCRIPT_CODE_ONLY.txt' }); return 'selected';";
const output = "VISIBLE_FINAL_OUTPUT";
const resource: InputContent = {
	type: "resource_link",
	file: { ref: "sha256:report", media_type: "application/pdf", filename: "result.pdf" },
	size: 42,
};

function recorded(tool: string, args: object, fields: Partial<ExecutedCall> = {}): ExecutedCall {
	return { id: 0, key: "nested-0", tool, arguments: JSON.stringify(args), status: "ok", output: "nested output", ...fields };
}

/** A completed script reloaded from persisted protocol entries, as opening its thread reads it. */
function executionOf(calls: readonly ExecutedCall[], status: CallStatus = "ok", content: readonly InputContent[] = [{ type: "input_text", text: output }]) {
	const entries: Entry[] = [
		{ id: "01", type: "input", delivery: "d", actor: "person", content: [{ type: "input_text", text: "Do the work" }] },
		{
			id: "02",
			type: "reply",
			request: "q",
			response: {
				id: "r",
				model: "m",
				status: "completed",
				output: [{ type: "function_call", call_id: "execute-1", name: "execute", arguments: JSON.stringify({ code }) }],
			},
			calls: [{ call_id: "execute-1", key: "execution-key" }],
		},
		{
			id: "03",
			type: "result",
			call: "execution-key",
			status,
			output: { call_id: "execute-1", output: [...content] },
			structuredContent: { calls: [...calls] },
		},
		{ id: "04", type: "turn-end", outcome: "replied" },
	];
	const persisted: Entry[] = JSON.parse(JSON.stringify(entries));
	const view: ThreadView = {
		entries: persisted,
		read: true,
		complete: true,
		status: { turn: "replied", inbox: [], asks: [], compacting: false },
		streaming: new Map(),
		gone: false,
	};
	const turn = itemsOf(conversationOf(view)).find((item) => item.kind === "turn")?.turn;
	const call = turn?.work.find((work) => work.kind === "call");
	if (turn === undefined || call === undefined) throw new Error("A persisted execution call");
	return { turn, call };
}

const reach: Reach = {
	open: () => {
		throw new Error("No socket needed to render a delivered file");
	},
	send: async () => ({ status: 200, contentType: "application/octet-stream", bytes: new Uint8Array() }),
	linkExample: "",
	readsLink: () => false,
	pair: async () => undefined,
};

describe("an execution's actual completed calls", () => {
	it("reads only a valid execute envelope and never infers completed work from the script", () => {
		const call = recorded("read", { path: "a.txt" });
		const calls = [call];
		expect(executedCallsOf("execute", { calls })).toEqual(calls);
		expect(executedCallsOf("server.execute", { calls })).toEqual([]);
		expect(executedCallsOf("execute", { calls: [{ ...call, status: "unknown" }] })).toEqual([]);
		expect(executedCallsOf("execute", null)).toEqual([]);
		expect(executionOf([]).call.calls).toEqual([]);
	});

	it("uses each nested tool's existing renderer, structured facts and title, without inventing timing", () => {
		const { turn, call } = executionOf([
			recorded(
				"read",
				{ path: "a.txt" },
				{ output: "file contents", structuredContent: { type: "read", path: "a.txt", text: "file contents", next: null, clipped: [], from: 2, to: 3 } },
			),
			recorded(
				"edit",
				{ path: "b.txt" },
				{
					id: 1,
					key: "nested-1",
					output: "--- b.txt\n+++ b.txt\n@@ -1 +1 @@\n-old\n+new",
					structuredContent: { type: "changed", path: "b.txt", created: false, additions: 1, deletions: 1 },
				},
			),
			recorded(
				"shell",
				{ command: "echo ready" },
				{
					id: 2,
					key: "nested-2",
					output: "ready\n<system-reminder>exit 0</system-reminder>",
					structuredContent: { type: "ran", output: "ready", ended: { type: "exited", code: 0 } },
				},
			),
			recorded("custom.report", { name: "report" }, { id: 3, key: "nested-3", metadata: { title: "Report title" } }),
		]);
		expect(call.calls.map((nested) => detailOf(nested).shape)).toEqual(["file", "diff", "command", "fields"]);
		expect(call.calls[0]?.aside).toEqual({ kind: "words", text: "lines 2–3" });
		expect(call.calls[3]?.subject).toBe("Report title");
		expect(call.calls.every((nested) => nested.began === undefined && nested.ended === undefined)).toBe(true);
		expect(turn.changes).toEqual([{ path: "b.txt", added: 1, removed: 1 }]);
	});

	it("keeps earlier successful work and failed child calls when the script fails", () => {
		const { call } = executionOf(
			[
				recorded("read", { path: "a.txt" }),
				recorded("write", { path: "b.txt", content: "new" }, { id: 1, key: "nested-1", status: "tool-error", output: "write refused" }),
			],
			"tool-error",
			[{ type: "input_text", text: "<system-reminder>the script threw: write refused</system-reminder>" }],
		);
		expect(call.state).toBe("failed");
		expect(call.calls.map((nested) => nested.state)).toEqual(["done", "failed"]);
		expect(summaryOf(call.calls)).toBe("read 1 file");
		expect(detailOf(call)).toEqual({ shape: "script", code });
		expect(call.output).toContain("the script threw: write refused");
	});

	it("does not report a rejected or failed write as an edited file, even when the script catches it", () => {
		const { call, turn } = executionOf([
			recorded("write", { path: "rejected.txt", content: "new" }, { status: "rejected", output: "denied" }),
			recorded("write", { path: "failed.txt", content: "new" }, { id: 1, key: "nested-1", status: "tool-error", output: "failed" }),
			recorded("write", { path: "stopped.txt", content: "new" }, { id: 2, key: "nested-2", status: "interrupted", output: "stopped" }),
		]);
		expect(call.state).toBe("done");
		expect(summaryOf(call.calls)).toBe("3 tool calls");
		expect(call.calls.map((nested) => nested.state)).toEqual(["rejected", "failed", "stopped"]);
		expect(turn.changes).toEqual([]);
	});
});

describe("the script's fold and final output", () => {
	it("keeps scripts on their own lines with their preceding thought, outside neighboring stretches", () => {
		const { call } = executionOf([]);
		const before = { ...call, id: "before", tool: "read", work: "read" as const };
		const after = { ...before, id: "after" };
		const thought: Work = { kind: "thought", key: "thought", text: "Plan the script", took: 1 };
		const laid = laidOut([before, thought, call, after]);
		expect(laid.map((item) => (item.kind === "calls" ? item.calls.map((each) => each.id) : item.kind))).toEqual([["before"], ["execute-1"], ["after"]]);
		expect(laid[1]?.kind === "calls" ? laid[1].steps : []).toEqual([thought, call]);
	});

	it("keeps output and source collapsed independently", () => {
		const { call } = executionOf([recorded("read", { path: "actual.txt" }, { output: "NESTED_OUTPUT_ONLY" })]);
		expect(summaryOf(call.calls)).toBe("read 1 file");
		const result = renderToStaticMarkup(<ScriptOutput call={call} />);
		expect(result).toContain("Output");
		expect(result).toContain('aria-expanded="false"');
		expect(result).not.toContain("VISIBLE_FINAL_OUTPUT");
		expect(result).not.toContain("SCRIPT_CODE_ONLY");
		expect(result).not.toContain("NESTED_OUTPUT_ONLY");
		const detail = renderToStaticMarkup(<CallDetail call={call} />);
		expect(detail).toContain("Source code");
		expect(detail).toContain('aria-expanded="false"');
		expect(detail).not.toContain("SCRIPT_CODE_ONLY");
	});

	it("counts paths rather than repeated edits, preserving failures separately", () => {
		const { call } = executionOf([
			recorded("edit", { path: "a.txt" }),
			recorded("edit", { path: "a.txt" }, { id: 1, key: "second" }),
			recorded("write", { path: "b.txt" }, { id: 2, key: "third" }),
			recorded("edit", { path: "failed.txt" }, { id: 3, key: "fourth", status: "tool-error" }),
		]);
		expect(summaryOf(call.calls)).toBe("edited 2 files (3 edits)");
	});

	it("uses recorded result paths ahead of argument spelling and keeps case distinct", () => {
		const changed = (path: string) => ({ type: "changed", path, additions: 1, deletions: 1, created: false });
		const { call } = executionOf([
			recorded("edit", { path: "./a.txt" }, { structuredContent: changed("a.txt") }),
			recorded("edit", { path: "a.txt" }, { structuredContent: changed("a.txt") }),
			recorded("edit", { path: "A.txt" }, { structuredContent: changed("A.txt") }),
		]);
		expect(summaryOf(call.calls)).toBe("edited 2 files (3 edits)");
		expect(summaryOf(executionOf([recorded("read", { path: "a.txt" }), recorded("read", { path: "a.txt", offset: 10 })]).call.calls)).toBe(
			"read 1 file (2 reads)",
		);
		expect(summaryOf(executionOf([recorded("edit", {})]).call.calls)).toBe("made 1 edit");
	});

	it("shows delivered resources only once at the outer boundary, including when the script failed", () => {
		const { call, turn } = executionOf([recorded("present_file", { path: "report.pdf" })], "tool-error", [{ type: "input_text", text: output }, resource]);
		expect(call.calls[0]?.files).toEqual([]);
		expect(call.files).toHaveLength(1);
		expect(laidOut(turn.work)).toHaveLength(1);
		const html = renderToStaticMarkup(
			<ReachContext value={reach}>
				<CallDetail call={call} />
				<ScriptOutput call={call} />
				<PresentedFiles thread="t1" files={call.files} />
			</ReachContext>,
		);
		expect(html.match(/More actions for result\.pdf/g)).toHaveLength(1);
		expect(html).not.toContain("VISIBLE_FINAL_OUTPUT");
	});
});
