import type { Client, Config } from "@octoii-works/octoii-client";
import { contentOf } from "./attachments.ts";
import type { Draft } from "./drafts.ts";

/** Keeps a created thread through a failed first submission while its new-thread page remains open. */
export class NewThread {
	readonly #client: Pick<Client, "create" | "keepFile" | "deliver">;
	readonly #created = new Map<string, string>();
	#sending: Promise<string> | undefined;

	constructor(client: Pick<Client, "create" | "keepFile" | "deliver">) {
		this.#client = client;
	}

	/** Concurrent sends share one submission; an explicit retry uses the current draft and the same thread. */
	send(config: Config, draft: Draft): Promise<string> {
		if (this.#sending !== undefined) return this.#sending;
		this.#sending = this.#submit(config, draft).finally(() => {
			this.#sending = undefined;
		});
		return this.#sending;
	}

	async #submit(config: Config, draft: Draft): Promise<string> {
		const key = JSON.stringify(config);
		let thread = this.#created.get(key);
		if (thread === undefined) {
			thread = await this.#client.create(config);
			this.#created.set(key, thread);
		}
		const attachments = [];
		for (const attachment of draft.attachments) {
			if (attachment.bytes === undefined) {
				attachments.push(attachment);
				continue;
			}
			const made = await this.#client.keepFile(thread, attachment.bytes, attachment.name);
			attachments.push({ ref: made.ref, media_type: made.media_type, name: made.filename ?? attachment.name, size: attachment.size });
		}
		await this.#client.deliver(thread, contentOf(draft.text, attachments, draft.feedback));
		this.#created.delete(key);
		return thread;
	}
}
