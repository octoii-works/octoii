/**
 * A stretch of calls as one line of the work — one call's own, or several gathered — led by one mark, in the slot every
 * line of the work gives one, so its words stand where those of every other line do. One call's line says what it does
 * and on what — its subject as code where it is code — and beside it what came of it: the lines a read held, what an
 * edit changed, what the calls made under it did; a call that went wrong is marked red, its why told only opened; at its
 * end how it stands where the person is to act, and how long it took or has run; opened, what the model thought before
 * it, the calls made under it, then what it was given and what it told. Several say what they did as a person says it,
 * how many failed, and how long they took, led by the mark of the call going on, else of the kind of work they did
 * most; opened, the latest of them each on a line of its own, at the same edges, closer together, those before behind a
 * line that shows them. What a command prints shows only opened, so the work grows by whole lines alone.
 */
import { memo, type ReactNode, useState } from "react";
import { Ban, Brain } from "lucide-react";
import { callDuration, duration } from "../../core/time.ts";
import { type CallLine, declinedText, leadOf, runsOf, thoughtCalls, type ThoughtCall, type Work } from "../../core/timeline.ts";
import { folderOf, nameOf, type Places, spokenPath, spokenText } from "../../core/files.ts";
import { summaryOf } from "../../core/tools.ts";
import { useArrived } from "../effects/arrived.ts";
import { Counting } from "../effects/now.ts";
import { cn } from "../ui/cn.ts";
import { Collapsible, CollapsiblePanel, CollapsibleTrigger, FoldChevron } from "../ui/collapsible.tsx";
import { Waiting } from "./ask-line.tsx";
import { CallDetail } from "./call-detail.tsx";
import { ChangeCount } from "./changes.tsx";
import { Prose } from "./markdown.tsx";
import { usePlaces } from "./message-thread.ts";
import { type Approval, ApprovalLine, PermissionDetail, useApproval } from "../asks/approval.tsx";
import { CallMark, Mark } from "./marks.tsx";

/** What a stretch's line is given: the thread whose work it is, its steps — its calls and what was thought between them — and its calls. */
type StretchProps = { readonly thread: string; readonly steps: readonly Work[]; readonly calls: readonly CallLine[] };

/** Whether two stretches hold the same steps and calls, each the same object, at the same place: a line kept the same is not drawn again. */
function sameStretch(a: StretchProps, b: StretchProps): boolean {
	return a.thread === b.thread && sameItems(a.steps, b.steps) && sameItems(a.calls, b.calls);
}

/** Whether two lists hold the same items, in order. */
function sameItems<Item>(a: readonly Item[], b: readonly Item[]): boolean {
	return a.length === b.length && a.every((item, index) => item === b[index]);
}

/** The line of a stretch of calls: one call's own, or several gathered. */
export const Calls = memo(function Calls(props: StretchProps): ReactNode {
	const [only] = props.calls;
	const one = props.calls.length === 1 ? only : undefined;
	return <StretchLine thread={props.thread} steps={props.steps} calls={props.calls} one={one} />;
}, sameStretch);

/** The line of a stretch: its mark, its words, how it ends and its time; opened, what it holds. A call whose permission waits stands open, answered under it. */
function StretchLine(props: StretchProps & { readonly one: CallLine | undefined }): ReactNode {
	const { one } = props;
	const approval = useApproval(props.thread, one?.state === "waiting" ? one.ask : undefined);
	if (one !== undefined && approval !== undefined) return <WaitingCall call={one} approval={approval} />;
	const lead = one ?? leadOf(props.calls);
	return (
		<Collapsible>
			<CollapsibleTrigger data-state={one?.state ?? stateOf(props.calls)} className="group/call work-line work-opens">
				{lead === undefined ? null : <CallMark call={lead} />}
				{one === undefined ? <GatheredWords calls={props.calls} /> : <CallWords call={one} />}
				<FoldChevron className="quiet-chevron" />
				<span className="flex-1" />
				{one === undefined ? <Failed calls={props.calls} /> : <CallEnd call={one} />}
				<span className="shrink-0 text-xs text-muted-foreground tabular-nums">
					<StretchTime calls={props.calls} />
				</span>
			</CollapsibleTrigger>
			<CollapsiblePanel>
				{one === undefined ? (
					<Steps thread={props.thread} steps={props.steps} />
				) : (
					<div className="work-indent">
						{props.steps.map((step) => (step.kind === "thought" ? <ThoughtNote key={step.key} text={step.text} took={step.took} /> : null))}
						{one.calls.length === 0 ? null : (
							<div className="work-list work-tight">
								{one.calls.map((call) => (
									<Calls key={call.id} thread={props.thread} steps={[call]} calls={[call]} />
								))}
							</div>
						)}
						<CallDetail call={one} />
					</div>
				)}
			</CollapsiblePanel>
		</Collapsible>
	);
}

