/** All views of a request share its submission state for the lifetime of this client. */
import { useSyncExternalStore } from "react";
import type { Client } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { Answers } from "../../core/answers.ts";

const clients = new WeakMap<Client, Answers>();

export function useAnswer(thread: string, key: string | undefined) {
	const client = useClient();
	let answers = clients.get(client);
	if (!answers) {
		answers = new Answers(client);
		clients.set(client, answers);
	}
	const store = answers;
	const answering = useSyncExternalStore(store.subscribe, () => key !== undefined && store.pending(thread, key));
	return { answering, submit: (value: Parameters<Client["answer"]>[2]) => (key === undefined ? Promise.resolve(false) : store.submit(thread, key, value)) };
}
