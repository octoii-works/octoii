/**
 * A terminal of the thread's as a message names it, `:terminal{id=…}` by its number: its chip, its icon and its name;
 * its card, its name and its number, and its screen's last lines, read again while the card is drawn — shown in the
 * panel as either is clicked; one closed said to be so.
 */
import { type ReactNode, useContext } from "react";
import type { TerminalListed } from "@octoii-works/octoii-client";
import { SquareTerminal } from "lucide-react";
import { terminalName } from "../../core/terminal.ts";
import { useNow } from "../../components/effects/now.ts";
import { useOutcome } from "../../components/effects/read.ts";
import { showSurface, terminalKey } from "../../components/state/panel.ts";
import { cn } from "../../components/ui/cn.ts";
import { IconTile } from "../../components/ui/icon-tile.tsx";
import { MessageThreadContext } from "../../components/thread/message-thread.ts";
import { CARD, CardWords, CHIP, CHIP_ICON, GONE_CHIP, type ThingView } from "../../components/thread/thing-look.tsx";

/** The terminal numbered `number` in the thread shown, and the way to show it; none where it is closed. */
function useTerminal(number: string): { readonly listed: TerminalListed; readonly show: () => void } | undefined {
	const shown = useContext(MessageThreadContext);
	const listed = shown?.terminals.find((each) => String(each.number) === number);
	if (shown === undefined || listed === undefined) return undefined;
	return { listed, show: () => showSurface(shown.thread, terminalKey(listed.id)) };
}

/** A terminal named within a line: its icon and its name, shown in the panel as it is clicked. */
function TerminalChip(props: { readonly number: string }): ReactNode {
	const terminal = useTerminal(props.number);
	if (terminal === undefined) return <span className={GONE_CHIP}>{`terminal ${props.number}, closed`}</span>;
	return (
		<button type="button" data-link title={terminal.listed.line} onClick={terminal.show} className={cn(CHIP, "cursor-pointer")}>
			<SquareTerminal className={CHIP_ICON} />
			{terminalName(terminal.listed)}
		</button>
	);
}

/** How many of a terminal's last lines its card shows. */
const CARD_LINES = 6;
/** How often a terminal's card reads its screen again, in ms. */
const READ_AGAIN_MS = 2_000;

/** A terminal named on a line of its own: its icon, its name and its number, and its screen's last lines under them. */
function TerminalCard(props: { readonly number: string }): ReactNode {
	const terminal = useTerminal(props.number);
	if (terminal === undefined)
		return (
			<button type="button" disabled className={CARD}>
				<IconTile>
					<SquareTerminal />
				</IconTile>
				<CardWords name={`Terminal ${props.number}`} detail="Closed" />
			</button>
		);
	return (
		<button type="button" onClick={terminal.show} className={cn(CARD, "flex-col items-stretch gap-2")}>
			<span className="flex min-w-0 items-center gap-3">
				<IconTile>
					<SquareTerminal />
				</IconTile>
				<CardWords name={terminalName(terminal.listed)} detail={`Terminal ${String(terminal.listed.number)}`} />
			</span>
			<TerminalLines terminal={terminal.listed.id} />
		</button>
	);
}

/** The last lines the terminal `terminal` shows, read again while they are drawn; nothing till they are read, or where none is written. */
function TerminalLines(props: { readonly terminal: string }): ReactNode {
	const outcome = useOutcome("terminals.text", { terminal: props.terminal }, useNow(READ_AGAIN_MS));
	if (outcome === undefined || !("result" in outcome) || outcome.result.text === "") return null;
	const lines = outcome.result.text.split("\n").slice(-CARD_LINES).join("\n");
	return <pre className="well overflow-hidden px-2.5 py-1.5 font-mono text-xs leading-4 whitespace-pre">{lines}</pre>;
}

/** A terminal of the thread's, as a message names it by its number. */
export const TERMINAL_VIEW: ThingView = { Chip: (props) => <TerminalChip number={props.id} />, Card: (props) => <TerminalCard number={props.id} /> };
