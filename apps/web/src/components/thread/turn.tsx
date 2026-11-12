/**
 * A turn of the thread, as the rows of the timeline draw it: the words that began it, or that steered it on — the
 * person's, a task its parent gave it, or what came of one it gave a sub-thread in the background; each part of its
 * work, laid out alike while it works and once it ended — what it said on its way, each stretch of calls a line that
 * opens to its steps, what it thought — so nothing of it moves as it ends; then its reply, a block at a time; and its
 * end: the files it changed, what waits on the person, why it failed where it did, and, as its last line, what it does
 * now and how long it has worked while it works, or its foot once it ended. A turn that works only grows as it goes:
 * its parts are added at the end, and what it does now changes its words in one line of its own, so nothing of it
 * moves back as it calls its tools.
 */
import type { ReactNode } from "react";
import { useThreads } from "@octoii-works/octoii-client/react";
import { CircleAlert, CornerDownRight, Globe, SquareTerminal } from "lucide-react";
import type { Doing, Foot, TurnEnd } from "../../core/rows.ts";
import { pastLabel, runningTime } from "../../core/time.ts";
import { endedOf } from "../../core/terminal.ts";
import { type Laid, type Said, SIPHON, siphonWords, TERMINAL, type Work } from "../../core/timeline.ts";
import { Counting, useNow } from "../effects/now.ts";
import { cn } from "../ui/cn.ts";
import { Collapsible, CollapsiblePanel, CollapsibleTrigger } from "../ui/collapsible.tsx";
import { CopyButton } from "../ui/copy-button.tsx";
import { AskLine } from "./ask-line.tsx";
import { Calls, Thought } from "./calls.tsx";
import { TurnChanges } from "./changes.tsx";
import { SystemDivider } from "./divider.tsx";
import { EntityContent, EntityMark } from "./entity.tsx";
import { Prose } from "./markdown.tsx";
import { Mark } from "./marks.tsx";
import { Delegations, SubThreadTold } from "./sub-thread.tsx";
import { ThreadMessage } from "./thread-message.tsx";
import { UserMessage } from "./user-message.tsx";

/** The words of a reply, as the page sets them: larger and airier than the work's lines. */
export const WORDS = "text-body";

/**
 * Words a turn took: a command line's end in a terminal, as the terminal's line; Siphon's own notice, as a divider in
 * words of its own, never the model's text; what came of a task given to a sub-thread of `thread`, as its line; a
 * message another thread gave it, folded on the input side; the person's, or a task from `parent`, as theirs.
 */
export function Words(props: { readonly said: Said; readonly thread: string; readonly parent: string | undefined }): ReactNode {
	const { said } = props;
	if (said.from === TERMINAL) return <TerminalEnded said={said} />;
	if (said.from === SIPHON) {
		const words = siphonWords(said.text);
		return <SystemDivider event={words.event} detail={words.detail} icon={<CornerDownRight />} />;
	}
	if (said.from !== undefined && said.from !== props.parent) return <FromThread said={said} thread={props.thread} />;
	return <UserMessage said={said} />;
}

/** Words from another thread: what a sub-thread of `thread` told of a task, as its line; any other thread's, as its message. */
function FromThread(props: { readonly said: Said; readonly thread: string }): ReactNode {
	const row = useThreads()?.find((each) => each.id === props.said.from);
	return row?.parent?.thread === props.thread ? <SubThreadTold said={props.said} /> : <ThreadMessage said={props.said} />;
}

/** A command line the model ran in a terminal, ended of itself, as the terminal's line: how it went by its dot, the line under its name, when; opened, its last screen. */
function TerminalEnded(props: { readonly said: Said }): ReactNode {
	const ended = endedOf(props.said.text);
	if (ended === undefined) return <SystemDivider event="A terminal ended" />;
	const standing = ended.ok ? "completed" : "failed";
	return (
		<Collapsible>
			<CollapsibleTrigger className="group/fold work-line work-opens" aria-description={ended.how}>
				<EntityContent
					mark={
						<EntityMark standing={standing}>
							<SquareTerminal />
						</EntityMark>
					}
					standing={standing}
					name={`Terminal ${String(ended.number)}`}
					detail={<span className="font-mono">{ended.line}</span>}
					time={clockTime(props.said.at)}
					opens="in-place"
				/>
			</CollapsibleTrigger>
			<CollapsiblePanel>
				<div className="work-indent pt-0.5 pb-2">
					<pre className="well overflow-x-auto px-3 py-2 font-mono text-xs whitespace-pre">{ended.screen === "" ? `[${ended.how}]` : ended.screen}</pre>
				</div>
			</CollapsiblePanel>
		</Collapsible>
	);
}

