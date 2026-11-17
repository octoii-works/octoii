/** In-flight UI state only. Durable readiness and cancellation still come from the Thread. */
import { useSyncExternalStore } from "react";
import type { Client } from "@octoii-works/octoii-client";
const active = new WeakMap<Client, Set<string>>();
const listeners = new Set<() => void>();
function subscribe(changed: () => void): () => void {
	listeners.add(changed);
	return () => listeners.delete(changed);
}
export function preparingWorktree(client: Client, thread: string): () => void {
	const threads = active.get(client) ?? new Set<string>();
	active.set(client, threads);
	threads.add(thread);
	for (const changed of listeners) changed();
	return () => {
		threads.delete(thread);
		for (const changed of listeners) changed();
	};
}
export function useWorktreePreparing(client: Client, thread: string): boolean {
	return useSyncExternalStore(subscribe, () => active.get(client)?.has(thread) ?? false);
}
