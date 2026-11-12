/**
 * What works in a thread's background now, as the last line of its timeline, the thread's present beside what its turns
 * did: quiet, in the words of a turn's last line — what runs, a command line the model runs in a terminal of its own or
 * a sub-thread given its task in the background, and how long; one thing opened or stopped from the line, several
 * listed in a card the line opens, each opened or stopped there.
 */
import type { ReactNode } from "react";
import type { Listed, TerminalListed } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { Bot, SquareTerminal } from "lucide-react";
import { workedOf } from "../../core/sub-threads.ts";
import type { Background } from "../../core/timeline.ts";
import { go } from "../state/navigation.ts";
import { failed } from "../state/notices.ts";
import { showSurface, terminalKey } from "../state/panel.ts";
import { cn } from "../ui/cn.ts";
import { Popover, PopoverPopup, PopoverTrigger } from "../ui/popover.tsx";
import { EntityMark } from "./entity.tsx";
import { WorkedFor } from "./sub-thread.tsx";

/** A quiet action of the line, in its words' size. */
const ACTION =
	"shrink-0 cursor-pointer rounded-sm text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none";

/** The line of what works in the background of `thread`. */
export function NowLine(props: { readonly thread: string; readonly background: Background }): ReactNode {
	const { terminals, threads } = props.background;
	const names = [...terminals.map((terminal) => `Terminal ${String(terminal.number)}`), ...threads.map((row) => row.config.title ?? "Sub-thread")];
	const [terminal] = terminals;
	const [row] = threads;
	if (names.length === 1 && terminal !== undefined) {
		return (
			<p className="meta-line">
				<SquareTerminal className="size-3.5 shrink-0" />
				<span className="min-w-0 truncate">
					Running · <code className="font-mono text-xs text-foreground">{terminal.line}</code>
					<span className="text-faint-foreground">{` in Terminal ${String(terminal.number)}`}</span>
				</span>
				<Actions>
					<TerminalActions thread={props.thread} terminal={terminal} />
				</Actions>
			</p>
		);
	}
	if (names.length === 1 && row !== undefined) {
		return (
			<p className="meta-line">
				<Bot className="size-3.5 shrink-0" />
				<span className="min-w-0 truncate">
					Running · <span className="text-foreground">{row.config.title ?? "Sub-thread"}</span>
				</span>
				<Actions>
					<SubThreadActions row={row} />
				</Actions>
			</p>
		);
	}
	return (
		<p className="meta-line">
			<SquareTerminal className="size-3.5 shrink-0" />
			<span className="min-w-0 truncate">
				{`${String(names.length)} running · `}
				<span className="text-faint-foreground">{names.join(", ")}</span>
			</span>
			<Popover>
				<PopoverTrigger className={cn(ACTION, "ms-1.5")}>Details</PopoverTrigger>
				<PopoverPopup side="top" align="start" className="w-[min(26rem,calc(100vw-2rem))] p-1.5">
					<div className="flex flex-col">
						{terminals.map((each) => (
							<DetailRow
								key={each.id}
								mark={
									<EntityMark standing="running">
										<SquareTerminal />
									</EntityMark>
								}
								name={`Terminal ${String(each.number)}`}
								detail={<code className="font-mono text-xs">{each.line}</code>}
							>
								<TerminalActions thread={props.thread} terminal={each} />
							</DetailRow>
						))}
						{threads.map((each) => (
							<DetailRow
								key={each.id}
								mark={
									<EntityMark standing="running">
										<Bot />
									</EntityMark>
								}
								name={each.config.title ?? "Sub-thread"}
								detail={undefined}
							>
								<SubThreadActions row={each} />
							</DetailRow>
						))}
					</div>
				</PopoverPopup>
			</Popover>
		</p>
	);
}

/** The actions of the line, set a little apart from its words. */
function Actions(props: { readonly children: ReactNode }): ReactNode {
	return <span className="ms-1.5 flex shrink-0 items-center gap-2.5">{props.children}</span>;
}

/** A thing of the background, as the card lists it: its mark, its name and what it runs, and its actions. */
function DetailRow(props: { readonly mark: ReactNode; readonly name: string; readonly detail: ReactNode; readonly children: ReactNode }): ReactNode {
	return (
		<div className="flex min-h-control items-center gap-2 rounded-md px-1.5 text-ui hover:bg-accent">
			{props.mark}
			<span className="shrink-0 text-foreground">{props.name}</span>
			<span className="min-w-0 flex-1 truncate text-muted-foreground">{props.detail}</span>
			<span className="flex shrink-0 items-center gap-2.5 text-xs">{props.children}</span>
		</div>
	);
}

/** A terminal's own actions: opened in the panel, or closed, ending what it runs. */
function TerminalActions(props: { readonly thread: string; readonly terminal: TerminalListed }): ReactNode {
	const client = useClient();
	return (
		<>
			<button type="button" className={ACTION} onClick={() => showSurface(props.thread, terminalKey(props.terminal.id))}>
				Open
			</button>
			<button
				type="button"
				className={ACTION}
				onClick={() => void client.closeTerminal(props.terminal.id).catch((error: unknown) => failed("The terminal could not be closed", error))}
			>
				Stop
			</button>
		</>
	);
}

/** A sub-thread's own actions, after how long it has worked: gone into, or stopped. */
function SubThreadActions(props: { readonly row: Listed }): ReactNode {
	const client = useClient();
	const worked = workedOf([props.row]);
	return (
		<>
			{worked === undefined ? null : (
				<span className="shrink-0 text-faint-foreground tabular-nums">
					<WorkedFor worked={worked} />
				</span>
			)}
			<button type="button" className={ACTION} onClick={() => go({ place: "thread", thread: props.row.id })}>
				Open
			</button>
			<button
				type="button"
				className={ACTION}
				onClick={() => void client.stop(props.row.id).catch((error: unknown) => failed("The sub-thread could not be stopped", error))}
			>
				Stop
			</button>
		</>
	);
}
