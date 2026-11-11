/**
 * Octoii's protocol, as the node speaks it over one WebSocket: its types, made from the node's own, and the version the
 * socket names. A client's call is JSON — its id, its method and the method's params — answered once by its id, with
 * its result or the error it was refused with; what a client subscribes to comes as events by the subscription's id,
 * a terminal's output and a page's frames as binary messages, the event's JSON ahead of its bytes.
 */
import type { Event, Methods } from "./generated/protocol.ts";

export * from "./generated/protocol.ts";

/** The protocol's version, as the socket names it in `Sec-WebSocket-Protocol`. */
export const PROTOCOL = "octoii.1";

export type MethodName = keyof Methods;
export type ParamsOf<Name extends MethodName> = Methods[Name]["params"];
export type ResultOf<Name extends MethodName> = Methods[Name]["result"];

/** An event of a subscription: what it tells of its topic. */
export type SubscriptionEvent = Exclude<Extract<Event, { readonly subscription: string }>, { readonly event: "lagged" }>;

/** The events of a kind, by its name. */
export type EventOf<Name extends SubscriptionEvent["event"]> = Extract<SubscriptionEvent, { readonly event: Name }>;
