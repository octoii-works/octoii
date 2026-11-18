/**
 * A thread as a client follows it: its log's latest entries — as many as it keeps, read first, then each as it is
 * appended, the oldest let go past the bound, as a terminal keeps its latest lines — and older ones only as a reader
 * reads further back. The subscription resumes log entries from its cursor. Live output is held by request identity
 * and offset until that request closes. A stream reset rebuilds live output from retained parts and leaves the log intact.
 */
import type { Cursor, Entry, EventOf, Status, Streamed, SubscriptionEvent } from "./protocol.ts";

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
	readonly id: string;
	#cursor: Cursor | undefined;
	/** How many entries it keeps: as many as the client keeps, and the pages read further back. */
	#bound: number;
	#view: ThreadView = { entries: [], read: false, complete: false, status: undefined, streaming: new Map(), gone: false };
	readonly #listeners = new Set<() => void>();

	constructor(id: string, kept: number) {
		this.id = id;
		this.#bound = kept;
	}

	/** Whether its latest entries were read, so it is followed on from where its reader stands. */
	get begun(): boolean {
		return this.#view.read;
	}

	/** Takes its latest entries as a read of them gave them: where the log then ended, it is followed on from. */
	begin(entries: readonly Entry[], end: Cursor | undefined, complete: boolean): void {
		this.#cursor = end;
		this.#publish({ ...this.#view, entries, read: true, complete });
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

	/** Where its reader stands; none before the first entry came. */
	get cursor(): Cursor | undefined {
		return this.#cursor;
	}

	subscribe(listener: () => void): () => void {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	}

	/** Takes an event of the thread's subscription. */
	take(event: SubscriptionEvent): void {
		const view = this.#view;
		switch (event.event) {
			case "entries":
				this.#cursor = event.cursor;
				return this.#publish(appended(view, event, this.#bound));
			case "status":
				return this.#publish({ ...view, status: event.status });
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
				return this.#publish({ ...view, gone: true });
			default:
		}
	}

	#publish(view: ThreadView): void {
		this.#view = view;
		for (const listener of [...this.#listeners]) listener();
	}
}

/** `view` with the durable entries appended, the oldest past `bound` let go. */
function appended(view: ThreadView, event: EventOf<"entries">, bound: number): ThreadView {
	const all = [...view.entries, ...event.entries];
	// Past its bound, the oldest entries are let go: the log keeps them, to read back.
	const entries = all.length > bound ? all.slice(all.length - bound) : all;
	return { ...view, entries, complete: view.complete && entries.length === all.length };
}
