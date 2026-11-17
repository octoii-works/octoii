import type { Client, Config, CreateWorktree, InputContent } from "@octoii-works/octoii-client";
import { contentOf } from "./attachments.ts";
import type { Draft } from "./drafts.ts";

export type WorktreeSubmission = { readonly key: string; readonly config: Config; readonly draft: Draft; readonly worktree: CreateWorktree };
export type AcceptWorktree = (submission: WorktreeSubmission) => Promise<string>;

/** The immutable submitted message and the acknowledgements saved while starting its thread. */
export type PendingThread = {
	readonly key: string;
	readonly savedAt: number;
	readonly config: Config;
	readonly draft: Draft;
	readonly worktree?: CreateWorktree;
	readonly thread?: string;
	readonly content?: readonly InputContent[];
	readonly delivered?: string;
};

/** One pending submission per node session and new-thread page. Writes finish before the next remote operation. */
export type PendingThreadStore = {
	read(): Promise<PendingThread | undefined>;
	exclusive(action: () => Promise<string>): Promise<string>;
	/** Atomically keeps this submission only if no other tab has already saved one. */
	begin(pending: PendingThread): Promise<PendingThread>;
	write(pending: PendingThread): Promise<void>;
	remove(key: string): Promise<void>;
};

/** Recovery replays the saved message, never a newly edited draft with an old delivery key. */
export class NewThread {
	readonly #client: Pick<Client, "call" | "keepFile">;
	readonly #store: PendingThreadStore;
	readonly #acceptWorktree: AcceptWorktree;
	#sending: Promise<string> | undefined;
	#completed: string | undefined;

	constructor(client: Pick<Client, "call" | "keepFile">, store: PendingThreadStore, acceptWorktree: AcceptWorktree) {
		this.#client = client;
		this.#store = store;
		this.#acceptWorktree = acceptWorktree;
	}

	read(): Promise<PendingThread | undefined> {
		return this.#store.read();
	}

	send(config: Config, draft: Draft): Promise<string> {
		return this.#run(() => this.#store.begin({ key: crypto.randomUUID(), savedAt: Date.now(), config, draft }));
	}

	sendWorktree(config: Config, draft: Draft, worktree: CreateWorktree): Promise<string> {
		return this.#run(() => this.#store.begin({ key: crypto.randomUUID(), savedAt: Date.now(), config, draft, worktree }));
	}

	resume(): Promise<string> {
		return this.#run(async () => {
			const pending = await this.#store.read();
			if (pending === undefined) throw new Error("This task has already been resolved. Reload the page.");
			return pending;
		});
	}

	/** Called after navigation to the acknowledged thread, so a failed cleanup remains recoverable. */
	async acknowledge(): Promise<void> {
		if (this.#completed !== undefined) await this.#store.remove(this.#completed);
	}

	/** Removes only the displayed recovery copy, never a thread or a running turn. */
	async forget(key: string): Promise<void> {
		await this.#store.exclusive(async () => {
			await this.#store.remove(key);
			return "";
		});
	}

	#run(read: () => Promise<PendingThread>): Promise<string> {
		if (this.#sending !== undefined) return this.#sending;
		this.#sending = this.#store
			.exclusive(async () => {
				const pending = await read();
				const thread = await this.#submit(pending);
				this.#completed = pending.key;
				return thread;
			})
			.finally(() => {
				this.#sending = undefined;
			});
		return this.#sending;
	}

	async #submit(saved: PendingThread): Promise<string> {
		if (saved.worktree !== undefined) {
			if (saved.thread !== undefined) return saved.thread;
			const thread = await this.#acceptWorktree({ ...saved, worktree: saved.worktree });
			await this.#store.write({ ...saved, thread });
			return thread;
		}
		let pending = saved;
		if (pending.delivered !== undefined && pending.thread !== undefined) return pending.thread;
		assertRecoverable(pending);
		const thread = pending.thread ?? (await this.#client.call("threads.create", { config: pending.config, key: `${pending.key}-create` })).thread;
		if (pending.thread === undefined) {
			pending = { ...pending, thread };
			await this.#store.write(pending);
		}
		let content = pending.content;
		if (content === undefined) {
			const attachments = [];
			for (const attachment of pending.draft.attachments) {
				if (attachment.bytes === undefined) {
					attachments.push(attachment);
					continue;
				}
				const made = await this.#client.keepFile(thread, attachment.bytes, attachment.name);
				attachments.push({ ref: made.ref, media_type: made.media_type, name: made.filename ?? attachment.name, size: attachment.size });
			}
			content = contentOf(pending.draft.text, attachments, pending.draft.feedback);
			pending = { ...pending, content };
			await this.#store.write(pending);
		}
		assertRecoverable(pending);
		const { delivery: delivered } = await this.#client.call("threads.deliver", { thread, content: [...content], key: `${pending.key}-deliver` });
		await this.#store.write({ ...pending, delivered });
		return thread;
	}
}

/** Core keeps idempotency records for 24 hours; reserve one hour and never replay older or future-dated submissions. */
function assertRecoverable(pending: PendingThread): void {
	const age = Date.now() - pending.savedAt;
	if (!Number.isFinite(age) || age < 0 || age >= 23 * 60 * 60 * 1000) {
		throw new Error("This submission is too old to retry safely. Check the existing threads before starting another task.");
	}
}
