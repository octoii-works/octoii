/**
 * A thread, in the page's frame: its column — its header, with the buttons of its details and the one that shows its
 * panel; its timeline in a column of the chat's width, each turn's words, its work and its answer, the reply being
 * written, kept at its end as it grows; docked at its foot, opaque, the messages waiting for the turn's end over the
 * composer, or over what waits on the person, in the composer's place while it waits; and its details beside the conversation where the column has room — and after it its panel, where it is shown: its files and changes, its terminals and pages,
 * which a path a message names opens in. What the thread brought is seen as it is shown.
 */
import { isUnreadyWorktreeStart, WorktreeStart } from "../extensions/git/thread-start.tsx";
import { type ReactNode, useMemo, useRef, useState } from "react";
import type { Conversation, Listed, Status } from "@octoii-works/octoii-client";
import { useClient, useConversation, useTerminals, useThreads } from "@octoii-works/octoii-client/react";
import type { Device } from "../core/device.ts";
import { type Project, projectOf, statusOf, stopTargetOf, titleOf } from "../core/threads.ts";
import { contentOf } from "../core/attachments.ts";
import { localServersOf } from "../core/local-servers.ts";
import { limitedOf } from "../core/limits.ts";
import { inBackground, waitingOn } from "../core/sub-threads.ts";
import { firstSaid, itemsOf, withLimited, withNow } from "../core/timeline.ts";
import { useApproval } from "../components/asks/approval.tsx";
import { AskDock } from "../components/asks/asks.tsx";
import { Composer } from "../components/composer/composer.tsx";
import { Queued } from "../components/composer/queued.tsx";
import { Pane, PanelFrame, ShowPanel } from "../components/frame/frame.tsx";
import { ChatHeader } from "../components/thread/header.tsx";
import { DetailsButton, DetailsCard, DetailsFloat } from "../components/thread/details.tsx";
import { Timeline, type TimelineHandle } from "../components/thread/timeline.tsx";
import { RightPanel } from "../components/panel/right-panel.tsx";
import { MessageThreadContext } from "../components/thread/message-thread.ts";
import { useExtension, useExtensionAccess } from "../components/state/extensions.ts";
import { thingsOf } from "../core/mentions.ts";
import type { Thing } from "../core/things.ts";
import { failed } from "../components/state/notices.ts";
import { useSeen } from "../components/effects/seen.ts";
import { useNow } from "../components/effects/now.ts";
import { useDetailsRoom } from "../components/effects/details-room.ts";
import { useFolder } from "../components/state/folder.ts";
import { useBrowserPanel, useThreadBrowser } from "../extensions/browser/panel.tsx";
import { useGitPanel } from "../extensions/git/panel.tsx";
import { useTerminalsPanel } from "../extensions/terminals/panel.tsx";
import { VersionControl } from "../extensions/git/version-control.tsx";
import { MemoryRow } from "../extensions/memory/memory-row.tsx";
import { TerminalRows } from "../extensions/terminals/terminal-rows.tsx";

