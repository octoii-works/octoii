import type { Config, SendRequest } from "@octoii-works/octoii-client";
import type { Draft } from "../../core/drafts.ts";
import { acceptThread, threadRequest, type ThreadRequest, type ThreadWorkspace } from "./request.ts";

/** The frozen HTTP request and the local draft identities used for recovery. */
export type PendingThread = {
	readonly key: string;
	readonly project: string | undefined;
	readonly workspace: ThreadWorkspace;
	readonly draft: Draft;
	readonly request: ThreadRequest;
	readonly thread?: string;
};

type Receipt = { readonly key: string; readonly thread: string };

/** One pending submission per paired session and new-thread page. */
export type PendingThreadStore = {
	read(): Promise<PendingThread | undefined>;
	exclusive<T>(action: () => Promise<T>): Promise<T>;
	write(pending: PendingThread): Promise<void>;
	remove(key: string): Promise<void>;
};

export class NewThread {
	readonly #send: SendRequest;
	readonly #store: PendingThreadStore;
	#sending: Promise<Receipt> | undefined;

	constructor(send: SendRequest, store: PendingThreadStore) {
		this.#send = send;
		this.#store = store;
	}

	read(): Promise<PendingThread | undefined> {
		return this.#store.read();
	}

	send(config: Config, workspace: ThreadWorkspace, draft: Draft): Promise<Receipt> {
		this.#sending ??= this.#store
			.exclusive(async () => {
				if ((await this.#store.read()) !== undefined) throw new Error("A task is already saved for this draft.");
				const key = crypto.randomUUID();
				const request = await threadRequest(this.#send, { key, config, workspace, draft });
				const pending = {
					key,
					project: workspace.kind === "isolated" ? workspace.path : (config.workspace ?? undefined),
					workspace,
					draft: { ...draft, attachments: draft.attachments.map(({ bytes: _bytes, ...file }) => file) },
					request,
				};
				await this.#store.write(pending);
				return this.#submit(pending);
			})
			.finally(() => {
				this.#sending = undefined;
			});
		return this.#sending;
	}

	resume(key: string): Promise<Receipt> {
		return this.#store.exclusive(async () => {
			const pending = await this.#store.read();
			if (pending?.key !== key) throw new Error("This saved submission has changed. Open the latest saved task to continue.");
			return this.#submit(pending);
		});
	}

	/** Removes only the local copy with this key. */
	clear(key: string): Promise<void> {
		return this.#store.exclusive(() => this.#store.remove(key));
	}

	async #submit(pending: PendingThread): Promise<Receipt> {
		const { key } = pending;
		if (pending.thread !== undefined) return { key, thread: pending.thread };
		const thread = await acceptThread(this.#send, pending.request);
		await this.#store.write({ ...pending, thread });
		return { key, thread };
	}
}
