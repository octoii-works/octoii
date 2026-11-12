/**
 * The thread whose messages a page shows, which what they name is read against: a path from its folder, a page of its
 * browser by its id — given by the page that shows the thread, none where a page shows messages of no thread; and the
 * places its paths are said by.
 */
import { createContext, useContext, useMemo } from "react";
import type { PageListed } from "@octoii-works/octoii-client";
import { useMachine } from "@octoii-works/octoii-client/react";
import type { Places } from "../../core/files.ts";

/** The thread of the messages shown: its id, the folder its paths start from — none where it has none — and its pages in the browser. */
export type MessageThread = { readonly thread: string; readonly workspace: string | undefined; readonly pages: readonly PageListed[] };

/** The thread of the messages shown; none where they are no thread's. */
export const MessageThreadContext = createContext<MessageThread | undefined>(undefined);

/** The places the paths of the thread shown are said by: its folder and scratchpad, the person's memory and home folder, as the machine tells them; none where messages of no thread are shown. */
export function usePlaces(): Places | undefined {
	const shown = useContext(MessageThreadContext);
	const machine = useMachine();
	const thread = shown?.thread;
	const workspace = shown?.workspace;
	return useMemo(
		() =>
			thread === undefined
				? undefined
				: {
						workspace,
						scratchpad: machine?.scratchpads == null ? undefined : `${machine.scratchpads}/${thread}`,
						memory: machine?.memory ?? undefined,
						home: machine?.home ?? undefined,
					},
		[thread, workspace, machine],
	);
}
