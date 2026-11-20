import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import type { Listed, Start } from "@octoii-works/octoii-client";
import { PreparationCard } from "../src/extensions/git/thread-start.tsx";
import { isSettled, statusOf } from "../src/core/threads.ts";

const start: Start = { request: { owner: "git.worktree", payload: {} }, token: "attempt", previous: null, state: { state: "pending" } };
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
const card = {
	start,
	inputs: ["Saved task"],
	worktrees: undefined,
	busy: false,
	allowed: true,
	error: undefined,
	onInspect() {},
	onContinue() {},
	onCancel() {},
};

test("held input is not shown as Agent work, and pending preparation does not auto-settle", () => {
	expect(statusOf(row)).toBe("preparing");
	expect(isSettled(row, { status: "preparing", since: 0 }, 30 * 24 * 60 * 60 * 1000)).toBe(false);
	expect(statusOf({ ...row, status: { ...row.status, start: { ...start, state: { state: "failed", reason: "checkout failed" } } } })).toBe("failed");
	expect(statusOf({ ...row, status: { ...row.status, start: { ...start, state: { state: "cancelled" } } } })).toBe("ready");
});

test("recovery offers one action, preserves the saved input and allows cancellation while preparing", () => {
	const waiting = renderToStaticMarkup(<PreparationCard {...card} />);
	expect(waiting).toContain("Saved task");
	expect(waiting).toMatch(/<button(?![^>]* disabled=)[^>]*>Continue preparation/);
	const busy = renderToStaticMarkup(<PreparationCard {...card} busy worktrees={[]} />);
	expect(busy).toMatch(/<button(?![^>]* disabled=)[^>]*>Cancel task/);
	const inspected = renderToStaticMarkup(<PreparationCard {...card} worktrees={[]} />);
	expect(inspected).toMatch(/<button(?![^>]* disabled=)[^>]*>Continue preparation/);
});

test("failed and cancelled states expose neither automatic adoption nor restart", () => {
	const failed = renderToStaticMarkup(<PreparationCard {...card} start={{ ...start, state: { state: "failed", reason: "branch exists" } }} worktrees={[]} />);
	expect(failed).toContain("branch exists");
	expect(failed).toContain("Retry preparation");
	expect(failed).toContain("never adopted or removed automatically");
	const cancelled = renderToStaticMarkup(<PreparationCard {...card} start={{ ...start, state: { state: "cancelled" } }} />);
	expect(cancelled).not.toContain("<button");
	expect(cancelled).toContain("This task will not run");
	expect(cancelled).toContain("Saved task");
});
