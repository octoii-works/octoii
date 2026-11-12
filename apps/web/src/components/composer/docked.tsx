/**
 * What is docked over the composer, on one strip joined to its top edge: the work that goes on in the background — a
 * command line the model runs in a terminal of its own, a sub-thread given its task in the background — each by its
 * mark and a dot of how it stands, its name and what it runs, with a way into it and a way to stop it; then the
 * person's messages that wait in the thread's inbox, each taken into the turn that works as its next step begins, which
 * it steers, or beginning the next turn — each rising in as it is sent, with its text, its links by their words, and a
 * cross that takes it back while it waits. The timeline tells only when background work began and ended.
 */
import type { ReactNode } from "react";
import type { Inbound, InputContent, Listed, TerminalListed } from "@octoii-works/octoii-client";
import { useClient, useTerminals, useThreads } from "@octoii-works/octoii-client/react";
import { Bot, CornerDownRight, SquareTerminal, X } from "lucide-react";
import { linksAsWords } from "../../core/links.ts";
import { inBackground, workedOf } from "../../core/sub-threads.ts";
import { inputText, threadOfActor, type Work } from "../../core/timeline.ts";
import { go } from "../state/navigation.ts";
import { failed } from "../state/notices.ts";
import { showSurface, terminalKey } from "../state/panel.ts";
import { EntityMark } from "../thread/entity.tsx";
import { WorkedFor } from "../thread/sub-thread.tsx";

/** What is docked over the composer of `thread`: what works in the background by `work`'s calls, and the person's messages waiting in `inbox`; nothing where there is neither. */
export function Docked(props: { readonly thread: string; readonly work: readonly Work[]; readonly inbox: readonly (readonly [string, Inbound])[] }): ReactNode {
	const terminals = (useTerminals(props.thread) ?? []).filter((terminal) => terminal.line !== undefined);
	const background = inBackground(props.work, useThreads() ?? [], props.thread);
	const queued = props.inbox.flatMap(([id, inbound]) =>
		inbound.type === "input" && threadOfActor(inbound.actor) === undefined ? [{ id, content: inbound.content }] : [],
	);
	if (terminals.length === 0 && background.length === 0 && queued.length === 0) return null;
	return (
		<div className="mx-3 mb-[-1px] flex flex-col gap-0.5 rounded-t-lg border border-b-0 border-border bg-card px-2 py-1.5">
			{terminals.map((terminal) => (
				<TerminalRow key={terminal.id} thread={props.thread} terminal={terminal} />
			))}
			{background.map((row) => (
				<SubThreadRow key={row.id} row={row} />
			))}
			{queued.map((input) => (
				<QueuedRow key={input.id} thread={props.thread} id={input.id} content={input.content} />
			))}
		</div>
	);
}

/** A row of the strip's look, rising in as it comes. */
const ROW =
	"flex h-control-sm items-center gap-2 rounded-md px-1.5 text-ui transition-[opacity,translate] duration-200 hover:bg-accent starting:translate-y-1 starting:opacity-0 motion-reduce:transition-none";

/** A row's quiet action. */
const ACTION = "shrink-0 rounded-sm px-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground";

/** A command line the model runs in a terminal of its own: the terminal and its line, opened in the panel or closed, ending what it runs. */
function TerminalRow(props: { readonly thread: string; readonly terminal: TerminalListed }): ReactNode {
	const client = useClient();
	const { terminal } = props;
	return (
		<div className={ROW}>
			<EntityMark standing="running">
				<SquareTerminal />
			</EntityMark>
			<span className="shrink-0 text-foreground">{`Terminal ${String(terminal.number)}`}</span>
			<span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">{terminal.line}</span>
			<button type="button" className={ACTION} onClick={() => showSurface(props.thread, terminalKey(terminal.id))}>
				Open
			</button>
			<button
				type="button"
				className={ACTION}
				onClick={() => void client.closeTerminal(terminal.id).catch((error: unknown) => failed("The terminal could not be closed", error))}
			>
				Stop
			</button>
		</div>
	);
}

/** A sub-thread at work in the background: its title and how long it has worked, opened or stopped. */
function SubThreadRow(props: { readonly row: Listed }): ReactNode {
	const client = useClient();
	const { row } = props;
	const worked = workedOf([row]);
	return (
		<div className={ROW}>
			<EntityMark standing="running">
				<Bot />
			</EntityMark>
			<span className="min-w-0 flex-1 truncate text-foreground">{row.config.title ?? "Sub-thread"}</span>
			{worked === undefined ? null : (
				<span className="shrink-0 text-xs text-muted-foreground tabular-nums">
					<WorkedFor worked={worked} />
				</span>
			)}
			<button type="button" className={ACTION} onClick={() => go({ place: "thread", thread: row.id })}>
				Open
			</button>
			<button
				type="button"
				className={ACTION}
				onClick={() => void client.stop(row.id).catch((error: unknown) => failed("The sub-thread could not be stopped", error))}
			>
				Stop
			</button>
		</div>
	);
}

/** A message of the person's waiting in the inbox, with a cross that takes it back. */
function QueuedRow(props: { readonly thread: string; readonly id: string; readonly content: readonly InputContent[] }): ReactNode {
	const client = useClient();
	return (
		<div className={ROW}>
			<CornerDownRight className="size-3.5 shrink-0 text-muted-foreground" />
			<span className="min-w-0 flex-1 truncate text-foreground">{linksAsWords(inputText(props.content)) || "A message"}</span>
			<span className="text-xs text-muted-foreground">Joins at the next step</span>
			<button
				type="button"
				aria-label="Take back"
				onClick={() => void client.withdraw(props.thread, props.id).catch((error: unknown) => failed("The message could not be taken back", error))}
				className="flex size-5 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground"
			>
				<X className="size-3.5" />
			</button>
		</div>
	);
}