/** The time of day `at` was, as a line tells it. */
function clockTime(at: number): string {
	return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** A part of a turn's work, as it is laid out: a stretch of calls, delegations as sub-threads, or a step on its own. */
export function WorkPart(props: { readonly thread: string; readonly laid: Laid }): ReactNode {
	const { laid } = props;
	switch (laid.kind) {
		case "calls":
			return <Calls thread={props.thread} steps={laid.steps} calls={laid.calls} />;
		case "delegations":
			return <Delegations thread={props.thread} calls={laid.calls} />;
		case "step":
			return <WorkStep thread={props.thread} step={laid.step} />;
	}
}

/** One step of the work on its own: a call that waits on the person, what the model thought, a search of the web, its words on the way, a request that came to nothing, or the thread summed up. */
function WorkStep(props: { readonly thread: string; readonly step: Work }): ReactNode {
	const { step } = props;
	switch (step.kind) {
		case "call":
			return <Calls thread={props.thread} steps={[step]} calls={[step]} />;
		case "thought":
			return <Thought text={step.text} took={step.took} />;
		case "web-search":
			return (
				<p className="work-line">
					<Mark>
						<Globe />
					</Mark>
					<span className="truncate">{step.query === undefined ? "Searched the web" : `Searched the web for ${step.query}`}</span>
				</p>
			);
		case "words":
			// Words on the way stand among the lines, a paragraph's breath above and below them.
			return <Prose text={step.text} className={cn(WORDS, "py-1.5 text-foreground")} />;
		case "failure":
			return <FailureLine text={step.text} />;
		case "compaction":
			return <SummedUp />;
	}
}

/** The end of a turn: the files it changed, what waits on the person, why it failed, and its last line — what it does now, or its foot. */
export function TurnEndPart(props: { readonly end: TurnEnd }): ReactNode {
	const { end } = props;
	return (
		<div className="flex flex-col gap-2">
			<TurnChanges changes={end.changes} />
			{end.asks.map((ask) => (
				<AskLine key={ask.key} ask={ask} />
			))}
			{end.failure === undefined ? null : <FailureLine text={end.failure} />}
			{/* What waits on the person says so where it stands, a call or a request of its own, which it answers. */}
			{end.doing === undefined || end.doing.waits ? null : <DoingLine doing={end.doing} />}
			{end.foot === undefined ? null : <TurnFoot foot={end.foot} />}
		</div>
	);
}

/**
 * The foot of a turn that ended, its last line: a button that copies its reply as it was written, where it wrote one;
 * how long it worked, or that the person stopped it; and when it ended.
 */
function TurnFoot(props: { readonly foot: Foot }): ReactNode {
	const { foot } = props;
	return (
		<div className={cn("flex min-h-control-sm items-center gap-1 text-xs text-faint-foreground tabular-nums", foot.reply !== "" && "-ms-1.5")}>
			{foot.reply === "" ? null : <CopyButton text={foot.reply} label="Copy reply" />}
			{foot.worked === undefined ? null : <span>{foot.worked}</span>}
			{foot.worked === undefined || foot.endedAt === undefined ? null : <span aria-hidden>·</span>}
			{foot.endedAt === undefined ? null : <Ended at={foot.endedAt} />}
		</div>
	);
}

/** When a turn ended, as the day it is read on says it — its hour that day, its date on another — the whole of it its tip. */
function Ended(props: { readonly at: number }): ReactNode {
	const now = useNow(60_000);
	const at = new Date(props.at);
	return (
		<time dateTime={at.toISOString()} title={at.toLocaleString()}>
			{pastLabel(props.at, now)}
		</time>
	);
}

/** That the thread's window was summed up there, as a rule across the column: within a turn's work, or between turns. */
export function SummedUp(): ReactNode {
	return <SystemDivider event="Context compacted" detail="what came before was summed up" />;
}

/** Why something came to nothing, as a line of the work, red. */
function FailureLine(props: { readonly text: string }): ReactNode {
	return (
		<p className="work-line text-error-foreground">
			<Mark troubled>
				<CircleAlert />
			</Mark>
			<span className="min-w-0">{props.text}</span>
		</p>
	);
}

/**
 * What a turn that works does now — waits on the person; thinks, while the request it waits on has written nothing;
 * forms a call, by its tool's verb; or works — and how long it has worked, counted on each second: one line in one
 * place, its words changing as the turn goes, so the turn neither grows nor shrinks as its model thinks and calls.
 */
function DoingLine(props: { readonly doing: Doing }): ReactNode {
	const { doing } = props;
	return (
		<p className="meta-line">
			<span className={doing.waits ? "text-warning-foreground" : "shimmer"}>{doing.words}</span>
			{doing.since === undefined ? null : (
				<span className="text-faint-foreground">
					· <Counting since={doing.since} written={runningTime} />
				</span>
			)}
		</p>
	);
}