/** How often a page looks again whether the quota that limits its thread is given again: a minute. */
const LIMITED_CHECKED_MS = 60_000;
const NO_ENTRIES: readonly [] = [];

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
	const status = visibleStatus(conversation, row);
	const terminalsOn = useExtensionAccess("terminals", "terminals.operate");
	const { available: browserOn, pages, openBrowser, pageReference } = useThreadBrowser(thread);
	const terminals = useTerminals(terminalsOn ? thread : undefined) ?? NO_ENTRIES;
	const terminalsPanel = useTerminalsPanel(thread, terminals, terminalsOn, browserOn ? openBrowser : undefined);
	const servers = useMemo(() => (browserOn ? localServersOf(pages, terminals, conversation.turns) : []), [browserOn, pages, terminals, conversation.turns]);
	const reachesThreads = useExtension("thread");
	const now = useNow(LIMITED_CHECKED_MS);
	useSeen(thread);
	// The input the person last sent from here, by the id it was delivered by, which the timeline keeps near its top as
	// the reply grows under it.
	const [sent, setSent] = useState<string | undefined>(undefined);
	const timeline = useRef<TimelineHandle>(null);
	const onSent = (delivery: string): void => {
		setSent(delivery);
		timeline.current?.toEnd();
	};
	const browserPanel = useBrowserPanel(thread, { pages, on: browserOn, onSent, servers });
	const limited = limitedOf(conversation, row?.config.model ?? undefined, now);
	const turnsAndLimit = withLimited(itemsOf(conversation), limited, conversation.status?.resuming_at ?? undefined);
	const work = turnsAndLimit.flatMap((item) => (item.kind === "turn" ? item.turn.work : []));
	// What the person sent, oldest first, which the composer's ↑ calls back.
	const theirs = turnsAndLimit.flatMap((item) =>
		item.kind === "turn" ? item.turn.said.filter((said) => said.from === undefined && said.text !== "").map((said) => said.text) : [],
	);
	// The thread's present stands as the timeline's last lines: its sub-threads that wait on the person, where it waits on them for nothing itself; then what works in its background.
	const items = withNow(turnsAndLimit, {
		waiting: conversation.asks.length > 0 ? [] : waitingOn(rows, thread),
		terminals: terminals.filter((terminal) => terminal.line !== undefined),
		threads: inBackground(work, rows, thread),
	});
	const project = row === undefined ? undefined : projectOf(row);
	const workspace = useFolder(row, status);
	const said = firstSaid(items);
	// Where the thread's turns stand, which its repository and its changes are read again at.
	const revision = `${conversation.status?.turn ?? ""} ${String(conversation.status?.ended_at ?? "")}`;
	const gitOn = useExtensionAccess("git", "files.read");
	const git = useGitPanel(thread, workspace, revision, gitOn);
	const parts = {
		folder: useExtensionAccess("memory", "files.read") && workspace !== undefined ? <MemoryRow folder={workspace} revision={revision} /> : null,
		sections: (
			<>
				{gitOn && workspace !== undefined ? <VersionControl thread={thread} folder={workspace} revision={revision} /> : null}
				{terminalsOn ? <TerminalRows thread={thread} terminals={terminals} /> : null}
			</>
		),
	};
	const details = (
		<DetailsCard thread={thread} row={row} workspace={workspace} turns={conversation.turns} compactions={conversation.compactions} parts={parts} />
	);
	return (
		<>
			<ThreadPane title={row?.config.title ?? (said === undefined ? "New thread" : titleOf(said))} status={status} project={project} details={details}>
				<div className="relative flex min-h-0 flex-1 flex-col">
					<MessageThreadContext value={{ thread, workspace, pages, terminals, ...(browserOn ? { openBrowser, pageReference } : {}) }}>
						<ThreadConversation thread={thread} status={status}>
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
						</ThreadConversation>
					</MessageThreadContext>
				</div>
				<Dock
					thread={thread}
					row={row}
					workspace={workspace}
					things={thingsOf(reachesThreads ? rows : [], terminals, pages, thread)}
					sent={theirs}
					conversation={conversation}
					status={status}
					device={props.device}
					onSent={onSent}
				/>
			</ThreadPane>
			<PanelFrame>
				<RightPanel
					thread={thread}
					folder={workspace}
					revision={revision}
					surfaces={[...git.surfaces, ...terminalsPanel.surfaces, ...browserPanel.surfaces]}
					openings={[...terminalsPanel.openings, ...browserPanel.openings, ...git.openings]}
					openingGroups={browserPanel.groups}
				/>
			</PanelFrame>
		</>
	);
}

