/**
 * A thread as a client follows it: its log's latest entries — as many as it keeps, read first, then each as it is
 * appended, the oldest let go past the bound, as a terminal keeps its latest lines — and older ones only as a reader
 * reads further back; its status as it changes; and what streams of each of its model's requests while it is answered,
 * by its offset, let go once the log holds what came of the request. Its subscription goes on from where its reader
 * stands, as a reader of a stream resumes from an offset, so every entry is taken once; what streamed, sent again from
 * each stream's start, is taken past what is held.
 */
import type { Cursor, Entry, EventOf, Status, Streamed, SubscriptionEvent } from "./protocol.ts";

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
	/** What streamed of each request the log holds nothing of yet, by the request's id, in the order it came. */
	readonly streaming: ReadonlyMap<string, readonly Streamed[]>;
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
				// A stream is taken in order, each event at the offset that follows what is held: what comes again, as a stream
				// sent from its start on subscribing again, is left.
				const held = view.streaming.get(event.request) ?? [];
				if (event.offset !== held.length) return;
				return this.#publish({ ...view, streaming: new Map([...view.streaming, [event.request, [...held, event.data]]]) });
			}
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

/** `view` with `event`'s entries appended, the oldest past `bound` let go, and the streams of the requests they settle. */
function appended(view: ThreadView, event: EventOf<"entries">, bound: number): ThreadView {
	const all = [...view.entries, ...event.entries];
	// Past its bound, the oldest entries are let go: the log keeps them, to read back.
	const entries = all.length > bound ? all.slice(all.length - bound) : all;
	const settled = new Set(event.entries.flatMap((entry) => ("request" in entry ? [entry.request] : [])));
	const streaming = [...view.streaming.keys()].some((request) => settled.has(request))
		? new Map([...view.streaming].filter(([request]) => !settled.has(request)))
		: view.streaming;
	return { ...view, entries, complete: view.complete && entries.length === all.length, streaming };
}
