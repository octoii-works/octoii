/**
 * The connection: one WebSocket to the node, speaking the protocol's version, opened as the client starts and again
 * after it drops, waiting longer after each drop up to a bound, the socket authenticated as the app opens it. The node
 * welcomes it first, with the session it knows the client by, its scopes, the machine and its extensions; then what comes is read in
 * order: an answer settles the call of its id; an event goes to the client — with its bytes, where it came as a binary
 * message — which subscribes again to what it follows on each connection that opens.
 */
import { RECONNECT_MAX_MS, RECONNECT_MIN_MS } from "./constants.ts";
import { ClientError, clientError, isConnectionError } from "./errors.ts";
import { Value } from "./followed.ts";
import type { ClientOptions, WebSocketLike } from "./options.ts";
import {
	type Event,
	type ExtensionListed,
	type Machine,
	type MethodName,
	type ParamsOf,
	PROTOCOL,
	type Received,
	type ResultOf,
	type Scope,
} from "./protocol.ts";

/** Where the connection stands. */
export type ConnectionStatus =
	| { readonly status: "connecting"; readonly attempt: number }
	| {
			readonly status: "open";
			readonly session: string;
			readonly scopes: readonly Scope[];
			readonly machine: Machine;
			readonly extensions: readonly ExtensionListed[];
	  }
	| { readonly status: "waiting"; readonly until: number; readonly error?: ClientError }
	| { readonly status: "stopped"; readonly error: ClientError };

/** What the connection tells its client: each time one opens, and each event the node sends, with its bytes where it has some. */
export type ConnectionHandlers = { readonly opened: () => void; readonly event: (event: Event, bytes: Uint8Array | undefined) => void };

/** What settles a call: the result the node answered with, or why there is none. */
type Settle = (answer: { readonly result: unknown } | { readonly error: ClientError }) => void;

/** One connection, opened again after it drops, until it is stopped. */
export class Connection {
	readonly status = new Value<ConnectionStatus>({ status: "connecting", attempt: 1 });
	readonly #options: ClientOptions;
	readonly #handlers: ConnectionHandlers;
	readonly #pending = new Map<string, Settle>();
	#socket: WebSocketLike | undefined;
	#cancelWait: (() => void) | undefined;
	/** The drops since a connection last opened: the next wait doubles with each. */
	#drops = 0;
	#counter = 0;

	constructor(options: ClientOptions, handlers: ConnectionHandlers) {
		this.#options = options;
		this.#handlers = handlers;
	}

	start(): void {
		this.#connect();
	}

	/** Whether a connection is open. */
	get open(): boolean {
		return this.status.value.status === "open";
	}

	/** Calls a method on the open connection: its result; rejects with `not-connected` where none is open. */
	call<Name extends MethodName>(method: Name, params: ParamsOf<Name>): Promise<ResultOf<Name>> {
		if (!this.open) return Promise.reject(clientError("not-connected", "No connection to the node is open."));
		return new Promise((resolve, reject) => {
			const id = String(++this.#counter);
			// SAFETY: the node answers a call by its id with its method's result, as the protocol its types are made from says.
			this.#pending.set(id, (answer) => ("error" in answer ? reject(answer.error) : resolve(answer.result as ResultOf<Name>)));
			this.#socket?.send(JSON.stringify({ id, method, params }));
		});
	}

	/**
	 * Calls a method once a connection is open, and again on the next where it drops before the answer: for a method done
	 * once however often it is asked, as an operation's key makes it. Rejects where the client stopped, or with the node's
	 * error.
	 */
	async callOpen<Name extends MethodName>(method: Name, params: ParamsOf<Name>): Promise<ResultOf<Name>> {
		for (;;) {
			await this.#ready();
			try {
				return await this.call(method, params);
			} catch (error) {
				if (!isConnectionError(error)) throw error;
			}
		}
	}

	/** Stops connecting for good: the client is closed. */
	close(): void {
		this.#stop(clientError("closed", "The client was closed."));
	}

	#ready(): Promise<void> {
		return new Promise((resolve, reject) => {
			const settled = (): boolean => {
				const now = this.status.value;
				if (now.status === "open") resolve();
				else if (now.status === "stopped") reject(now.error);
				else return false;
				return true;
			};
			if (settled()) return;
			const stop = this.status.subscribe(() => {
				if (settled()) stop();
			});
		});
	}

