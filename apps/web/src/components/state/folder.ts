/** The folder a thread works in, as the node has it work. */
import type { Listed, Status } from "@octoii-works/octoii-client";
import { useMachine } from "@octoii-works/octoii-client/react";

/** The folder the thread of `row` works in: its own, else the machine's folder of the threads of no folder — no project's; none before either is known. */
export function useFolder(row: Listed | undefined, status: Status | undefined = row?.status): string | undefined {
	const machine = useMachine();
	const start = status?.start?.state;
	if (start !== undefined) return start.state === "ready" || start.state === "unavailable" ? start.directory.workspace : undefined;
	return row === undefined ? undefined : (row.config.workspace ?? machine?.workspace);
}
