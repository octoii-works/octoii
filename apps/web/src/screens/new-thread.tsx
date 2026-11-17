/** A local draft: project headline, centered composer, shared context controls and floating details. */
import { type ReactNode, useMemo, useState } from "react";
import { useClient, useConnection, useSettings, useThreads } from "@octoii-works/octoii-client/react";
import { Folder, Laptop } from "lucide-react";
import type { Device } from "../core/device.ts";
import { recentModelsOf } from "../core/models.ts";
import type { AcceptWorktree, NewThread, PendingThread } from "../core/new-thread.ts";
import { usePendingThread } from "../components/effects/pending-thread.ts";
import { useOutcome } from "../components/effects/read.ts";
import { failed } from "../components/state/notices.ts";
import { clearPendingDraft } from "../components/state/draft.ts";
import { thingsOf } from "../core/mentions.ts";
import { useExtension } from "../components/state/extensions.ts";
import { folderName, projectsOf } from "../core/threads.ts";
import { Composer } from "../components/composer/composer.tsx";
import { ModeControl } from "../components/composer/mode-picker.tsx";
import { FolderChooser } from "../components/files/folder-chooser.tsx";
import { Pane } from "../components/frame/frame.tsx";
import { go } from "../components/state/navigation.ts";
import { ChatHeader } from "../components/thread/header.tsx";
import { DraftProject } from "../components/thread/draft-project.tsx";
import { DetailsToggle, DetailsFloat, DetailRow } from "../components/thread/details.tsx";
import { useNewThreadOptions } from "../components/state/new-thread-options.ts";
import { useReach } from "../components/state/reach.ts";
import { prepareWorktree } from "../extensions/git/prepare.ts";
import { acceptWorktree } from "../extensions/git/submit.ts";
import { WorkspaceChoice, BaseBranchChoice } from "../extensions/git/new-worktree.tsx";
import { ServersMenu } from "../extensions/mcp/servers-menu.tsx";
import type { Draft } from "../core/drafts.ts";

export type NewThreadScreenProps = { readonly project: string | undefined; readonly device: Device };

export function NewThreadScreen(props: NewThreadScreenProps): ReactNode {
	const client = useClient();
	const connection = useConnection();
	const { send } = useReach();
	const accept = useMemo<AcceptWorktree>(() => (submission) => acceptWorktree(send, submission), [send]);
	const saved = usePendingThread(client, props.project, accept);
	const { value: options, change } = useNewThreadOptions(connection.status === "open" ? connection.session : undefined, props.project);
	const [choosing, setChoosing] = useState(false);
	const [sending, setSending] = useState(false);
	const rows = useThreads() ?? [];
	const reachesThreads = useExtension("thread");
	const mcp = useExtension("mcp");
	const settings = useSettings();
	const folder = options.chosen === undefined ? props.project : options.chosen.folder;
	const model = options.model ?? recentModelsOf(rows)[0];
	const mode = options.mode ?? settings?.environment.thread_mode;
	const projects = projectsOf(rows);
	const branches = useDraftBranches(folder);
	const choose = (next: string | undefined): void => {
		change({ chosen: { folder: next }, worktree: undefined });
		setChoosing(false);
	};
	const projectPicker = <DraftProject folder={folder} projects={projects} onChoose={choose} onAnother={() => setChoosing(true)} />;
	const workspace =
		branches !== undefined && "result" in branches && folder !== undefined ? (
			<WorkspaceChoice folder={folder} branches={branches.result} value={options.worktree} onChange={(worktree) => change({ worktree })} />
		) : null;
	const branch =
		branches !== undefined && "result" in branches && folder !== undefined ? (
			<BaseBranchChoice folder={folder} branches={branches.result} value={options.worktree} onChange={(worktree) => change({ worktree })} />
		) : null;
	const details = (
		<DraftDetails
			machine={connection.status === "open" ? connection.machine.name : "This machine"}
			project={projectPicker}
			workspace={workspace}
			branch={branch}
			servers={mcp ? <ServersMenu folder={folder} /> : null}
		/>
	);
	const submit = useDraftSubmit(saved, folder, model, options, setSending);
	return (
		<Pane>
			<ChatHeader
				title="New thread"
				{...(folder === undefined ? {} : { project: { folder, name: folderName(folder) } })}
				actions={
					<DetailsToggle>
						<fieldset disabled={sending || saved.pending !== undefined}>{details}</fieldset>
					</DetailsToggle>
				}
			/>
			<div className="flex min-h-0 flex-1">
				<div className="flex min-w-0 flex-1 items-center justify-center overflow-y-auto px-4 py-8">
					<div className="w-full max-w-(--chat-width)">
						<fieldset disabled={sending || saved.pending !== undefined} className="min-w-0 border-0 p-0">
							<DraftHeadline folder={folder} project={projectPicker} onNone={() => choose(undefined)} />
						</fieldset>
						{!saved.loaded ? (
							<LoadingDraft error={saved.error} retry={saved.retry} />
						) : saved.pending !== undefined ? (
							<PendingTask pending={saved.pending} starting={saved.starting} onForgotten={() => saved.setPending(undefined)} />
						) : (
							<>
								<Composer
									model={model}
									onModel={(model) => change({ model })}
									mode={undefined}
									onMode={(mode) => change({ mode })}
									effort={options.effort}
									onEffort={(effort) => change({ effort })}
									working={false}
									sendShortcut={props.device.send_shortcut}
									draft={{ project: folder }}
									folder={folder}
									things={thingsOf(reachesThreads ? rows : [], [], [], undefined)}
									onSend={submit}
									onStop={() => undefined}
								/>
								<fieldset disabled={sending} className="mt-1 flex min-w-0 flex-wrap items-center justify-between gap-1 border-0 p-0">
									<div className="min-w-0">{workspace}</div>
									<div className="flex min-w-0 max-w-full items-center gap-1">
										{branch}
										{mode === undefined ? null : <ModeControl mode={mode} onChoose={(mode) => change({ mode })} />}
									</div>
								</fieldset>
								{branches !== undefined && "failure" in branches ? (
									<p role="alert" className="mt-2 text-xs text-muted-foreground">
										{branches.failure}
									</p>
								) : null}
							</>
						)}
					</div>
				</div>
				<fieldset disabled={sending || saved.pending !== undefined} className="contents">
					<DetailsFloat>{details}</DetailsFloat>
				</fieldset>
			</div>
			<FolderChooser open={choosing} onOpenChange={setChoosing} start={folder} onChoose={choose} />
		</Pane>
	);
}