/** A call whose permission waits on the person: its line, what it would do in full under it, and the answers. */
function WaitingCall(props: { readonly call: CallLine; readonly approval: Approval }): ReactNode {
	const { call } = props;
	return (
		<div data-state="waiting" className="group/call">
			<div className="work-line">
				<CallMark call={call} />
				<CallWords call={call} />
			</div>
			<div className="work-indent flex flex-col gap-1.5 pb-2">
				<PermissionDetail lines={props.approval.lines} />
				<ApprovalLine approval={props.approval} />
			</div>
		</div>
	);
}

/** How many of a stretch's calls show as it is opened, the latest; those before wait behind a line that shows them. */
const SHOWN = 8;

/**
 * The calls a stretch gathered, opened, each on a line of its own at the stretch's own edges, what was thought before
 * it held in it: the latest few, those before behind a line that shows them; one that joins as it is open fading in.
 */
function Steps(props: { readonly thread: string; readonly steps: readonly Work[] }): ReactNode {
	const [all, setAll] = useState(false);
	const shown = runsOf(thoughtCalls(props.steps));
	const hidden = all ? 0 : Math.max(0, shown.length - SHOWN);
	const arrived = useArrived(shown.map(runKey));
	return (
		<div className="work-list work-tight">
			{hidden === 0 ? null : (
				<button type="button" onClick={() => setAll(true)} className="work-line work-opens text-xs">
					<span className="size-control-2xs shrink-0" />
					{`Show ${String(hidden)} earlier`}
				</button>
			)}
			{shown.slice(hidden).map((run) => (
				<div key={runKey(run)} className={cn(arrived(runKey(run)) && "animate-fade-in motion-reduce:animate-none")}>
					{run.length === 1 && run[0] !== undefined ? <ThoughtfulCall thread={props.thread} shown={run[0]} /> : <RunLine thread={props.thread} run={run} />}
				</div>
			))}
		</div>
	);
}

/** A run's key: its first call's id, so a run that grows is the same line. */
function runKey(run: readonly ThoughtCall[]): string {
	return run[0]?.call.id ?? "";
}

/**
 * Calls one after another that changed one file, as one line: the latest's verb and the file, how many times, all
 * they added and took out, and how long they took; opened, each of them on a line of its own.
 */
function RunLine(props: { readonly thread: string; readonly run: readonly ThoughtCall[] }): ReactNode {
	const calls = props.run.map(({ call }) => call);
	const latest = calls.at(-1);
	if (latest === undefined) return null;
	const changed = calls.reduce(
		(sum, call) => (call.aside?.kind === "change" ? { added: sum.added + call.aside.added, removed: sum.removed + call.aside.removed } : sum),
		{ added: 0, removed: 0 },
	);
	return (
		<Collapsible>
			<CollapsibleTrigger data-state={latest.state} className="group/call work-line work-opens">
				<CallMark call={latest} />
				<CallWords call={{ ...latest, aside: undefined }} />
				<span className="shrink-0 text-xs text-muted-foreground">{`×${String(calls.length)}`}</span>
				<ChangeCount added={changed.added} removed={changed.removed} />
				<FoldChevron className="quiet-chevron" />
				<span className="flex-1" />
				<CallEnd call={latest} />
				<span className="shrink-0 text-xs text-muted-foreground tabular-nums">
					<StretchTime calls={calls} />
				</span>
			</CollapsibleTrigger>
			<CollapsiblePanel>
				<div className="work-list work-tight">
					{props.run.map((each) => (
						<ThoughtfulCall key={each.call.id} thread={props.thread} shown={each} />
					))}
				</div>
			</CollapsiblePanel>
		</Collapsible>
	);
}

/** A call among those a stretch gathered, as its own line, what was thought before it opening with it. */
function ThoughtfulCall(props: { readonly thread: string; readonly shown: ThoughtCall }): ReactNode {
	const { call, thoughts } = props.shown;
	return <Calls thread={props.thread} steps={[...thoughts, call]} calls={[call]} />;
}

/** What the model thought before a call, as the call opens: how long it thought, then its words, quiet. */
function ThoughtNote(props: { readonly text: string; readonly took: number | undefined }): ReactNode {
	return (
		<div className="flex flex-col gap-0.5 pt-0.5 pb-1.5 text-xs text-muted-foreground">
			<span className="flex items-center gap-1 text-faint-foreground">
				<Brain className="size-3" />
				{thoughtWords(props.took)}
			</span>
			<Prose text={props.text} className="text-xs" />
		</div>
	);
}

/** How long the model thought, in words. */
function thoughtWords(took: number | undefined): string {
	return took === undefined || took <= 0 ? "Thought" : `Thought for ${duration(took)}`;
}

