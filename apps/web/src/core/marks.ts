/**
 * How the person keeps a thread, as a change they can take back: what putting a thread away — settling it, snoozing it,
 * archiving it, unpinning it, marking it unread — is said as, and the change that puts it back as it was.
 */
import type { Marks, MarksChange } from "@octoii-works/octoii-client";
import { wakeLabel } from "./time.ts";

/** A change taken back: what it did, in words, and the change that undoes it, made at the time it is taken back. */
export type Undoable = { readonly words: string; readonly undo: (now: number) => MarksChange };

/**
 * The way back from `change` of a thread whose marks were `marks`, made at `now`; none where the change puts nothing
 * away — pinning it, waking it, bringing it back — which needs no taking back.
 */
export function undoOf(marks: Marks, change: MarksChange, now: number): Undoable | undefined {
	if (change.settled_at != null) return { words: "Settled", undo: (at) => ({ unsettled_at: at }) };
	if (change.snoozed_until != null && change.snoozed_until > now) {
		const before = marks.snoozed_until ?? 0;
		return { words: `Snoozed till ${wakeLabel(change.snoozed_until, now)}`, undo: () => ({ snoozed_until: before }) };
	}
	if (change.archived === true && !marks.archived) return { words: "Archived", undo: () => ({ archived: false }) };
	if (change.pinned === false && marks.pinned) return { words: "Unpinned", undo: () => ({ pinned: true }) };
	if (change.seen_until === 0) {
		const before = marks.seen_until ?? 0;
		return { words: "Marked unread", undo: () => ({ seen_until: before }) };
	}
	return undefined;
}
