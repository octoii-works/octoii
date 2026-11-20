/** HTTP commands, queries, and file uploads for the client's session. */
import type { Connection } from "./connection.ts";
import { RECONNECT_MIN_MS } from "./constants.ts";
import { ClientError, clientError, isConnectionError } from "./errors.ts";
import type { ClientOptions, NodeAnswer, NodeRequest } from "./options.ts";
import { CALL_PATH, type FileRef, type MethodName, type ParamsOf, type ResultOf } from "./protocol.ts";

type Keyed = "threads.create" | "threads.deliver" | "accounts.add";

export class Requests {
	readonly #options: ClientOptions;
	readonly #connection: Connection;

	constructor(options: ClientOptions, connection: Connection) {
		this.#options = options;
		this.#connection = connection;
	}

	async call<Name extends MethodName>(method: Name, params: ParamsOf<Name>): Promise<ResultOf<Name>> {
		const answer = await this.#send({ method: "POST", path: CALL_PATH, contentType: "application/json", body: JSON.stringify({ method, params }) });
		const words = new TextDecoder().decode(answer.bytes);
		try {
			const reply: unknown = JSON.parse(words);
			if (typeof reply === "object" && reply !== null) {
				if ("error" in reply && typeof reply.error === "object" && reply.error !== null) {
					const error = reply.error;
					if ("code" in error && typeof error.code === "string" && "message" in error && typeof error.message === "string")
						throw new ClientError(error.code, error.message);
				}
				if (succeeded(answer) && "result" in reply) {
					// SAFETY: the node's method registry fixes the type of each successful result.
					return reply.result as ResultOf<Name>;
				}
			}
		} catch (error) {
			if (error instanceof ClientError) throw error;
		}
		throw clientError("invalid-answer", `The node did not return a command result (HTTP ${answer.status}).`);
	}

	/** Retries an operation that the node deduplicates by its key. */
	async keyed<Name extends Keyed>(method: Name, params: ParamsOf<Name>): Promise<ResultOf<Name>> {
		for (let attempt = 0; ; attempt++) {
			try {
				return await this.call(method, params);
			} catch (error) {
				if (!isConnectionError(error) || attempt === 3) throw error;
				await new Promise<void>((resolve) => this.#options.clock.setTimer(resolve, RECONNECT_MIN_MS * 2 ** attempt));
			}
		}
	}

	async keepFile(thread: string, bytes: Blob, name: string): Promise<FileRef> {
		const path = `/threads/${encodeURIComponent(thread)}/files?name=${encodeURIComponent(name)}`;
		const answer = await this.#send({ method: "POST", path, contentType: bytes.type || "application/octet-stream", body: bytes });
		const words = new TextDecoder().decode(answer.bytes);
		if (!succeeded(answer)) throw new Error(words.trim() || `The node did not keep the file (HTTP ${answer.status}).`);
		try {
			const file: unknown = JSON.parse(words);
			if (
				typeof file === "object" &&
				file !== null &&
				"ref" in file &&
				typeof file.ref === "string" &&
				"media_type" in file &&
				typeof file.media_type === "string"
			) {
				const filename = "filename" in file ? file.filename : undefined;
				if (filename === undefined || filename === null || typeof filename === "string")
					return { ref: file.ref, media_type: file.media_type, ...(filename === undefined ? {} : { filename }) };
			}
		} catch {
			// An unreadable successful response is a protocol error.
		}
		throw clientError("invalid-answer", "The node did not return a file reference.");
	}

	async #send(request: NodeRequest): Promise<NodeAnswer> {
		const state = this.#connection.status.value;
		if (state.status === "stopped") throw state.error;
		let answer: NodeAnswer;
		try {
			answer = await this.#options.send(request);
		} catch (error) {
			throw clientError("connection-lost", "No HTTP response came from the node.", { cause: error });
		}
		if (answer.status === 401) {
			this.#connection.revoke();
			throw clientError("unauthorized", "The node ended this session.");
		}
		return answer;
	}
}

function succeeded(answer: NodeAnswer): boolean {
	return answer.status >= 200 && answer.status < 300;
}