function LoadingDraft({ error, retry }: { readonly error: string | undefined; readonly retry: () => void }): ReactNode {
	return (
		<p className="p-4 text-body text-muted-foreground">
			{error === undefined ? (
				"Loading saved task…"
			) : (
				<>
					Saved tasks could not be read: {error}.{" "}
					<button type="button" className="underline" onClick={retry}>
						Retry
					</button>
				</>
			)}
		</p>
	);
}
function PendingTask({
	pending,
	starting,
	onForgotten,
}: {
	readonly pending: PendingThread;
	readonly starting: NewThread | undefined;
	readonly onForgotten: () => void;
}): ReactNode {
	const [resuming, setResuming] = useState(false);
	return (
		<div className="rounded-lg border border-border p-4 text-body">
			<p className="font-medium">Unfinished submission</p>
			<p className="my-3 max-h-48 overflow-auto whitespace-pre-wrap">{pending.draft.text}</p>
			<p className="text-muted-foreground">
				{pending.config.workspace ?? "No project"} · {pending.draft.attachments.length} attachments
			</p>
			{pending.worktree === undefined ? null : (
				<p className="mt-2 break-all text-muted-foreground">
					New worktree: {pending.worktree.branch} · {pending.worktree.destination}
				</p>
			)}
			<p className="my-3 text-muted-foreground">
				Continue the original submission. If it was already accepted, this opens its thread without sending it again.
			</p>
			<button
				type="button"
				disabled={resuming}
				className="rounded-md bg-primary px-3 py-2 text-primary-foreground disabled:opacity-50"
				onClick={() => {
					if (starting === undefined || resuming) return;
					setResuming(true);
					void starting
						.resume()
						.then(async (thread) => {
							clearPendingDraft({ project: pending.config.workspace ?? undefined }, pending.draft);
							go({ place: "thread", thread });
							await starting.acknowledge();
						})
						.catch((error: unknown) => failed("The task could not be resumed", error))
						.finally(() => setResuming(false));
				}}
			>
				{resuming ? "Continuing…" : "Continue task"}
			</button>
			{pending.thread === undefined ? null : (
				<button type="button" className="ml-3 text-primary underline" onClick={() => go({ place: "thread", thread: pending.thread ?? "" })}>
					View existing thread
				</button>
			)}
			<p className="mt-3 text-xs text-muted-foreground">
				{pending.worktree === undefined ? "Recovery is available for 23 hours. " : ""}Discarding this saved copy does not stop or delete an existing thread.
			</p>
			<button
				type="button"
				disabled={resuming}
				className="mt-2 text-xs text-muted-foreground underline"
				onClick={() => {
					if (starting === undefined || resuming) return;
					setResuming(true);
					void starting
						.forget(pending.key)
						.then(onForgotten)
						.catch((error: unknown) => failed("The saved copy could not be discarded", error))
						.finally(() => setResuming(false));
				}}
			>
				Discard saved copy
			</button>
		</div>
	);
}