/** How a stretch of several stands, as its line marks it: running while one runs or waits on the person, else done. */
function stateOf(calls: readonly CallLine[]): string {
	return calls.some((call) => call.state === "running" || call.state === "waiting") ? "running" : "done";
}

/** What a call does, as its line says it: its verb — shimmering while it runs — and what it works on, its tool's name where it has no verb; then what it says beside them. */
function CallWords(props: { readonly call: CallLine }): ReactNode {
	const { call } = props;
	const places = usePlaces();
	return (
		<>
			{call.verb === undefined ? (
				<span className="shrink-0 font-mono text-xs text-foreground group-data-[state=running]/call:shimmer">{call.tool}</span>
			) : (
				<span className="shrink-0 group-data-[state=running]/call:shimmer">{call.verb}</span>
			)}
			{call.subject === undefined ? null : <Subject call={call} subject={call.subject} places={places} />}
			{call.aside === undefined ? null : call.aside.kind === "change" ? (
				<ChangeCount added={call.aside.added} removed={call.aside.removed} />
			) : (
				<span className="min-w-0 shrink-0 truncate text-muted-foreground">{call.aside.text}</span>
			)}
		</>
	);
}

/**
 * What a call works on, by the places its paths lie in: a file by its name, the folder it is in quieter before it and
 * the first to give way to the line's room; code as code; the whole as written on hover.
 */
function Subject(props: { readonly call: CallLine; readonly subject: string; readonly places: Places | undefined }): ReactNode {
	const { call, subject, places } = props;
	if (call.code) {
		return (
			<span title={subject} className="min-w-0 truncate font-mono text-xs text-foreground">
				{places === undefined ? subject : spokenText(subject, places)}
			</span>
		);
	}
	if (call.work !== "read" && call.work !== "edit") return <span className="min-w-0 truncate text-foreground">{subject}</span>;
	const spoken = places === undefined ? subject : spokenPath(subject, places);
	const folder = folderOf(spoken);
	return (
		<span title={subject} className="flex min-w-0">
			{folder === "" ? null : <span className="min-w-0 truncate text-faint-foreground">{`${folder}/`}</span>}
			<span className="shrink-0 text-foreground">{nameOf(spoken)}</span>
		</span>
	);
}

/** What several calls did, as a person says it, its first letter a capital — shimmering while one runs. */
function GatheredWords(props: { readonly calls: readonly CallLine[] }): ReactNode {
	return <span className="min-w-0 truncate first-letter:uppercase group-data-[state=running]/call:shimmer">{summaryOf(props.calls)}</span>;
}

/** How many of several calls failed; nothing where none did. */
function Failed(props: { readonly calls: readonly CallLine[] }): ReactNode {
	const failed = props.calls.filter((call) => call.state === "failed" || call.trouble !== undefined).length;
	return failed === 0 ? null : <span className="shrink-0 text-xs text-error-foreground">{failed} failed</span>;
}

/**
 * What a call's line says at its end of how it stands: waiting for the person, declined, or stopped; nothing while it
 * runs, once it went well, or where it went wrong — its mark says so, its why told as it opens.
 */
function CallEnd(props: { readonly call: CallLine }): ReactNode {
	const { call } = props;
	switch (call.state) {
		case "failed":
		case "done":
		case "running":
			return null;
		case "waiting":
			return <Waiting />;
		case "rejected":
			return (
				<span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
					<Ban className="size-3.5" />
					{declinedText(call)}
				</span>
			);
		case "stopped":
			return <span className="shrink-0 text-xs text-muted-foreground">Stopped</span>;
	}
}

/** How long a stretch's calls took, from the first's start to the last's end, or have run while one runs; nothing where none began. */
function StretchTime(props: { readonly calls: readonly CallLine[] }): ReactNode {
	const began = Math.min(...props.calls.flatMap((call) => (call.began === undefined ? [] : [call.began])));
	if (!Number.isFinite(began)) return null;
	if (props.calls.some((call) => call.state === "running")) return <Counting since={began} written={callDuration} />;
	const ended = Math.max(...props.calls.flatMap((call) => (call.ended === undefined ? [] : [call.ended])));
	return Number.isFinite(ended) ? callDuration(ended - began) : null;
}

/** What the model thought on its way, as its reply gave it: folded under how long its request took; open, its words. */
export const Thought = memo(function Thought(props: { readonly text: string; readonly took: number | undefined }): ReactNode {
	return (
		<Collapsible>
			<CollapsibleTrigger className="work-line work-opens">
				<Mark>
					<Brain />
				</Mark>
				{thoughtWords(props.took)}
				<FoldChevron className="quiet-chevron" />
			</CollapsibleTrigger>
			<CollapsiblePanel>
				<Prose text={props.text} className="work-indent pt-0.5 pb-2 text-ui text-muted-foreground" />
			</CollapsiblePanel>
		</Collapsible>
	);
});
