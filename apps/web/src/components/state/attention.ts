/**
 * What the person is told of their threads as it happens: where the page is in front of them, a notice that opens the
 * thread — none for the thread they read; where it is not, the system's notification, one a thread, which brings the
 * page forward on the thread as it is clicked, where they let the page notify, else the notice, there as they come
 * back; and the page's title counts the threads that wait on them.
 */
import type { Listed } from "@octoii-works/octoii-client";
import { useThreads } from "@octoii-works/octoii-client/react";
import { type Attention, attentionOf, attentionWords, waitingCount } from "../../core/attention.ts";
import { threadTitle } from "../../core/threads.ts";
import { useChanges, useTitle } from "../effects/attention.ts";
import { go } from "./navigation.ts";
import { notices } from "./notices.ts";

/** No threads, as the list stands before it is read. */
const NONE: readonly Listed[] = [];

/** How long a notice of a thread stays, in ms. */
const NOTICE_MS = 8000;

/** Tells the person of their threads as it happens, `reading` the thread they have open. */
export function useAttention(reading: string | undefined): void {
	const rows = useThreads() ?? NONE;
	useChanges(rows, (before, after) => {
		for (const attention of attentionOf(before, after)) tell(attention, after, reading);
	});
	const waiting = waitingCount(rows);
	useTitle(waiting === 0 ? "Octoii" : `(${String(waiting)}) Octoii`);
}

/** Tells the person `attention`, by the page or by the system, as the page stands before them. */
function tell(attention: Attention, rows: readonly Listed[], reading: string | undefined): void {
	const row = rows.find((each) => each.id === attention.thread);
	const title = row === undefined ? "A thread" : threadTitle(row);
	const words = attentionWords(attention);
	const open = (): void => go({ place: "thread", thread: attention.thread });
	const inFront = document.visibilityState === "visible" && document.hasFocus();
	if (inFront && attention.thread === reading) return;
	if (!inFront && typeof Notification !== "undefined" && Notification.permission === "granted") {
		const notification = new Notification(title, { body: words, tag: attention.thread });
		notification.addEventListener("click", () => {
			window.focus();
			open();
			notification.close();
		});
		return;
	}
	notices.add({
		title: words,
		description: title,
		timeout: NOTICE_MS,
		...(attention.kind === "failed" ? { type: "error" } : {}),
		actionProps: { children: "Open", onClick: open },
	});
}
