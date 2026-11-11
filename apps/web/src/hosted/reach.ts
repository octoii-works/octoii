/**
 * The page at the app's address reaches a computer through a tunnel: the computer it is paired with and the credential
 * of its session there kept in its storage, one tunnel session to that computer at a time, lasting as long as the
 * WebSocket on it.
 */
import { type WebSocketLike } from "@octoii-works/octoii-client";
import { matches } from "../core/schema.ts";
import { Type } from "typebox";
import { pairBrowser } from "../app/pairing.ts";
import { jsonOf, type Reach, type SendRequest } from "../components/state/reach.ts";
import { remotePairingOf } from "../core/pairing.ts";
import init, { Session, type Socket } from "../../vendor/tunnel/octoii_tunnel_wasm.js";
import tunnelModule from "../../vendor/tunnel/octoii_tunnel_wasm_bg.wasm";
import { createFileSources } from "./file-sources.ts";

const MACHINE_ITEM = "octoii.machine";

const MachineSchema = Type.Object({ relay: Type.String(), key: Type.String(), credential: Type.Optional(Type.String()) });
const PairedSchema = Type.Object({ credential: Type.String() });

/** The computer this browser is paired with: its relay, its key, and the credential of its session there once paired. */
type Machine = { readonly relay: string; readonly key: string; readonly credential?: string };

/** Why the computer cannot be reached, whichever it was: the relay does not tell a computer off from one that left. */
const AWAY = "Can't reach your computer. It may be off or asleep, or Octoii is not running on it. Trying again…";

/** The computer this browser is paired with; none where it has none. */
function pairedMachine(): Machine | undefined {
	const kept = localStorage.getItem(MACHINE_ITEM);
	if (kept === null) return undefined;
	try {
		const told: unknown = JSON.parse(kept);
		return matches(MachineSchema, told) ? told : undefined;
	} catch {
		return undefined;
	}
}

/** The tunnel's WebAssembly, instantiated once. */
let instantiated: Promise<void> | undefined;

function tunnelReady(): Promise<void> {
	instantiated ??= init({ module_or_path: new URL(tunnelModule, import.meta.url) }).then(() => undefined);
	return instantiated;
}

/** The value of the header `name` among `headers`, name and value in turn, as the tunnel gives them in lower case. */
function headerOf(headers: readonly string[], name: string): string | undefined {
	for (let at = 0; at + 1 < headers.length; at += 2) if (headers[at] === name) return headers[at + 1];
	return undefined;
}

/** `headers` as the tunnel takes them: name and value in turn. */
function flattened(headers: Readonly<Record<string, string>>): string[] {
	return Object.entries(headers).flat();
}

/** A message as a page's WebSocket of binary type `arraybuffer` gives it: text a string, bytes an `ArrayBuffer`. */
function received(data: TunnelMessage): string | ArrayBuffer {
	return data instanceof Uint8Array ? data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength) : data;
}

/** A message the tunnel's WebSocket gives: text a string, bytes a `Uint8Array`. */
type TunnelMessage = string | Uint8Array<ArrayBuffer>;

/** A listener of a socket. */
type SocketListener = (event: { readonly data: unknown }) => void;

/** The protocol's socket as the client takes it, until the tunnel's WebSocket is open behind it. */
class PageSocket implements WebSocketLike {
	binaryType = "arraybuffer";
	readonly #listeners = { open: new Set<SocketListener>(), close: new Set<SocketListener>(), message: new Set<SocketListener>() };
	#socket: Socket | undefined;
	#closed = false;

	addEventListener(type: "open" | "close", listener: () => void): void;
	addEventListener(type: "message", listener: SocketListener): void;
	addEventListener(type: "open" | "close" | "message", listener: SocketListener): void {
		this.#listeners[type].add(listener);
	}

	send(data: string): void {
		this.#socket?.send_text(data).catch(() => this.close());
	}