function DraftDetails({
	machine,
	project,
	workspace,
	branch,
	servers,
}: {
	readonly machine: string;
	readonly project: ReactNode;
	readonly workspace: ReactNode;
	readonly branch: ReactNode;
	readonly servers: ReactNode;
}): ReactNode {
	return (
		<div className="divide-y divide-border text-ui">
			<section className="p-1.5">
				<DetailRow icon={<Laptop />} label={machine} />
				<div className="flex min-w-0 items-center gap-2 px-2 py-2">
					<Folder className="size-4 shrink-0 text-muted-foreground" />
					{project}
				</div>
				{workspace}
			</section>
			<section className="flex flex-col items-start gap-1 p-1.5">
				{branch}
				{servers}
			</section>
		</div>
	);
}

function DraftHeadline({
	folder,
	project,
	onNone,
}: {
	readonly folder: string | undefined;
	readonly project: ReactNode;
	readonly onNone: () => void;
}): ReactNode {
	return (
		<div className="mb-6 text-center">
			<h1 className="flex flex-wrap items-baseline justify-center gap-x-2 text-title font-normal tracking-tight">
				{folder === undefined ? (
					<span>What should we work on?</span>
				) : (
					<>
						<span>What should we build in</span>
						<span className="max-w-full [&>button]:font-semibold [&>button]:text-muted-foreground [&>button]:underline [&>button]:decoration-border [&>button]:underline-offset-4 [&>button]:transition-colors [&>button:hover]:text-primary [&>button:hover]:decoration-primary">
							{project}
						</span>
						<span>?</span>
					</>
				)}
			</h1>
			{folder === undefined ? (
				<div className="mt-2 text-xs text-muted-foreground">{project}</div>
			) : (
				<button type="button" className="mt-2 text-xs text-muted-foreground hover:text-foreground" onClick={onNone}>
					or start without a project
				</button>
			)}
		</div>
	);
}
function draftConfig(
	folder: string | undefined,
	model: string | undefined,
	options: {
		readonly mode?: import("@octoii-works/octoii-client").Mode | undefined;
		readonly effort?: import("@octoii-works/octoii-client").ReasoningEffort | undefined;
	},
): import("@octoii-works/octoii-client").Config {
	return {
		...(folder === undefined ? {} : { workspace: folder }),
		...(model === undefined ? {} : { model }),
		...(options.mode === undefined ? {} : { mode: options.mode }),
		...(options.effort === undefined ? {} : { effort: options.effort }),
	};
}

function useDraftBranches(folder: string | undefined) {
	const git = useExtension("git");
	const connection = useConnection();
	const allowed =
		git &&
		connection.status === "open" &&
		["files.read", "git.operate", "threads.operate"].every((scope) => connection.scopes.some((value) => value === scope));
	const repository = useOutcome("git.repository", allowed && folder !== undefined ? { path: folder } : undefined);
	const branches = useOutcome(
		"git.branches",
		repository !== undefined && "result" in repository && repository.result.repository !== undefined && folder !== undefined ? { path: folder } : undefined,
	);
	return branches;
}

function useDraftSubmit(
	saved: ReturnType<typeof usePendingThread>,
	folder: string | undefined,
	model: string | undefined,
	options: ReturnType<typeof useNewThreadOptions>["value"],
	setSending: (sending: boolean) => void,
) {
	const client = useClient();
	return async (draft: Draft): Promise<void> => {
		const starting = saved.starting;
		if (starting === undefined) throw new Error("Wait for the node connection.");
		setSending(true);
		try {
			const config = draftConfig(folder, model, options);
			const selected = options.worktree;
			if (selected !== undefined && selected.path !== folder) throw new Error("Choose a base branch for this project.");
			const thread =
				selected === undefined
					? await starting.send(config, draft)
					: await starting.sendWorktree(
							config,
							draft,
							await client.call("git.worktree-plan", { path: selected.path, base: selected.base, key: crypto.randomUUID() }),
							(thread) => prepareWorktree(client, thread, () => go({ place: "thread", thread })),
						);
			if (selected === undefined) go({ place: "thread", thread });
			await starting.acknowledge();
		} catch (error) {
			saved.setPending(await starting.read());
			throw error;
		} finally {
			setSending(false);
		}
	};
}
