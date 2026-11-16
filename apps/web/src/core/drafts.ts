import type { ElementFeedback } from "@octoii-works/octoii-client";
import type { Attachment } from "./attachments.ts";

/** A message being written, including captures explicitly chosen from its thread's browser. */
export type Draft = { readonly text: string; readonly attachments: readonly Attachment[]; readonly feedback: readonly ElementFeedback[] };

export const EMPTY_DRAFT: Draft = { text: "", attachments: [], feedback: [] };

/** Clears the submitted snapshot, preserving text edited and files or captures added while sending. */
export function afterSubmission(current: Draft, submitted: Draft): Draft {
	return {
		text: current.text === submitted.text ? "" : current.text,
		attachments: current.attachments.filter((file) => !submitted.attachments.includes(file)),
		feedback: current.feedback.filter((capture) => !submitted.feedback.includes(capture)),
	};
}

/** Where a draft belongs: a thread, by its id, or a new thread of a project's folder, or of none. */
export type DraftPlace = { readonly thread: string } | { readonly project: string | undefined };

/** A draft's key: its thread's id, or `new:` and its project's folder. */
export function draftKey(place: DraftPlace): string {
	return "thread" in place ? place.thread : `new:${place.project ?? ""}`;
}
