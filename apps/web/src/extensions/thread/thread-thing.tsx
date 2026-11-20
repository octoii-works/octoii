/**
 * A thread of the person's as a message names it, `:thread{id=…}`: its chip, a dot of how it stands and its title; its
 * card, the mark of how it stands, its title, and how it stands and how long it worked — going into it as either is
 * clicked; one deleted said to be gone.
 */
import type { ReactNode } from "react";
import type { Listed } from "@octoii-works/octoii-client";
import { useThreads } from "@octoii-works/octoii-client/react";
import { standingOf, workedOf } from "../../core/sub-threads.ts";
import { statusOf, THREAD_STATUS_LABELS, threadTitle } from "../../core/threads.ts";
import { cn } from "../../components/ui/cn.ts";
import { IconTile } from "../../components/ui/icon-tile.tsx";
import { standingDot } from "../../components/thread/entity.tsx";
import { StatusMark } from "../../components/thread/status-mark.tsx";
import { WorkedFor } from "../../components/thread/sub-thread.tsx";
import { CARD, CardWords, CHIP, GONE_CHIP, type ThingView } from "../../components/thread/thing-look.tsx";

/** The thread `thread` as the person's threads list it; none where it is not listed. */
function useListed(thread: string): Listed | undefined {
	return useThreads()?.find((each) => each.id === thread);
}

/** A thread named within a line: a dot of how it stands and its title, going into it as it is clicked. */
function ThreadChip(props: { readonly thread: string }): ReactNode {
	const row = useListed(props.thread);
	if (row === undefined) return <span className={GONE_CHIP}>a thread no longer there</span>;
	const unavailable = statusOf(row.status) === "unavailable";
	return (
		<a href={`#/thread/${props.thread}`} className={CHIP} title={unavailable ? THREAD_STATUS_LABELS.unavailable : undefined}>
			<span className={cn("size-1.5 shrink-0 self-center rounded-full", standingDot(unavailable ? "failed" : standingOf(row)))} />
			{threadTitle(row)}
			{unavailable ? <span className="sr-only"> ({THREAD_STATUS_LABELS.unavailable})</span> : null}
		</a>
	);
}

/** A thread named on a line of its own: the mark of how it stands, its title, how it stands and how long it worked. */
function ThreadCard(props: { readonly thread: string }): ReactNode {
	const row = useListed(props.thread);
	if (row === undefined)
		return (
			<button type="button" disabled className={CARD}>
				<CardWords name="A thread no longer there" />
			</button>
		);
	const status = statusOf(row.status);
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
						{THREAD_STATUS_LABELS[status]}
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

/** A thread, as a message names it. */
export const THREAD_VIEW: ThingView = { Chip: (props) => <ThreadChip thread={props.id} />, Card: (props) => <ThreadCard thread={props.id} /> };
