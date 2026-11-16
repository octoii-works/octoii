/**
 * The `thread` tool as the page shows a call of it: the threads listed, searched by the keywords asked for, or a thread
 * read; or a thread started, messaged or stopped, by its title or its message.
 */
import { toolView, type ToolView, type Verbs } from "../../core/tools.ts";

/** The `thread` tool's verbs, by its action. */
const THREAD_VERBS: Readonly<Record<string, Verbs>> = {
	list: ["List threads", "Listing threads", "Listed threads"],
	search: ["Search threads", "Searching threads", "Searched threads"],
	read: ["Read a thread", "Reading a thread", "Read a thread"],
	start: ["Start a thread", "Starting a thread", "Started a thread"],
	send: ["Message a thread", "Messaging a thread", "Messaged a thread"],
	stop: ["Stop a thread", "Stopping a thread", "Stopped a thread"],
	rename: ["Rename a thread", "Requesting a new title", "Requested a new title"],
	pin: ["Pin a thread", "Pinning a thread", "Pinned a thread"],
	unpin: ["Unpin a thread", "Unpinning a thread", "Unpinned a thread"],
	archive: ["Archive a thread", "Archiving a thread", "Archived a thread"],
	unarchive: ["Unarchive a thread", "Unarchiving a thread", "Unarchived a thread"],
	snooze: ["Snooze a thread", "Snoozing a thread", "Snoozed a thread"],
	unsnooze: ["Unsnooze a thread", "Unsnoozing a thread", "Unsnoozed a thread"],
};

/** The threads extension's tools. */
export const THREAD_TOOLS: Readonly<Record<string, ToolView>> = {
	thread: toolView({
		work: "thread",
		verbs: (given) => THREAD_VERBS[given.action ?? ""] ?? ["Work with a thread", "Working with a thread", "Worked with a thread"],
		subject: (given) => given.title ?? given.message?.split("\n")[0] ?? given.keywords,
	}),
};