/** The floating details share the measured conversation pane with the timeline and composer. */
function ThreadPane(props: {
	readonly title: string;
	readonly status: Status | undefined;
	readonly project: Project | undefined;
	readonly details: ReactNode;
	readonly children: ReactNode;
}): ReactNode {
	const { room, pane } = useDetailsRoom();
	return (
		<Pane>
			<ChatHeader
				title={props.title}
				status={props.status === undefined ? undefined : statusOf(props.status)}
				{...(props.project === undefined ? {} : { project: props.project })}
				actions={
					<>
						<DetailsButton room={room}>{props.details}</DetailsButton>
						<ShowPanel />
					</>
				}
			/>
			<div ref={pane} className="flex min-h-0 flex-1">
				<div className="flex min-h-0 min-w-0 flex-1 flex-col">{props.children}</div>
				{room ? <DetailsFloat>{props.details}</DetailsFloat> : null}
			</div>
		</Pane>
	);
}

function visibleStatus(conversation: Conversation, row: Listed | undefined): Status | undefined {
	return conversation.gone ? undefined : (conversation.status ?? row?.status);
}

/**
 * What docks at the column's foot, opaque, the turns fading out behind its upper edge: the messages waiting in the
 * thread's inbox over the composer, in the chat's column — or, while something waits on the person, over what waits,
 * docked in the composer's place till it is answered.
 */
function Dock(props: {
	readonly thread: string;
	readonly row: Listed | undefined;
	readonly workspace: string | undefined;
	/** The things the composer's `@` mentions past files. */
	readonly things: readonly Thing[];
	/** What the person sent in the thread, oldest first. */
	readonly sent: readonly string[];
	readonly conversation: Conversation;
	readonly status: Status | undefined;
	readonly device: Device;
	/** Told the id each input the person sends is delivered by. */
	readonly onSent: (input: string) => void;
}): ReactNode {
	const { thread, row, workspace, conversation } = props;
	const client = useClient();
	const working = conversation.status?.turn === "running" || conversation.inbox.length > 0;
	const stop = stopTargetOf(conversation.status);
	// A call's permission, the oldest of what waits, is answered in the composer; anything else waits docked in its place.
	const approval = useApproval(thread, conversation.asks.toSorted((a, b) => (a.key < b.key ? -1 : 1))[0]);
	if (!conversation.read || conversation.gone || isUnreadyWorktreeStart(props.status?.start)) return null;

	return (
		<div className="relative shrink-0 bg-background pb-[max(--spacing(4),env(safe-area-inset-bottom))]">
			<div aria-hidden className="pointer-events-none absolute inset-x-0 bottom-full h-6 bg-linear-to-t from-background to-transparent" />
			{/* The list's bar is given its room here too, so this column stands where the list's does. */}
			<div className="overflow-hidden scrollbar-even">
				<div className="chat-column">
					{/* What waits steers a turn that works, docked till its next step; sent to a thread at rest, it begins the next turn, shown in the timeline as it does. */}
					<Queued thread={thread} inbox={conversation.inbox} />
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
							things={props.things}
							onSend={async ({ text, attachments, feedback }) => {
								props.onSent(await client.deliver(thread, contentOf(text, attachments, feedback)));
							}}
							onQueue={async ({ text, attachments, feedback }) => {
								props.onSent(await client.queue(thread, contentOf(text, attachments, feedback)));
							}}
							onStop={
								stop === undefined
									? undefined
									: () => void client.stop(thread, stop).catch((error: unknown) => failed("The thread could not be stopped", error))
							}
							approval={approval}
							sent={props.sent}
						/>
					)}
				</div>
			</div>
		</div>
	);
}

function ThreadConversation({
	thread,
	status,
	children,
}: {
	readonly thread: string;
	readonly status: Status | undefined;
	readonly children: ReactNode;
}): ReactNode {
	const start = status?.start;
	const hideTimeline = isUnreadyWorktreeStart(start) && start.state.state !== "unavailable";
	return (
		<>
			{status === undefined ? null : <WorktreeStart thread={thread} status={status} />}
			<div className={hideTimeline ? "hidden" : "contents"}>{children}</div>
		</>
	);
}
