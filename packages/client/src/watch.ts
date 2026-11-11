/**
 * What the client watches for its readers — the list of threads, the settings, a thread's terminals or its pages — by a
 * key of its own: one subscription for all who read it, made as its first reader comes, what it is told held for them,
 * and let go once its last reader lets it go.
 */
import { type Followable, holding, type Reading } from "./followed.ts";
import type { SubscriptionEvent, Topic } from "./protocol.ts";
import type { Subscription, Subscriptions } from "./subscriptions.ts";

/** What a watch is: what holds what it is told, what a reader sees of it and hears change, its topic, and how it takes an event. */
export type WatchOf<Thing, View> = {
	readonly make: () => Thing;
	readonly view: (thing: Thing) => View;
	readonly subscribe: (thing: Thing, listener: () => void) => () => void;
	readonly topic: Topic;
	readonly take: (thing: Thing, event: SubscriptionEvent) => void;
};

type Watched = Reading<unknown> & { readonly subscription: Subscription };

export class Watches {
	readonly #watched = new Map<string, Watched>();
	readonly #subscriptions: Subscriptions;

	constructor(subscriptions: Subscriptions) {
		this.#subscriptions = subscriptions;
	}

	/** One more reader of the watch of `key`, as `of` makes it where none is held: one watch for all who read it. */
	hold<Thing, View>(key: string, of: () => WatchOf<Thing, View>): Followable<View> {
		const watch = of();
		let watched = this.#watched.get(key);
		if (watched === undefined) {
			const made = watch.make();
			const subscription = this.#subscriptions.hold(watch.topic, (event) => watch.take(made, event));
			watched = { thing: made, holders: 0, subscription };
			this.#watched.set(key, watched);
		}
		// SAFETY: each key is held by one caller, whose `of` makes the same kind of thing every time.
		const thing = watched.thing as Thing;
		const { subscription } = watched;
		return holding(
			watched,
			(listener) => watch.subscribe(thing, listener),
			() => watch.view(thing),
			() => {
				this.#watched.delete(key);
				subscription.stop();
			},
		);
	}
}