	close(): void {
		if (this.#closed) return;
		void this.#socket?.close();
		this.dropped();
	}

	/** The tunnel's WebSocket is open behind this one: its messages told till it closes. */
	joined(socket: Socket): void {
		if (this.#closed) return void socket.close();
		this.#socket = socket;
		this.#tell("open", undefined);
		void this.#read(socket);
	}

	async #read(socket: Socket): Promise<void> {
		try {
			for (let data: TunnelMessage | null = await socket.next(); data !== null && !this.#closed; data = await socket.next())
				this.#tell("message", received(data));
		} catch {
			// A socket that failed has closed, as one that ended has.
		}
		this.dropped();
	}

	/** This socket is over: closed, or no tunnel's WebSocket opened behind it. */
	dropped(): void {
		if (this.#closed) return;
		this.#closed = true;
		this.#tell("close", undefined);
	}

	#tell(type: "open" | "close" | "message", data: unknown): void {
		for (const listener of this.#listeners[type]) listener({ data });
	}
}

/** The reach of the page at the app's address, and whether this browser has a computer to reach. */
export function tunnelReach(about: string): { readonly reach: Reach; readonly paired: () => boolean } {
	let machine = pairedMachine();
	let session: Promise<Session> | undefined;
	let away: string | undefined;
	const letGo = (): void => {
		const going = session;
		session = undefined;
		void going?.then(
			(ending) => ending.close().finally(() => ending.free()),
			() => undefined,
		);
	};
	const tunnel = (): Promise<Session> => {
		const target = machine;
		if (target === undefined) return Promise.reject(new Error("This browser is paired with no computer."));
		if (session !== undefined) return session;
		const opening = tunnelReady().then(() => Session.open(target.relay, target.key));
		session = opening;
		void opening.catch(() => {
			if (session === opening) session = undefined;
		});
		return opening;
	};
	/** What every request carries: this browser, and its session's credential where it is paired. */
	const carried = (): Record<string, string> => {
		const credential = machine?.credential;
		return { "user-agent": navigator.userAgent, ...(credential === undefined ? {} : { authorization: `Bearer ${credential}` }) };
	};
	const send: SendRequest = async ({ method, path, contentType, body }) => {
		const reached = await tunnel();
		const given = body === undefined ? new Uint8Array() : typeof body === "string" ? new TextEncoder().encode(body) : new Uint8Array(await body.arrayBuffer());
		const headers = { ...carried(), ...(contentType === undefined ? {} : { "content-type": contentType }) };
		const answer = await reached.request(method, path, flattened(headers), given);
		try {
			return { status: answer.status, contentType: headerOf(answer.headers, "content-type"), bytes: answer.body };
		} finally {
			answer.free();
		}
	};
	const open = (protocol: string): WebSocketLike => {
		const page = new PageSocket();
		page.addEventListener("close", letGo);
		tunnel()
			.then((reached) => reached.socket("/ws", flattened({ ...carried(), "sec-websocket-protocol": protocol })))
			.then(
				(socket) => {
					away = undefined;
					page.joined(socket);
				},
				() => {
					away = AWAY;
					page.dropped();
				},
			);
		return page;
	};
	const pair = async (text: string): Promise<void> => {
		const given = remotePairingOf(text);
		if (given === undefined) throw new Error("That is no pairing link.");
		const before = machine;
		machine = given.machine;
		letGo();
		try {
			const paired = jsonOf(await pairBrowser(send, given.credential));
			if (!matches(PairedSchema, paired)) throw new Error("The computer paired this browser but gave it no credential.");
			machine = { ...given.machine, credential: paired.credential };
		} catch (error) {
			machine = before;
			letGo();
			throw error;
		}
		localStorage.setItem(MACHINE_ITEM, JSON.stringify(machine));
		// Kept through the browser's clearing of storage, where it agrees to, as a lost credential means pairing again.
		void navigator.storage.persist().catch(() => false);
	};
	const reach: Reach = {
		open,
		send,
		linkExample: "https://app.octoii.com/pair#key=…",
		readsLink: (text) => remotePairingOf(text) !== undefined,
		pair,
		fileSource: createFileSources((address) => send({ method: "GET", path: address })),
		whyAway: () => away,
		about,
	};
	return { reach, paired: () => machine?.credential !== undefined };
}
