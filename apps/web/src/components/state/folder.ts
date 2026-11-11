/** The folder a thread works in, as the node has it work. */
import type { Listed } from "@octoii-works/octoii-client";
import { useMachine } from "@octoii-works/octoii-client/react";

/** The folder the thread of `row` works in: its own, else the machine's folder of the threads of no folder — no project's; none before either is known. */
export function useFolder(row: Listed | undefined): string | undefined {
	const machine = useMachine();
	return row?.config.workspace ?? machine?.workspace;
}
