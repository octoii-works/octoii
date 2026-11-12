/** A change of how a thread is kept, as the person takes it back. */
import { describe, expect, it } from "bun:test";
import type { Marks } from "@octoii-works/octoii-client";
import { undoOf } from "../src/core/marks.ts";

describe("a thread put away", () => {
	const marks: Marks = { pinned: true, archived: false, seen_until: 50 };

	it("is put back as it was, and nothing else is taken back", () => {
		const back = (change: Parameters<typeof undoOf>[1]) => {
			const undoable = undoOf(marks, change, 100);
			return undoable === undefined ? undefined : [undoable.words, undoable.undo(200)];
		};
		// Brought back as the person takes it back, later than it was settled.
		expect(back({ settled_at: 100 })).toEqual(["Settled", { unsettled_at: 200 }]);
		expect(back({ snoozed_until: 5_000 })?.[1]).toEqual({ snoozed_until: 0 });
		expect(back({ archived: true })).toEqual(["Archived", { archived: false }]);
		expect(back({ pinned: false })).toEqual(["Unpinned", { pinned: true }]);
		expect(back({ seen_until: 0 })).toEqual(["Marked unread", { seen_until: 50 }]);
		expect(undoOf(marks, { pinned: true }, 100)).toBeUndefined();
		expect(undoOf(marks, { snoozed_until: 100 }, 100)).toBeUndefined();
		expect(undoOf(marks, { unsettled_at: 100 }, 100)).toBeUndefined();
	});
});
