/**
 * The terminals in the thread's details: a row for each terminal open in it, by the command line the model runs in it
 * or by its name, opening it in the panel.
 */
import type { ReactNode } from "react";
import type { TerminalListed } from "@octoii-works/octoii-client";
import { SquareTerminal } from "lucide-react";
import { terminalName } from "../../core/terminal.ts";
import { showSurface, terminalKey } from "../../components/state/panel.ts";
import { DetailRow } from "../../components/thread/details.tsx";

/** The terminals open in the thread, each by the command line the model runs in it or by its name, opened in the panel; nothing where none is. */
export function TerminalRows(props: { readonly thread: string; readonly terminals: readonly TerminalListed[] }): ReactNode {
	const { terminals } = props;
	if (terminals.length === 0) return null;
	return (
		<section className="p-1.5">
			{terminals.map((terminal) => (
				<DetailRow
					key={terminal.id}
					icon={<SquareTerminal />}
					label={terminal.line === undefined ? terminalName(terminal) : <code className="font-mono text-xs">{terminal.line}</code>}
					aside={`Terminal ${String(terminal.number)}`}
					title={terminal.line ?? terminalName(terminal)}
					onOpen={() => showSurface(props.thread, terminalKey(terminal.id))}
				/>
			))}
		</section>
	);
}
