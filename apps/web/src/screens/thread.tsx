/**
 * A thread, in the page's frame: its column — its header, with the buttons of its details and the one that shows its
 * panel; its timeline in a column of the chat's width, each turn's words, its work and its answer, the reply being
 * written, kept at its end as it grows; docked at its foot, opaque, the messages waiting for the turn's end over the
 * composer, or over what waits on the person, in the composer's place while it waits; and its details on a card floating at the timeline's top right, where the column is wide enough, the chat's
 * column centred under it — and after it its panel, where it is shown: its files and changes, its terminals and pages,
 * which a path a message names opens in. What the thread brought is seen as it is shown.
 */
import { type ReactNode, useRef, useState } from "react";
import type { Conversation, Listed } from "@octoii-works/octoii-client";
import { useClient, useConversation, usePages, useTerminals, useThreads } from "@octoii-works/octoii-client/react";
import type { Device } from "../core/device.ts";
import { projectOf, statusOf, titleOf } from "../core/threads.ts";
import { contentOf } from "../core/attachments.ts";
import { limitedOf } from "../core/limits.ts";
import { inBackground, waitingOn } from "../core/sub-threads.ts";
import { firstSaid, itemsOf, withLimited, withNow } from "../core/timeline.ts";
import { useApproval } from "../components/asks/approval.tsx";
import { AskDock } from "../components/asks/asks.tsx";
import { Composer } from "../components/composer/composer.tsx";
import { Queued } from "../components/composer/queued.tsx";
import { Pane, PanelFrame, ShowPanel } from "../components/frame/frame.tsx";
import { ChatHeader } from "../components/thread/header.tsx";
import { DetailsButtons, type DetailsProps, InlineDetails } from "../components/thread/details.tsx";
import { Timeline, type TimelineHandle } from "../components/thread/timeline.tsx";
import { RightPanel } from "../components/panel/right-panel.tsx";
import { MessageThreadContext } from "../components/thread/message-thread.ts";
import { failed } from "../components/state/notices.ts";
import { useSeen } from "../components/effects/seen.ts";
import { useNow } from "../components/effects/now.ts";
import { useFolder } from "../components/state/folder.ts";

/** How often a page looks again whether the quota that limits its thread is given again: a minute. */
const LIMITED_CHECKED_MS = 60_000;

/** What a thread's page is given: its thread, and the device's settings. */
export type ThreadScreenProps = {
	readonly thread: string;
	readonly device: Device;
};

/** A thread's page. */
export function ThreadScreen(props: ThreadScreenProps): ReactNode {
	const { thread } = props;
	const client = useClient();
	const conversation = useConversation(thread);
	const rows = useThreads() ?? [];
	const row = rows.find((each) => each.id === thread);
	const terminals = useTerminals(thread) ?? [];
	const pages = usePages(thread) ?? [];
	const now = useNow(LIMITED_CHECKED_MS);
	useSeen(thread);
	// The input the person last sent from here, by the id it was delivered by, which the timeline keeps near its top as
	// the reply grows under it.
	const [sent, setSent] = useState<string | undefined>(undefined);
	const timeline = useRef<TimelineHandle>(null);
	const limited = limitedOf(conversation, row?.config.model ?? undefined, now);
	const turnsAndLimit = withLimited(itemsOf(conversation), limited, conversation.status?.resuming_at ?? undefined);
	const work = turnsAndLimit.flatMap((item) => (item.kind === "turn" ? item.turn.work : []));
	// The thread's present stands as the timeline's last lines: its sub-threads that wait on the person, where it waits on them for nothing itself; then what works in its background.
	const items = withNow(turnsAndLimit, {
		waiting: conversation.asks.length > 0 ? [] : waitingOn(rows, thread),
		terminals: terminals.filter((terminal) => terminal.line !== undefined),
		threads: inBackground(work, rows, thread),
	});
	const project = row === undefined ? undefined : projectOf(row);
	const workspace = useFolder(row);
	const said = firstSaid(items);
	// Where the thread's turns stand, which its repository and its changes are read again at.
	const revision = `${conversation.status?.turn ?? ""} ${String(conversation.status?.ended_at ?? "")}`;
	const details: DetailsProps = { thread, row, turns: conversation.turns, compactions: conversation.compactions, revision };
	return (
		<>
			<Pane>
				<ChatHeader
					title={row?.config.title ?? (said === undefined ? "New thread" : titleOf(said))}
					status={row === undefined ? undefined : statusOf(row)}
					{...(project === undefined ? {} : { project })}
					actions={
						<div className="-mr-1 flex items-center gap-1">
							<DetailsButtons {...details} />
							<ShowPanel />
						</div>
					}
				/>
				{/* The timeline's room, between the header and the dock, its details floating at its top right. */}
				<div className="relative flex min-h-0 flex-1 flex-col">
					<MessageThreadContext value={{ thread, workspace, pages }}>
						<Timeline
							ref={timeline}
							thread={thread}
							parent={row?.parent?.thread}
							items={items}
							read={conversation.read}
							complete={conversation.complete}
							working={conversation.status?.turn === "running"}
							waiting={conversation.asks.length}
							sent={sent}
							onEarlier={() => void client.readOlder(thread).catch((error: unknown) => failed("Earlier messages could not be read", error))}
						/>
					</MessageThreadContext>
					<InlineDetails {...details} />
				</div>
				<Dock
					thread={thread}
					row={row}
					conversation={conversation}
					device={props.device}
					onSent={(delivery) => {
						setSent(delivery);
						timeline.current?.toEnd();
					}}
				/>
			</Pane>
			<PanelFrame>
				<RightPanel thread={thread} folder={workspace} terminals={terminals} pages={pages} revision={revision} />
			</PanelFrame>
		</>
	);
}

