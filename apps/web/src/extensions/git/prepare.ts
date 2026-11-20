/** Planning and preparation on the explicit first send. */
import type { Client, CreateWorktree } from "@octoii-works/octoii-client";
import { preparingWorktree } from "./worktree-preparation.ts";

export type WorktreeBase = { readonly commit: string; readonly label: string };
export type DraftWorktree = { readonly path: string; readonly base?: WorktreeBase | undefined };

/** The default resolves the current commit on send; an explicit branch choice already holds its commit. */
export async function planWorktree(client: Pick<Client, "call">, draft: DraftWorktree): Promise<CreateWorktree> {
	let base = draft.base?.commit;
	if (base === undefined) {
		const { repository } = await client.call("git.repository", { path: draft.path });
		if (repository?.head === undefined) throw new Error("A separate directory requires a Git repository with a commit.");
		base = repository.head;
	}
	return client.call("git.worktree-plan", { path: draft.path, base, key: crypto.randomUUID() });
}

export async function prepareWorktree(client: Client, thread: string, open: () => void): Promise<void> {
	const finished = preparingWorktree(client, thread);
	try {
		open();
		const inspected = await client.call("git.start", { action: "inspect", thread });
		if (inspected.start.state.state === "pending") await client.call("git.start", { action: "prepare", thread, token: inspected.start.token });
	} finally {
		finished();
	}
}