	#connect(): void {
		this.status.set({ status: "connecting", attempt: this.#drops + 1 });
		const socket = this.#options.open(PROTOCOL);
		socket.binaryType = "arraybuffer";
		this.#socket = socket;
		socket.addEventListener("message", (event) => {
			if (this.#socket === socket) this.#receive(event.data);
		});
		socket.addEventListener("close", () => this.#dropped(socket));
	}

	/** Takes one message the node sent: its welcome opens the connection; an answer settles its call; an event goes to the client. */
	#receive(data: unknown): void {
		let received: Received;
		let bytes: Uint8Array | undefined;
		try {
			// SAFETY: what the node sends is the protocol's, made from the node's own types, of the version the socket names.
			[received, bytes] = data instanceof ArrayBuffer ? framed(data) : [JSON.parse(String(data)) as Received, undefined];
		} catch (error) {
			return this.#options.report(clientError("invalid-answer", "The node sent a message the client cannot read.", { cause: error }));
		}
		if ("event" in received) {
			if (received.event === "welcome") return this.#welcomed(received);
			return this.#handlers.event(received, bytes);
		}
		const refused = "error" in received ? new ClientError(received.error.code, received.error.message) : undefined;
		// A message whose id the node could not read is answered by none, and fails no call.
		if (received.id === null) return this.#options.report(refused);
		const settle = this.#pending.get(received.id);
		if (settle === undefined) return this.#options.report(clientError("invalid-answer", `An answer came for no call of this connection: ${received.id}.`));
		this.#pending.delete(received.id);
		settle(refused === undefined ? { result: "result" in received ? received.result : undefined } : { error: refused });
	}

	#welcomed(welcome: Extract<Event, { readonly event: "welcome" }>): void {
		this.#drops = 0;
		this.status.set({ status: "open", session: welcome.session, scopes: welcome.scopes, machine: welcome.machine, extensions: welcome.extensions });
		this.#handlers.opened();
	}

	/** The socket closed: each call waiting on it fails as lost, and the connection waits, longer after each drop, to connect again. */
	#dropped(socket: WebSocketLike): void {
		if (this.#socket !== socket) return;
		this.#socket = undefined;
		this.#failPending(clientError("connection-lost", "The connection to the node dropped before the answer came."));
		if (this.status.value.status === "stopped") return;
		const wait = Math.min(RECONNECT_MAX_MS, RECONNECT_MIN_MS * 2 ** this.#drops);
		this.#drops += 1;
		this.status.set({ status: "waiting", until: this.#options.clock.now() + wait });
		this.#cancelWait = this.#options.clock.setTimer(() => {
			this.#cancelWait = undefined;
			this.#connect();
		}, wait);
	}

	#stop(error: ClientError): void {
		if (this.status.value.status === "stopped") return;
		this.status.set({ status: "stopped", error });
		this.#cancelWait?.();
		this.#cancelWait = undefined;
		const socket = this.#socket;
		this.#socket = undefined;
		this.#failPending(error);
		socket?.close();
	}

	#failPending(error: ClientError): void {
		const pending = [...this.#pending.values()];
		this.#pending.clear();
		for (const settle of pending) settle({ error });
	}
}

/** A binary message: its event, the JSON whose length its first four bytes give, big-endian; and the bytes after it. */
function framed(data: ArrayBuffer): [Received, Uint8Array] {
	const length = new DataView(data).getUint32(0);
	// SAFETY: what the node sends is the protocol's, made from the node's own types, of the version the socket names.
	const event = JSON.parse(new TextDecoder().decode(new Uint8Array(data, 4, length))) as Received;
	return [event, new Uint8Array(data, 4 + length)];
}
