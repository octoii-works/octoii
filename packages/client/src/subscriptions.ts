/**
 * The client's subscriptions, as Redis's clients hold theirs: each a topic and what takes its events, made on the
 * connection that is open and again on each that opens after, and again — from where it stood, as the node says — where
 * it fell behind; let go of once its holder stops it. A holder moves its topic on as it takes what it is told, so it is
 * made again from where it stands.
 */
import type { Connection } from "./connection.ts";
import type { Event, SubscriptionEvent, Topic } from "./protocol.ts";

/** What takes a subscription's events, with the bytes a binary message carries. */
export type Take = (event: SubscriptionEvent, bytes: Uint8Array | undefined) => void;

/** A subscription its holder keeps: the topic it is made again with, and how to let it go. */
export type Subscription = { topic: Topic; stop(): void };

type Held = { topic: Topic; readonly take: Take; id: string | undefined; stopped: boolean };

export class Subscriptions {
	readonly #held = new Set<Held>();
	readonly #byId = new Map<string, Held>();
	readonly #connection: Connection;
	/** Tells of a failure that is not the connection's own. */
	readonly #told: (error: unknown) => void;

	constructor(connection: Connection, told: (error: unknown) => void) {
		this.#connection = connection;
		this.#told = told;
	}

	/** Makes each subscription again, as a connection opens. */
	again(): void {
		this.#byId.clear();
		for (const held of this.#held) this.#subscribe(held);
	}

	/** Subscribes to `topic`, its events given to `take`, till it is stopped. */
	hold(topic: Topic, take: Take): Subscription {
		const held: Held = { topic, take, id: undefined, stopped: false };
		this.#held.add(held);
		if (this.#connection.open) this.#subscribe(held);
		return {
			get topic() {
				return held.topic;
			},
			set topic(moved) {
				held.topic = moved;
			},
			stop: () => this.#stop(held),
		};
	}

	/** Gives an event to its subscription's holder; a subscription that fell behind made again from where it stood. */
	take(event: Extract<Event, { readonly subscription: string }>, bytes: Uint8Array | undefined): void {
		const held = this.#byId.get(event.subscription);
		if (held === undefined) return;
		if (event.event !== "lagged") return held.take(event, bytes);
		this.#byId.delete(event.subscription);
		held.topic = event.topic;
		this.#subscribe(held);
	}

	#subscribe(held: Held): void {
		held.id = undefined;
		this.#connection.call("subscribe", { topic: held.topic }).then(({ subscription }) => {
			if (held.stopped) return void this.#connection.call("unsubscribe", { subscription }).catch(this.#told);
			held.id = subscription;
			this.#byId.set(subscription, held);
		}, this.#told);
	}

	#stop(held: Held): void {
		if (held.stopped) return;
		held.stopped = true;
		this.#held.delete(held);
		if (held.id === undefined) return;
		this.#byId.delete(held.id);
		if (this.#connection.open) this.#connection.call("unsubscribe", { subscription: held.id }).catch(this.#told);
	}
}
