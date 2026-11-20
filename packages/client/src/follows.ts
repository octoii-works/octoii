/**
 * One watch for each followed thread, with older history fetched on demand.
 */
import { READ_BACK_PAGE } from "./constants.ts";
import { type Followable, holding, type Reading } from "./followed.ts";
import type { Requests } from "./requests.ts";
import type { Subscription, Subscriptions } from "./subscriptions.ts";
import { FollowedThread, type ThreadView } from "./thread.ts";

type Following = Reading<FollowedThread> & { subscription: Subscription };

export class Follows {
	readonly #followed = new Map<string, Following>();
	readonly #requests: Requests;
	readonly #subscriptions: Subscriptions;
	/** How many of a thread's latest entries are kept. */
	readonly #kept: number;
	constructor(requests: Requests, subscriptions: Subscriptions, kept: number) {
		this.#requests = requests;
		this.#subscriptions = subscriptions;
		this.#kept = kept;
	}

	/** One more reader of the thread `id`: one following of it for all who read it, begun as the first comes. */
	follow(id: string): Followable<ThreadView> {
		let following = this.#followed.get(id);
		if (following === undefined) {
			const thread = new FollowedThread(this.#kept);
			const subscription = this.#subscriptions.hold({ type: "thread", thread: id, limit: this.#kept }, (event) => thread.take(event));
			following = { thing: thread, holders: 0, subscription };
			this.#followed.set(id, following);
		}
		const held = following;
		const thread = held.thing;
		return holding(
			held,
			thread.subscribe.bind(thread),
			() => thread.view,
			() => {
				this.#followed.delete(id);
				held.subscription.stop();
			},
		);
	}

	/** Reads back a page of the thread `id` before the earliest entry its readers hold, where its log goes back further. */
	async readOlder(id: string): Promise<void> {
		const thread = this.#followed.get(id)?.thing;
		const first = thread?.view.entries[0];
		if (thread === undefined || first === undefined || thread.view.complete || thread.view.gone) return;
		const page = await this.#requests.call("threads.read", { thread: id, before: first.id, limit: READ_BACK_PAGE });
		if (this.#followed.get(id)?.thing === thread && !thread.view.gone && first.id === thread.view.entries[0]?.id) thread.prepend(page.entries, page.complete);
	}
}
