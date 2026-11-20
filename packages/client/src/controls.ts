/** Keeps a view's interactive HTTP controls in order and drops queued input from an ended view. */
import type { Connection, ConnectionStatus } from "./connection.ts";

type Control = {
	readonly when: ConnectionStatus;
	readonly replace: "move" | "resize" | undefined;
	readonly send: () => Promise<object>;
};

export class Controls {
	readonly #connection: Connection;
	readonly #told: (error: unknown) => void;
	readonly #waiting: Control[] = [];
	#draining = false;
	#stopped = false;

	constructor(connection: Connection, told: (error: unknown) => void) {
		this.#connection = connection;
		this.#told = told;
	}

	send(send: Control["send"], replace?: Control["replace"]): void {
		const when = this.#connection.status.value;
		if (this.#stopped || when.status !== "open") return;
		const last = this.#waiting.at(-1);
		if (replace !== undefined && last?.replace === replace && last.when === when) this.#waiting.pop();
		this.#waiting.push({ when, send, replace });
		if (!this.#draining) void this.#drain();
	}

	stop(): void {
		this.#stopped = true;
		this.#waiting.length = 0;
	}

	async #drain(): Promise<void> {
		this.#draining = true;
		try {
			for (let next = this.#waiting.shift(); next !== undefined; next = this.#waiting.shift()) {
				if (this.#connection.status.value === next.when) await next.send().catch(this.#told);
			}
		} finally {
			this.#draining = false;
		}
	}
}
