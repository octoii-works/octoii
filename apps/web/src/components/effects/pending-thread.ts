import { useEffect, useMemo, useState } from "react";
import type { Client } from "@octoii-works/octoii-client";
import { useConnection } from "@octoii-works/octoii-client/react";
import { NewThread, type PendingThread, type AcceptWorktree } from "../../core/new-thread.ts";
import { pendingThreadStore } from "../state/pending-thread.ts";

/** Reads the saved submission only after the node identifies the paired session; reconnects keep its identity. */
export function usePendingThread(client: Client, project: string | undefined, acceptWorktree: AcceptWorktree) {
	const connection = useConnection();
	const [session, setSession] = useState<string | undefined>(undefined);
	if (connection.status === "open" && connection.session !== session) setSession(connection.session);
	const starting = useMemo(
		() => (session === undefined ? undefined : new NewThread(client, pendingThreadStore(session, project), acceptWorktree)),
		[client, session, project, acceptWorktree],
	);
	const [read, setRead] = useState<{ readonly source: NewThread; readonly pending: PendingThread | undefined; readonly error?: string } | undefined>(undefined);
	const [attempt, setAttempt] = useState(0);
	useEffect(() => {
		let active = true;
		if (starting !== undefined)
			void starting
				.read()
				.then((pending) => {
					if (active) setRead({ source: starting, pending });
				})
				.catch((error: unknown) => {
					if (active) setRead({ source: starting, pending: undefined, error: error instanceof Error ? error.message : String(error) });
				});
		return () => {
			active = false;
		};
	}, [starting, attempt]);
	return {
		starting,
		loaded: starting !== undefined && read?.source === starting && read.error === undefined,
		error: read?.source === starting ? read?.error : undefined,
		retry: () => setAttempt((value) => value + 1),
		pending: read?.source === starting ? read?.pending : undefined,
		setPending: (pending: PendingThread | undefined) => {
			if (starting !== undefined) setRead({ source: starting, pending });
		},
	};
}