/**
 * What docks at the column's foot, opaque, the turns fading out behind its upper edge: the messages waiting in the
 * thread's inbox over the composer, in the chat's column — or, while something waits on the person, over what waits,
 * docked in the composer's place till it is answered.
 */
function Dock(props: {
	readonly thread: string;
	readonly row: Listed | undefined;
	readonly conversation: Conversation;
	readonly device: Device;
	/** Told the id each input the person sends is delivered by. */
	readonly onSent: (input: string) => void;
}): ReactNode {
	const { thread, row, conversation } = props;
	const client = useClient();
	const workspace = useFolder(row);
	const working = conversation.status?.turn === "running" || conversation.inbox.length > 0;
	// A call's permission, the oldest of what waits, is answered in the composer; anything else waits docked in its place.
	const approval = useApproval(thread, conversation.asks.toSorted((a, b) => (a.key < b.key ? -1 : 1))[0]);
	return (
		<div className="relative shrink-0 bg-background pb-[max(--spacing(4),env(safe-area-inset-bottom))]">
			<div aria-hidden className="pointer-events-none absolute inset-x-0 bottom-full h-6 bg-linear-to-t from-background to-transparent" />
			{/* The list's bar is given its room here too, so this column stands where the list's does. */}
			<div className="overflow-hidden scrollbar-even">
				<div className="chat-column">
					{/* What waits steers a turn that works, docked till its next step; sent to a thread at rest, it begins the next turn, shown in the timeline as it does. */}
					<Queued thread={thread} inbox={conversation.status?.turn === "running" ? conversation.inbox : []} />
					{conversation.asks.length > 0 && approval === undefined ? (
						<AskDock thread={thread} asks={conversation.asks} />
					) : (
						<Composer
							model={row?.config.model ?? undefined}
							onModel={(model) => void client.configure(thread, { model }).catch((error: unknown) => failed("The model could not be changed", error))}
							mode={row?.config.mode ?? undefined}
							onMode={(mode) => void client.configure(thread, { mode }).catch((error: unknown) => failed("The mode could not be changed", error))}
							effort={row?.config.effort ?? undefined}
							onEffort={(effort) => void client.configure(thread, { effort }).catch((error: unknown) => failed("The effort could not be changed", error))}
							working={working}
							sendShortcut={props.device.send_shortcut}
							{...(working ? { placeholder: "Steer the work: your message joins at its next step" } : {})}
							draft={{ thread }}
							folder={workspace}
							onSend={async ({ text, attachments }) => {
								props.onSent(await client.deliver(thread, contentOf(text, attachments)));
							}}
							onStop={() => void client.stop(thread).catch((error: unknown) => failed("The thread could not be stopped", error))}
							approval={approval}
						/>
					)}
				</div>
			</div>
		</div>
	);
}
