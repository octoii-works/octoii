/** Types shared by HTTP requests and watch subscriptions, generated from the Node. */
import type { Event, Methods } from "./generated/protocol.ts";

export * from "./generated/protocol.ts";

export type MethodName = keyof Methods;
export type ParamsOf<Name extends MethodName> = Methods[Name]["params"];
export type ResultOf<Name extends MethodName> = Methods[Name]["result"];

/** An event of a subscription: what it tells of its topic. */
export type SubscriptionEvent = Exclude<Extract<Event, { readonly subscription: string }>, { readonly event: "lagged" }>;

/** The events of a kind, by its name. */
export type EventOf<Name extends SubscriptionEvent["event"]> = Extract<SubscriptionEvent, { readonly event: Name }>;
