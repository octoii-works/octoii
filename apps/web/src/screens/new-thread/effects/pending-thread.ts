import { useEffect, useMemo, useState } from "react";
import { useConnection } from "@octoii-works/octoii-client/react";
import { useReach } from "../../../components/state/reach.ts";
import { NewThread, type PendingThread } from "../submission.ts";
import { pendingThreadStore } from "../pending-store.ts";

/** Reads the saved submission only after the node identifies the paired session; reconnects keep its identity. */
export function usePendingThread(project: string | undefined) {
	const connection = useConnection();
	const { send } = useReach();
	const [session, setSession] = useState<string | undefined>(undefined);
	if (connection.status === "open" && connection.session !== session) setSession(connection.session);
	const starting = useMemo(() => (session === undefined ? undefined : new NewThread(send, pendingThreadStore(session, project))), [send, session, project]);
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
		pending: read?.source === starting ? read?.pending : undefined,
		retry: () => {
			setRead(undefined);
			setAttempt((value) => value + 1);
		},
	};
}
