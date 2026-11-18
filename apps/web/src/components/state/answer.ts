/** All views of a request share its submission state for the lifetime of this client. */
import { useSyncExternalStore } from "react";
import type { Client } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { Answers } from "../../core/answers.ts";

const clients = new WeakMap<Client, Answers>();

export function useAnswer(thread: string, request: string | undefined) {
	const client = useClient();
	let answers = clients.get(client);
	if (!answers) {
		answers = new Answers(client);
		clients.set(client, answers);
	}
	const store = answers;
	const answering = useSyncExternalStore(store.subscribe, () => request !== undefined && store.pending(thread, request));
	return {
		answering,
		submit: (value: Parameters<Client["answer"]>[2]) => (request === undefined ? Promise.resolve(false) : store.submit(thread, request, value)),
	};
}
