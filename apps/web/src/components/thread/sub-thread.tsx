/**
 * Sub-threads in the timeline of the thread that delegated to them, as lines of its work: a thing's line for each — a
 * bot's round mark with a dot of how it stands, its title, under it its task or what it said, how long it worked, and a
 * way into it — for the calls that gave them tasks, those of one step gathered under one mark of how they stand together, how
 * many there are, how they stand and how long they worked, opened to list them at the same edges; and a row for what came of a task given in the
 * background, as the sub-thread told it, marked how its work ended and when.
 */
import type { ReactNode } from "react";
import type { Listed } from "@octoii-works/octoii-client";
import { useThreads } from "@octoii-works/octoii-client/react";
import { Bot } from "lucide-react";
import { plainLine, type Standing, standingOf, standingsSummary, type Worked, workedOf } from "../../core/sub-threads.ts";
import { subThreadOf } from "../../core/threads.ts";
import { duration } from "../../core/time.ts";
import type { CallLine, Said } from "../../core/timeline.ts";
import { callWords } from "../../core/tools.ts";
import { Counting } from "../effects/now.ts";
import { go } from "../state/navigation.ts";
import { Collapsible, CollapsiblePanel, CollapsibleTrigger, FoldChevron } from "../ui/collapsible.tsx";
import { EntityContent, EntityMark, StandingDot } from "./entity.tsx";

/** The delegations of one step, made by the thread `thread`: one a row, several gathered. */
export function Delegations(props: { readonly thread: string; readonly calls: readonly CallLine[] }): ReactNode {
	const rows = useThreads() ?? [];
	const delegated = props.calls.map((call) => ({ call, row: subThreadOf(rows, props.thread, call) }));
	const [only] = delegated;
	if (only !== undefined && delegated.length === 1) return <DelegationRow call={only.call} row={only.row} />;
	return <Gathered delegated={delegated} />;
}

/** Several delegations of a step, under how many they are and how they stand — clear while one works — opened to list each one's row. */
function Gathered(props: { readonly delegated: readonly { readonly call: CallLine; readonly row: Listed | undefined }[] }): ReactNode {
	const standings = props.delegated.map(({ call, row }) => standingOf(row, call));
	const tone = standings.includes("running") ? "running" : standings.includes("failed") ? "failed" : undefined;
	const worked = workedOf(props.delegated.map(({ row }) => row));
	return (
		<Collapsible>
			<CollapsibleTrigger data-tone={tone} className="work-line work-opens">
				<SubThreadMark standing={tone ?? (standings.every((standing) => standing === "stopped") ? "stopped" : "completed")} />
				<span className="shrink-0 text-foreground">{`${String(standings.length)} sub-threads`}</span>
				<span className="min-w-0 truncate group-data-[tone=failed]/fold:text-error-foreground group-data-[tone=running]/fold:text-info-foreground">
					{standingsSummary(standings)}
				</span>
				<FoldChevron className="quiet-chevron" />
				<span className="flex-1" />
				{worked === undefined ? null : (
					<span className="shrink-0 text-xs text-muted-foreground tabular-nums">
						<WorkedFor worked={worked} />
					</span>
				)}
			</CollapsibleTrigger>
			<CollapsiblePanel>
				<div className="work-list work-tight">
					{props.delegated.map(({ call, row }) => (
						<DelegationRow key={call.id} call={call} row={row} />
					))}
				</div>
			</CollapsiblePanel>
		</Collapsible>
	);
}

/** A delegation's line: its sub-thread as it stands, its task under its title where the title is its own; or, where it is not listed, the call as it stands. */
function DelegationRow(props: { readonly call: CallLine; readonly row: Listed | undefined }): ReactNode {
	const { call, row } = props;
	const standing = standingOf(row, call);
	const worked = workedOf([row]);
	const title = row?.config.title ?? call.subject ?? callWords(call);
	return (
		<SubThreadRow
			title={title}
			standing={standing}
			detail={call.subject === undefined || call.subject === title ? undefined : call.subject}
			time={worked === undefined ? undefined : <WorkedFor worked={worked} />}
			thread={row?.id}
		/>
	);
}

/** What came of a task given in the background, as the sub-thread `said` it: how it stands, what it said, and when. */
export function SubThreadTold(props: { readonly said: Said }): ReactNode {
	const { said } = props;
	const row = useThreads()?.find((each) => each.id === said.from);
	const detail = plainLine(said.text);
	return (
		<SubThreadRow
			title={row?.config.title ?? "Sub-thread"}
			standing={standingOf(row)}
			detail={detail === "" ? undefined : detail}
			time={new Date(said.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
			thread={row?.id}
		/>
	);
}

/** A sub-thread's line: its mark with a dot of how it stands, its title, what it does or said under it, its time, and, where it is listed, a way into it. */
function SubThreadRow(props: {
	readonly title: string;
	readonly standing: Standing;
	readonly detail: string | undefined;
	readonly time: ReactNode;
	readonly thread: string | undefined;
}): ReactNode {
	const thread = props.thread;
	const content = (
		<EntityContent
			mark={<SubThreadMark standing={props.standing} />}
			standing={props.standing}
			name={props.title}
			detail={props.detail}
			time={props.time}
			opens={thread === undefined ? undefined : "elsewhere"}
		/>
	);
	if (thread === undefined) return <div className="work-line">{content}</div>;
	return (
		<button
			type="button"
			aria-label={`Open ${props.title}`}
			onClick={() => go({ place: "thread", thread })}
			className="group/entity work-line work-opens cursor-pointer"
		>
			{content}
		</button>
	);
}

/** How long sub-threads worked: counted on each second while one works, as it stood once all ended. */
export function WorkedFor(props: { readonly worked: Worked }): ReactNode {
	const { began, ended } = props.worked;
	return ended === undefined ? <Counting since={began} written={duration} /> : duration(ended - began);
}

/** A sub-thread's mark among the lines of the work: a bot on a round, with a dot of how it stands. */
function SubThreadMark(props: { readonly standing: Standing }): ReactNode {
	return (
		<EntityMark standing={props.standing}>
			<Bot />
		</EntityMark>
	);
}

/** A sub-thread's icon, as a card lists it among its icons: a bot, with a dot of how it stands where `standing` is given. */
export function SubThreadAvatar(props: { readonly standing: Standing | undefined }): ReactNode {
	return (
		<span className="relative inline-flex shrink-0 text-muted-foreground">
			<Bot className="size-4" />
			{props.standing === undefined ? null : <StandingDot standing={props.standing} />}
		</span>
	);
}
