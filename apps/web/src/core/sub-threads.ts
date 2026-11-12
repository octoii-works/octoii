/**
 * Sub-threads as the thread that delegated to them shows them: how each stands — as it works, or how its work ended —
 * from its own status where it is listed, from the call that gave it its task where it is not; when they worked; what
 * a sub-thread said, in a line; and those that work in the background.
 */
import type { Listed } from "@octoii-works/octoii-client";
import { subThreadOf } from "./threads.ts";
import type { CallLine, Work } from "./timeline.ts";
import { givenOf } from "./tools.ts";

/** How a sub-thread stands. */
export type Standing = "running" | "completed" | "failed" | "stopped";

/** How the sub-thread `row` stands, by its status; where it is not listed — not made, or deleted — as `call` stands. */
export function standingOf(row: Listed | undefined, call?: CallLine): Standing {
	if (row !== undefined) {
		switch (row.status.turn) {
			case "created":
			case "running":
				return "running";
			case "replied":
				return "completed";
			case "failed":
				return "failed";
			case "stopped":
				return "stopped";
		}
	}
	switch (call?.state) {
		case "running":
		case "waiting":
			return "running";
		case "done":
			return "completed";
		case "failed":
		case "rejected":
			return "failed";
		case "stopped":
		case undefined:
			return "stopped";
	}
}

/** When sub-threads worked: from the first one's latest turn's beginning to the last one's end — none while any works. */
export type Worked = { readonly began: number; readonly ended: number | undefined };

/** When the sub-threads `rows` worked on what they were last given; none where none began, or none is listed. */
export function workedOf(rows: readonly (Listed | undefined)[]): Worked | undefined {
	const spans = rows.flatMap((row) => {
		const began = row?.status.began_at;
		return began === undefined || began === null ? [] : [{ began, ended: row?.status.ended_at ?? undefined }];
	});
	if (spans.length === 0) return undefined;
	const ends = spans.flatMap((span) => (span.ended === undefined ? [] : [span.ended]));
	return { began: Math.min(...spans.map((span) => span.began)), ended: ends.length < spans.length ? undefined : Math.max(...ends) };
}

/** How many sub-threads stand each way, in words — "2 running · 1 completed" — the ways none stand left out. */
export function standingsSummary(standings: readonly Standing[]): string {
	const order: readonly Standing[] = ["running", "completed", "failed", "stopped"];
	return order
		.map((standing) => [standing, standings.filter((each) => each === standing).length] as const)
		.filter(([, count]) => count > 0)
		.map(([standing, count]) => `${String(count)} ${standing}`)
		.join(" · ");
}

/** What a sub-thread said, as one plain line: its links by their words, its code, emphasis, heading and list marks let go of, its breaks as spaces. */
export function plainLine(text: string): string {
	return text
		.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
		.replace(/`|\*\*|__/g, "")
		.replace(/^[ \t]*#+[ \t]+/gm, "")
		.replace(/^[ \t]*[-*][ \t]+/gm, "")
		.replace(/\s+/g, " ")
		.trim();
}

/** The sub-threads of the thread `thread` among `rows` that `work`'s calls gave a task in the background, which work on it still. */
export function inBackground(work: readonly Work[], rows: readonly Listed[], thread: string): readonly Listed[] {
	const working: Listed[] = [];
	for (const step of work) {
		if (step.kind !== "call" || step.work !== "delegate" || givenOf(step.arguments).background !== true) continue;
		const row = subThreadOf(rows, thread, step);
		if (row !== undefined && standingOf(row) === "running" && !working.includes(row)) working.push(row);
	}
	return working;
}
