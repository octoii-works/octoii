/**
 * How the person keeps a thread, changed: the change sent to the node, what it refused told as a notice; and a change
 * that puts a thread away — settled, snoozed, archived, unpinned, marked unread — told as a notice that takes it back,
 * as ⌘Z does wherever no text field has the focus, the latest first. What was taken back is not taken back again.
 */
import type { Client, Listed, MarksChange } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { undoOf } from "../../core/marks.ts";
import { threadTitle } from "../../core/threads.ts";
import { useShortcutOutsideText } from "../effects/shortcut.ts";
import { failed, notices } from "./notices.ts";

/** A change that can be taken back: the thread, the change back as it is made when taken, and the notice that offers it. */
type Taken = { readonly thread: string; readonly undo: (now: number) => MarksChange; readonly notice: string };

/** How many changes wait to be taken back at most, the latest last. */
const KEPT = 20;

/** The changes that can be taken back, the latest last. */
const taken: Taken[] = [];

/** How long a notice that offers to take a change back stays, in ms. */
const UNDO_MS = 6000;

/** Changes how the person keeps a thread, a change that puts it away offered back. */
export function useMark(): (row: Listed, change: MarksChange) => void {
	const client = useClient();
	return (row, change) => {
		const undoable = undoOf(row.marks, change, Date.now());
		client
			.mark(row.id, change)
			.then(() => {
				if (undoable === undefined) return;
				const entry: Taken = { thread: row.id, undo: undoable.undo, notice: "" };
				const notice = notices.add({
					title: undoable.words,
					description: threadTitle(row),
					timeout: UNDO_MS,
					actionProps: { children: "Undo", onClick: () => takeBack(client, entry) },
				});
				taken.push({ ...entry, notice });
				if (taken.length > KEPT) taken.shift();
			})
			.catch((error: unknown) => failed("The thread could not be changed", error));
	};
}

/** Takes back the latest change with ⌘Z, wherever no text field has the focus, which keeps its own. */
export function useUndo(): void {
	const client = useClient();
	useShortcutOutsideText("mod+z", () => {
		const latest = taken.at(-1);
		if (latest !== undefined) takeBack(client, latest);
	});
}

/** Takes `entry` back, once: its change undone, its notice gone. */
function takeBack(client: Client, entry: Taken): void {
	const at = taken.findIndex((each) => each.thread === entry.thread && each.undo === entry.undo);
	if (at < 0) return;
	const [kept] = taken.splice(at, 1);
	if (kept !== undefined && kept.notice !== "") notices.close(kept.notice);
	client.mark(entry.thread, entry.undo(Date.now())).catch((error: unknown) => failed("The change could not be taken back", error));
}
