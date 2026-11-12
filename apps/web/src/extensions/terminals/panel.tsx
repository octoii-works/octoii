/**
 * The terminals in the panel: a tab for each terminal of the thread's, named by the title its program set, its screen
 * below, closing the terminal as it is closed; and `+`'s way to open one.
 */
import type { TerminalListed } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { SquareTerminal } from "lucide-react";
import { terminalName } from "../../core/terminal.ts";
import type { Opening, Surface } from "../../components/panel/right-panel.tsx";
import { failed } from "../../components/state/notices.ts";
import { showSurface, terminalKey } from "../../components/state/panel.ts";
import { TerminalPane } from "./terminal-pane.tsx";

/** The size a terminal opens at, before the view that opens it sizes it to its room. */
const OPENED_SIZE = { rows: 24, cols: 80 } as const;

/** The panel's part of the terminals `terminals` of the thread `thread`; nothing where the terminals are switched off (`on`). */
export function useTerminalsPanel(
	thread: string,
	terminals: readonly TerminalListed[],
	on: boolean,
): { readonly surfaces: readonly Surface[]; readonly openings: readonly Opening[] } {
	const client = useClient();
	if (!on) return { surfaces: [], openings: [] };
	const surfaces = terminals.map((terminal): Surface => ({
		key: terminalKey(terminal.id),
		icon: <SquareTerminal />,
		label: terminalName(terminal),
		body: <TerminalPane terminal={terminal.id} />,
		close: () => {
			client.closeTerminal(terminal.id).catch((error: unknown) => failed("The terminal could not be closed", error));
		},
	}));
	const opening: Opening = {
		label: "Terminal",
		icon: SquareTerminal,
		letter: "T",
		open: () => {
			client.openTerminal(thread, OPENED_SIZE).then(
				(id) => showSurface(thread, terminalKey(id)),
				(error: unknown) => failed("A terminal could not be opened", error),
			);
		},
	};
	return { surfaces, openings: [opening] };
}
