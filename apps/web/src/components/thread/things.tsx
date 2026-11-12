/**
 * The things a message names by a directive, as the page draws them — a thread of the person's, a page of the thread's
 * browser, a terminal of the thread's — each in two forms: a chip within a line, its mark and its name, as a link to a
 * path looks; and a card on a line of its own, its mark on a tile, its name, and under it how it stands now — a thread's
 * standing and how long it worked, a page's address, a terminal's last lines, read again while the card is drawn. Each
 * opens as it is clicked: a thread gone into, a page or a terminal shown in the panel beside the chat. A thing that is
 * not there — a thread deleted, a page or a terminal closed — is drawn as such, quiet, and opens nothing.
 */
import { type ReactNode, useContext } from "react";
import type { Listed, PageListed, TerminalListed } from "@octoii-works/octoii-client";
import { useThreads } from "@octoii-works/octoii-client/react";
import { Globe, SquareTerminal } from "lucide-react";
import { standingOf, workedOf } from "../../core/sub-threads.ts";
import { terminalName } from "../../core/terminal.ts";
import { type Kind, pageName } from "../../core/things.ts";
import { statusOf, threadTitle } from "../../core/threads.ts";
import { useNow } from "../effects/now.ts";
import { useOutcome } from "../effects/read.ts";
import { pageKey, showSurface, terminalKey } from "../state/panel.ts";
import { cn } from "../ui/cn.ts";
import { IconTile } from "../ui/icon-tile.tsx";
import { standingDot } from "./entity.tsx";
import { MessageThreadContext } from "./message-thread.ts";
import { StatusMark } from "./status-mark.tsx";
import { WorkedFor } from "./sub-thread.tsx";

/** The chip of the thing of kind `kind` whose id is `id`. */
export function ThingChip(props: { readonly kind: Kind; readonly id: string }): ReactNode {
	switch (props.kind) {
		case "thread":
			return <ThreadChip thread={props.id} />;
		case "page":
			return <PageChip page={props.id} />;
		case "terminal":
			return <TerminalChip number={props.id} />;
	}
}

/** The card of the thing of kind `kind` whose id is `id`. */
export function ThingCard(props: { readonly kind: Kind; readonly id: string }): ReactNode {
	switch (props.kind) {
		case "thread":
			return <ThreadCard thread={props.id} />;
		case "page":
			return <PageCard page={props.id} />;
		case "terminal":
			return <TerminalCard number={props.id} />;
	}
}

/** A chip's look: its mark and its words, kept together on the line. */
const CHIP = "inline-flex items-baseline gap-1 whitespace-nowrap";
/** The look of a chip of a thing that is not there. */
const GONE_CHIP = cn(CHIP, "text-muted-foreground");
/** A chip's icon, set on the words' line. */
const CHIP_ICON = "size-[1em] shrink-0 self-center";

/** A card's look: a line of its own, as wide as it needs to be up to a measure, opening as it is clicked. */
const CARD =
	"card my-2 flex w-full max-w-md items-center gap-3 px-3 py-2.5 text-left outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-60";

/** A card's words: its name, and under it, quieter, how the thing stands. */
function CardWords(props: { readonly name: ReactNode; readonly detail?: ReactNode }): ReactNode {
	return (
		<span className="flex min-w-0 flex-1 flex-col">
			<span className="truncate text-ui font-medium">{props.name}</span>
			{props.detail === undefined ? null : <span className="truncate text-xs text-muted-foreground">{props.detail}</span>}
		</span>
	);
}

/** The thread `thread` as the person's threads list it; none where it is not listed. */
function useListed(thread: string): Listed | undefined {
	return useThreads()?.find((each) => each.id === thread);
}

/** A thread named within a line: a dot of how it stands and its title, going into it as it is clicked. */
function ThreadChip(props: { readonly thread: string }): ReactNode {
	const row = useListed(props.thread);
	if (row === undefined) return <span className={GONE_CHIP}>a thread no longer there</span>;
	return (
		<a href={`#/thread/${props.thread}`} className={CHIP}>
			<span className={cn("size-1.5 shrink-0 self-center rounded-full", standingDot(standingOf(row)))} />
			{threadTitle(row)}
		</a>
	);
}

/** The words of how a thread stands, on its card. */
const STATUS_WORDS: Readonly<Record<ReturnType<typeof statusOf>, string>> = {
	asks: "Waiting for you",
	working: "Working",
	failed: "Failed",
	ready: "Done",
};

/** A thread named on a line of its own: the mark of how it stands, its title, how it stands and how long it worked. */
function ThreadCard(props: { readonly thread: string }): ReactNode {
	const row = useListed(props.thread);
	if (row === undefined)
		return (
			<button type="button" disabled className={CARD}>
				<CardWords name="A thread no longer there" />
			</button>
		);
	const status = statusOf(row);
	const worked = workedOf([row]);
	return (
		<a href={`#/thread/${props.thread}`} className={CARD}>
			<IconTile>
				<StatusMark status={status} />
			</IconTile>
			<CardWords
				name={threadTitle(row)}
				detail={
					<>
						{STATUS_WORDS[status]}
						{worked === undefined ? null : (
							<>
								{" · "}
								<WorkedFor worked={worked} />
							</>
						)}
					</>
				}
			/>
		</a>
	);
}

/** The page `page` of the thread shown, as its browser lists it, and the way to show it; none where it is closed. */
function usePage(page: string): { readonly listed: PageListed; readonly show: () => void } | undefined {
	const shown = useContext(MessageThreadContext);
	const listed = shown?.pages.find((each) => each.id === page);
	if (shown === undefined || listed === undefined) return undefined;
	return { listed, show: () => showSurface(shown.thread, pageKey(page)) };
}

/** A page named within a line: a globe and its title, shown in the panel as it is clicked. */
function PageChip(props: { readonly page: string }): ReactNode {
	const page = usePage(props.page);
	if (page === undefined) return <span className={GONE_CHIP}>a page no longer open</span>;
	return (
		<button type="button" data-link title={page.listed.url} onClick={page.show} className={cn(CHIP, "cursor-pointer")}>
			<Globe className={CHIP_ICON} />
			{pageName(page.listed)}
		</button>
	);
}

/** A page named on a line of its own: a globe, its title and its address. */
function PageCard(props: { readonly page: string }): ReactNode {
	const page = usePage(props.page);
	return (
		<button type="button" disabled={page === undefined} onClick={page?.show} className={CARD}>
			<IconTile>
				<Globe />
			</IconTile>
			{page === undefined ? <CardWords name="A page no longer open" /> : <CardWords name={pageName(page.listed)} detail={page.listed.url} />}
		</button>
	);
}

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
