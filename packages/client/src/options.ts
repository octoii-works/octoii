/** What a client takes from its app: how it reaches the node, as the session it holds, and what the platform gives. */
import type { FileRef } from "./protocol.ts";

/** What a socket gives the client: the platform's WebSocket, as a browser, Bun and Node have it. */
export type WebSocketLike = {
	/** How binary messages come: the client sets `arraybuffer`. */
	binaryType: string;
	send(data: string): void;
	close(): void;
	addEventListener(type: "open" | "close", listener: () => void): void;
	addEventListener(type: "message", listener: (event: { readonly data: unknown }) => void): void;
};

/** The clock the client waits by: the time, and a timer it can cancel. */
export type Clock = { now(): number; setTimer(callback: () => void, ms: number): () => void };

/** What `createClient` takes. */
export type ClientOptions = {
	/**
	 * Opens a socket to the node speaking `protocol`, as `new WebSocket(url, protocol)` does, authenticated as the
	 * session the app holds: a browser's page by its session cookie, which the browser sends itself; a program by its
	 * bearer token, in the socket's headers.
	 */
	readonly open: (protocol: string) => WebSocketLike;
	/** Mints the key of an operation done once, as `crypto.randomUUID` does. */
	readonly randomUUID: () => string;
	readonly clock: Clock;
	/** What fails no caller: what the node sends that the client cannot read, or a thread it could not follow. */
	readonly report: (error: unknown) => void;
	/** How many of a thread's latest entries the client keeps, more only while a reader reads further back; `ENTRIES_KEPT` where none is named. */
	readonly entriesKept?: number;
	/**
	 * Sends a file's bytes for the thread `thread` to keep, named `name`, as a POST to the node's
	 * `/threads/{thread}/files` does, authenticated as `open` is: the file as the thread keeps it, an image fitted to
	 * what models read; none where the app sends no files.
	 */
	readonly keepFile?: (thread: string, bytes: Blob, name: string) => Promise<FileRef>;
};
