/**
 * The threads the client follows: one following of each for all who read it — read first, its latest entries alone,
 * then subscribed to from where its log ended as it was read, the subscription made again from where its reader stands
 * as each connection opens; read back a page at a time as a reader asks; let go once its last reader does.
 */
import type { Connection } from "./connection.ts";
import { READ_BACK_PAGE } from "./constants.ts";
import { type Followable, holding, type Reading } from "./followed.ts";
import type { Subscription, Subscriptions } from "./subscriptions.ts";
import { FollowedThread, type ThreadView } from "./thread.ts";

type Following = Reading<FollowedThread> & { subscription: Subscription | undefined };

export class Follows {
	readonly #followed = new Map<string, Following>();
	readonly #connection: Connection;
	readonly #subscriptions: Subscriptions;
	/** How many of a thread's latest entries are kept. */
	readonly #kept: number;
	/** Tells of a failure that is not the connection's own. */
	readonly #told: (error: unknown) => void;

	constructor(connection: Connection, subscriptions: Subscriptions, kept: number, told: (error: unknown) => void) {
		this.#connection = connection;
		this.#subscriptions = subscriptions;
		this.#kept = kept;
		this.#told = told;
	}

	/** Reads each thread whose latest entries were not read yet, as a connection opens: the others' subscriptions are made again. */
	again(): void {
		for (const following of this.#followed.values()) if (following.subscription === undefined) this.#begin(following);
	}

	/** One more reader of the thread `id`: one following of it for all who read it, begun as the first comes. */
	follow(id: string): Followable<ThreadView> {
		let following = this.#followed.get(id);
		if (following === undefined) {
			following = { thing: new FollowedThread(id, this.#kept), holders: 0, subscription: undefined };
			this.#followed.set(id, following);
			if (this.#connection.open) this.#begin(following);
		}
		const held = following;
		const thread = held.thing;
		return holding(
			held,
			thread.subscribe.bind(thread),
			() => thread.view,
			() => {
				this.#followed.delete(id);
				held.subscription?.stop();
			},
		);
	}

	/** Reads back a page of the thread `id` before the earliest entry its readers hold, where its log goes back further. */
	async readOlder(id: string): Promise<void> {
		const thread = this.#followed.get(id)?.thing;
		const first = thread?.view.entries[0];
		if (thread === undefined || thread.view.complete) return;
		const page = await this.#connection.call(
			"threads.read",
			first === undefined ? { thread: id, limit: READ_BACK_PAGE } : { thread: id, before: first.id, limit: READ_BACK_PAGE },
		);
		thread.prepend(page.entries, page.complete);
	}

	/** A thread is read first, its latest entries alone, then subscribed to from where the log ended as it was read. */
	#begin(following: Following): void {
		const thread = following.thing;
		this.#connection
			.call("threads.read", { thread: thread.id, limit: this.#kept })
			.then((page) => {
				if (this.#followed.get(thread.id) !== following || following.subscription !== undefined) return;
				thread.begin(page.entries, page.end, page.complete);
				const topic = { type: "thread" as const, thread: thread.id, ...(page.end === undefined ? {} : { from: page.end }) };
				const subscription = this.#subscriptions.hold(topic, (event) => {
					thread.take(event);
					if (event.event === "entries") subscription.topic = { type: "thread", thread: thread.id, from: event.cursor };
				});
				following.subscription = subscription;
			})
			.catch(this.#told);
	}
}
