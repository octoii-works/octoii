import type { Conversation } from "@octoii-works/octoii-client";

const PERSON = "person";

/** The thread whose an input is, by its actor. The person's input has no sending thread. */
export function threadOfActor(actor: string): string | undefined {
	return actor === PERSON ? undefined : actor;
}

/** The person's currently visible inbox messages, by the IDs they can be withdrawn with. */
export function queuedInputsOf(inbox: Conversation["inbox"]) {
	return inbox.flatMap(([id, input]) => (input.type === "input" && input.actor === PERSON ? [{ id, input }] : []));
}
