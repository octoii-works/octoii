/**
 * The list of threads as a client watches it: the list as it stood when its subscription was made, then each thread's
 * row as the node tells it changed, newest thread first. A subscription made again — after a lost connection, or where
 * it fell behind — starts from the list anew.
 */
import type { Listed, SubscriptionEvent } from "./protocol.ts";

/** The threads, newest first; none until the node answered the watch. */
export type ThreadsView = readonly Listed[] | undefined;

export class WatchedThreads {
	#rows: Map<string, Listed> | undefined;
	#view: ThreadsView;
	readonly #listeners = new Set<() => void>();

	/** The threads as far as the client knows them: the same array until one changes. */
	get view(): ThreadsView {
		return this.#view;
	}

	subscribe(listener: () => void): () => void {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	}

	/** Takes an event of the list's subscription. */
	take(event: SubscriptionEvent): void {
		if (event.event === "threads") this.reset(event.threads);
		else if (event.event === "thread") this.put(event.thread);
		else if (event.event === "thread_gone") this.forget(event.thread);
	}

	/** Takes the list as it stands, as its subscription tells it first. */
	reset(rows: readonly Listed[]): void {
		this.#rows = new Map(rows.map((row) => [row.id, row]));
		this.#publish();
	}

	/** Takes a thread's row as it changed; one before the list came is in the list when it comes. */
	put(row: Listed): void {
		if (this.#rows === undefined) return;
		this.#rows.set(row.id, row);
		this.#publish();
	}

	/** Lets go of the row of a thread gone, as it was deleted. */
	forget(id: string): void {
		if (this.#rows?.delete(id) !== true) return;
		this.#publish();
	}

	#publish(): void {
		// A thread's id is minted as it is made, later ids sorting after: the newest thread first.
		this.#view = [...(this.#rows?.values() ?? [])].sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
		for (const listener of [...this.#listeners]) listener();
	}
}
