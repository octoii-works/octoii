/** In-flight answers belong to a client and request, not to any one button or mounted component. */
import { ClientError, type Client, type JsonValue } from "@octoii-works/octoii-client";

export class Answers {
	readonly #client: Pick<Client, "answer">;
	readonly #pending = new Set<string>();
	readonly #listeners = new Set<() => void>();

	constructor(client: Pick<Client, "answer">) {
		this.#client = client;
	}

	readonly subscribe = (listener: () => void): (() => void) => {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	};

	pending(thread: string, key: string): boolean {
		return this.#pending.has(JSON.stringify([thread, key]));
	}

	/** False means another entry point already submitted this request; its answer is never replaced or retried. */
	async submit(thread: string, key: string, value: JsonValue): Promise<boolean> {
		const id = JSON.stringify([thread, key]);
		if (this.#pending.has(id)) return false;
		this.#pending.add(id);
		this.#changed();
		try {
			await this.#client.answer(thread, key, value);
			return true;
		} finally {
			this.#pending.delete(id);
			this.#changed();
		}
	}

	#changed(): void {
		for (const listener of [...this.#listeners]) listener();
	}
}

/** A lost acknowledgement leaves the outcome unknown; it does not mean the node refused the answer. */
export function answerError(error: unknown): string {
	if (error instanceof ClientError && error.code === "connection-lost") {
		return "The connection dropped before confirmation. Your answer may have been received. Wait for reconnection and check whether the request still needs an answer.";
	}
	return error instanceof Error ? error.message : String(error);
}
