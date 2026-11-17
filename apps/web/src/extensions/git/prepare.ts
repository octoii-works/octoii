/** Called by the explicit first send, never by mounting a Thread or reconnecting. */
import type { Client } from "@octoii-works/octoii-client";
import { preparingWorktree } from "../../components/state/worktree-preparation.ts";
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
