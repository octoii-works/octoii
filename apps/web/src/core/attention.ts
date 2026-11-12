/**
 * What the person is told of their threads as it happens, wherever they are: a thread that begins to wait on them — a
 * question, a call to allow — and a thread of their own, not a sub-thread, whose turn ended, done or failed; and how
 * many threads wait on them, which the page's title counts.
 */
import type { Listed } from "@octoii-works/octoii-client";

/** What the person is told of a thread: which, its title, and what happened. */
export type Attention = { readonly thread: string; readonly kind: "waiting" | "done" | "failed" };

/** What happened between `before` and `after`, the threads as listed then and now. */
export function attentionOf(before: readonly Listed[], after: readonly Listed[]): readonly Attention[] {
	const was = new Map(before.map((row) => [row.id, row]));
	return after.flatMap((row): readonly Attention[] => {
		const then = was.get(row.id);
		if (then === undefined) return [];
		if (then.status.asks.length === 0 && row.status.asks.length > 0) return [{ thread: row.id, kind: "waiting" }];
		if (row.parent != null || then.status.turn !== "running") return [];
		if (row.status.turn === "replied") return [{ thread: row.id, kind: "done" }];
		if (row.status.turn === "failed") return [{ thread: row.id, kind: "failed" }];
		return [];
	});
}

/** How many of `rows` wait on the person, the archived left out. */
export function waitingCount(rows: readonly Listed[]): number {
	return rows.filter((row) => !row.marks.archived && row.status.asks.length > 0).length;
}

/** What happened, in words. */
export function attentionWords(attention: Attention): string {
	switch (attention.kind) {
		case "waiting":
			return "Waiting for you";
		case "done":
			return "Done";
		case "failed":
			return "Failed";
	}
}
