/**
 * A thread's committed view and live output, held independently. A new snapshot reseeds the committed view.
 */
import type { Entry, Status, Streamed, SubscriptionEvent } from "./protocol.ts";

type LiveStream = { readonly key: string; readonly offset: bigint; readonly parts: readonly Streamed[] };

/** A followed thread as the interface reads it. */
export type ThreadView = {
	/** Its latest entries, as many as the client keeps, and those read further back. */
	readonly entries: readonly Entry[];
	/** Whether its latest entries were read: till they are, what it holds is not the thread yet. */
	readonly read: boolean;
	/** Whether the entries reach the log's start: none is left to read further back. */
	readonly complete: boolean;
	/** Its status, once the node told it. */
	readonly status: Status | undefined;
	/** Live output by exact request identity. */
	readonly streaming: ReadonlyMap<string, LiveStream>;
	/** Whether the thread is gone, as it was deleted. */
	readonly gone: boolean;
};

/** A thread the client follows, and those who read it. */
export class FollowedThread {
	/** How many entries it keeps: as many as the client keeps, and the pages read further back. */
	#bound: number;
	#view: ThreadView = { entries: [], read: false, complete: false, status: undefined, streaming: new Map(), gone: false };
	readonly #listeners = new Set<() => void>();

	constructor(kept: number) {
		this.#bound = kept;
	}

	/** Takes entries read further back, before those it holds: it keeps them too. */
	prepend(entries: readonly Entry[], complete: boolean): void {
		this.#bound += entries.length;
		this.#publish({ ...this.#view, entries: [...entries, ...this.#view.entries], complete });
	}

	/** What the thread is, as far as the client has read it: the same object until it changes. */
	get view(): ThreadView {
		return this.#view;
	}

	subscribe(listener: () => void): () => void {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	}

	/** Takes an event of the thread's subscription. */
	take(event: SubscriptionEvent): void {
		const view = this.#view;
		switch (event.event) {
			case "thread_snapshot": {
				const { history, status } = event;
				const first = history.entries[0]?.id;
				const shared = history.complete ? -1 : view.entries.findIndex((entry) => entry.id === first);
				const entries = shared > 0 ? [...view.entries.slice(0, shared), ...history.entries] : history.entries;
				const complete = shared > 0 ? view.complete : history.complete;
				return this.#publish(this.#bounded({ ...view, entries, status, read: true, complete, gone: false }));
			}
			case "thread_update": {
				const entries = event.entries.length === 0 ? view.entries : [...view.entries, ...event.entries];
				return this.#publish(this.#bounded({ ...view, entries, status: event.status }));
			}
			case "stream": {
				const offset = BigInt(event.offset);
				const held = view.streaming.get(event.request);
				if (held !== undefined && offset <= held.offset) return;
				const streaming = new Map(view.streaming);
				streaming.set(event.request, { key: event.key, offset, parts: [...(held?.parts ?? []), event.data] });
				return this.#publish({ ...view, streaming });
			}
			case "stream_closed": {
				if (!view.streaming.has(event.request)) return;
				const streaming = new Map(view.streaming);
				streaming.delete(event.request);
				return this.#publish({ ...view, streaming });
			}
			case "streams_reset":
				if (view.streaming.size === 0) return;
				return this.#publish({ ...view, streaming: new Map() });
			case "thread_gone":
				return this.#publish({ ...view, read: true, gone: true, streaming: new Map() });
			default:
		}
	}

	#bounded(view: ThreadView): ThreadView {
		if (view.entries.length <= this.#bound) return view;
		return { ...view, entries: view.entries.slice(-this.#bound), complete: false };
	}

	#publish(view: ThreadView): void {
		this.#view = view;
		for (const listener of [...this.#listeners]) listener();
	}
}
