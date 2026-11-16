/**
 * Drafts: the next message of each place — a thread, or a new thread of a project — its words and the files it gives,
 * kept on the page while the person goes between places. The composer edits a place's draft.
 */
import { useSyncExternalStore } from "react";
import type { ElementFeedback } from "@octoii-works/octoii-client";
import type { Attachment } from "../../core/attachments.ts";
import { type Draft, type DraftPlace, EMPTY_DRAFT, afterSubmission, draftKey } from "../../core/drafts.ts";

/** A place's draft, and how it changes. */
export type DraftHandle = {
	readonly text: string;
	readonly attachments: readonly Attachment[];
	readonly feedback: readonly ElementFeedback[];
	readonly setText: (text: string) => void;
	/** Adds a file to the draft as it stands then, as files kept one by one arrive in any order. */
	readonly attach: (attachment: Attachment) => void;
	/** Takes the file `ref` away from the draft. */
	readonly detach: (ref: string) => void;
	readonly removeFeedback: (feedback: ElementFeedback) => void;
	/** Clears only this submitted snapshot after acknowledgement, retaining anything edited or added meanwhile. */
	readonly clearSubmitted: () => void;
	/** Clears only the submitted text, preserving newer edits and attachments that were not sent. */
	readonly clearText: () => void;
};

const drafts = new Map<string, Draft>();
const listeners = new Set<() => void>();

function changed(key: string, change: Partial<Draft>): void {
	const next = { ...(drafts.get(key) ?? EMPTY_DRAFT), ...change };
	if (next.text === "" && next.attachments.length === 0 && next.feedback.length === 0) drafts.delete(key);
	else drafts.set(key, next);
	for (const listener of listeners) listener();
}

/** A capture always belongs to the thread whose files contain it, even if another thread is now on screen. */
export function attachElementFeedback(feedback: ElementFeedback): void {
	const key = draftKey({ thread: feedback.thread });
	changed(key, { feedback: [...(drafts.get(key) ?? EMPTY_DRAFT).feedback, feedback] });
}

function subscribe(listener: () => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** A place's draft, followed as anything changes it. */
export function useDraft(place: DraftPlace): DraftHandle {
	const key = draftKey(place);
	const draft = useSyncExternalStore(subscribe, () => drafts.get(key) ?? EMPTY_DRAFT);
	return {
		text: draft.text,
		attachments: draft.attachments,
		feedback: draft.feedback,
		setText: (text) => changed(key, { text }),
		attach: (attachment) => changed(key, { attachments: [...(drafts.get(key) ?? EMPTY_DRAFT).attachments, attachment] }),
		detach: (ref) => changed(key, { attachments: (drafts.get(key) ?? EMPTY_DRAFT).attachments.filter((each) => each.ref !== ref) }),
		removeFeedback: (feedback) => changed(key, { feedback: (drafts.get(key) ?? EMPTY_DRAFT).feedback.filter((each) => each !== feedback) }),
		clearSubmitted: () => changed(key, afterSubmission(drafts.get(key) ?? EMPTY_DRAFT, draft)),
		clearText: () => {
			if ((drafts.get(key) ?? EMPTY_DRAFT).text === draft.text) changed(key, { text: "" });
		},
	};
}
