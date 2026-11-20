/** A local draft: project headline, centered composer, shared context controls and floating details. */
import { type ReactNode, useState } from "react";
import { useClient, useConnection, useSettings, useThreads } from "@octoii-works/octoii-client/react";
import { Folder, Laptop } from "lucide-react";
import type { Device } from "../../core/device.ts";
import { recentModelsOf } from "../../core/models.ts";
import type { NewThread, PendingThread } from "./submission.ts";
import type { ThreadWorkspace } from "./request.ts";
import { usePendingThread } from "./effects/pending-thread.ts";
import { failed } from "../../components/state/notices.ts";
import { clearPendingDraft } from "../../components/state/draft.ts";
import { thingsOf } from "../../core/mentions.ts";
import { useExtension } from "../../components/state/extensions.ts";
import { folderName, projectsOf } from "../../core/threads.ts";
import { Composer } from "../../components/composer/composer.tsx";
import { ModeControl } from "../../components/composer/mode-picker.tsx";
import { FolderChooser } from "../../components/files/folder-chooser.tsx";
import { Pane } from "../../components/frame/frame.tsx";
import { go } from "../../components/state/navigation.ts";
import { ChatHeader } from "../../components/thread/header.tsx";
import { DraftProject } from "./draft-project.tsx";
import { DetailsToggle, DetailsFloat, DetailRow } from "../../components/thread/details.tsx";
import { useNewThreadOptions } from "./options.ts";
import { planWorktree, prepareWorktree } from "../../extensions/git/prepare.ts";
import { SeparateDirectory } from "../../extensions/git/new-worktree.tsx";
import { ServersMenu } from "../../extensions/mcp/servers-menu.tsx";
import type { Draft } from "../../core/drafts.ts";

export type NewThreadScreenProps = { readonly project: string | undefined; readonly device: Device };

export function NewThreadScreen(props: NewThreadScreenProps): ReactNode {
	const connection = useConnection();
	const saved = usePendingThread(props.project);
	const { value: options, change } = useNewThreadOptions(connection.status === "open" ? connection.session : undefined, props.project);
	const [choosing, setChoosing] = useState(false);
	const [sending, setSending] = useState(false);
	const rows = useThreads() ?? [];
	const reachesThreads = useExtension("thread");
	const mcp = useExtension("mcp");
	const git = useExtension("git");
	const settings = useSettings();
	const folder = options.chosen === undefined ? props.project : options.chosen.folder;
	const model = options.model ?? recentModelsOf(rows)[0];
	const mode = options.mode ?? settings?.environment.thread_mode;
	const projects = projectsOf(rows);
	const worktree = options.worktree;
	const worktreeAllowed =
		git &&
		connection.status === "open" &&
		["files.read", "git.operate", "threads.operate"].every((scope) => connection.scopes.some((value) => value === scope));
	const locked = sending || saved.pending !== undefined;
	const choose = (next: string | undefined): void => {
		if (next !== folder) change({ chosen: { folder: next }, worktree: undefined });
		setChoosing(false);
	};
	const directoryAction =
		worktree !== undefined
			? { label: "Use project directory", select: () => change({ worktree: undefined }) }
			: worktreeAllowed && folder !== undefined
				? { label: "Work in a separate directory", select: () => change({ worktree: { path: folder } }) }
				: undefined;
	const projectPicker = (
		<DraftProject
			folder={folder}
			projects={projects}
			onChoose={choose}
			onAnother={() => setChoosing(true)}
			directoryAction={directoryAction}
			disabled={locked}
		/>
	);
	const details = (
		<DraftDetails
			machine={connection.status === "open" ? connection.machine.name : "This machine"}
			project={projectPicker}
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
						<fieldset disabled={locked}>{details}</fieldset>
					</DetailsToggle>
				}
			/>
			<div className="flex min-h-0 flex-1">
				<div className="flex min-w-0 flex-1 items-center justify-center overflow-y-auto px-4 py-8">
					<div className="w-full max-w-(--chat-width)">
						<fieldset disabled={locked} className="min-w-0 border-0 p-0">
							<DraftHeadline folder={folder} project={projectPicker} onNone={() => choose(undefined)} />
						</fieldset>
						{!saved.loaded ? (
							<LoadingDraft error={saved.error} retry={saved.retry} />
						) : saved.pending !== undefined ? (
							<PendingTask pending={saved.pending} starting={saved.starting} refresh={saved.retry} />
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
								{worktree !== undefined || mode !== undefined ? (
									<fieldset disabled={sending} className="mt-1 flex min-w-0 flex-wrap items-center gap-1 border-0 p-0">
										{worktree === undefined ? null : (
											<SeparateDirectory
												worktree={worktree}
												disabled={sending}
												onBaseChange={worktreeAllowed ? (base) => change({ worktree: { ...worktree, base } }) : undefined}
											/>
										)}
										{mode === undefined ? null : (
											<div className="ms-auto">
												<ModeControl mode={mode} onChoose={(mode) => change({ mode })} />
											</div>
										)}
									</fieldset>
								) : null}
							</>
						)}
					</div>
				</div>
				<fieldset disabled={locked} className="contents">
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
	refresh,
}: {
	readonly pending: PendingThread;
	readonly starting: NewThread | undefined;
	readonly refresh: () => void;
}): ReactNode {
	const [resuming, setResuming] = useState(false);
	const worktree = pending.workspace.kind === "isolated" ? pending.workspace.worktree : undefined;
	return (
		<div className="rounded-lg border border-border p-4 text-body">
			<p className="font-medium">Unfinished submission</p>
			<p className="my-3 max-h-48 overflow-auto whitespace-pre-wrap">{pending.draft.text}</p>
			<p className="text-muted-foreground">
				{pending.project ?? "No project"} · {pending.draft.attachments.length} attachments
			</p>
			{worktree === undefined ? null : (
				<p className="mt-2 break-all text-muted-foreground">
					Separate directory · {worktree.branch} · {worktree.destination}
				</p>
			)}
			<p className="my-3 text-muted-foreground">
				{pending.thread === undefined ? "Continue with the original message and attachments." : "The node accepted this task."}
			</p>
			<button
				type="button"
				disabled={resuming}
				className="rounded-md bg-primary px-3 py-2 text-primary-foreground disabled:opacity-50"
				onClick={() => {
					if (starting === undefined || resuming) return;
					setResuming(true);
					void starting
						.resume(pending.key)
						.then(async ({ key, thread }) => {
							clearPendingDraft({ project: pending.project }, pending.draft);
							go({ place: "thread", thread });
							await starting.clear(key).catch((error: unknown) => failed("The saved copy could not be cleared", error));
						})
						.catch((error: unknown) => {
							failed("The task could not be resumed", error);
							refresh();
						})
						.finally(() => setResuming(false));
				}}
			>
				{resuming ? "Continuing…" : pending.thread === undefined ? "Continue task" : "Open thread"}
			</button>
			<p className="mt-3 text-xs text-muted-foreground">Discarding this local copy leaves any accepted thread on the node.</p>
			<button
				type="button"
				disabled={resuming}
				className="mt-2 text-xs text-muted-foreground underline"
				onClick={() => {
					if (starting === undefined || resuming) return;
					setResuming(true);
					void starting
						.clear(pending.key)
						.then(refresh)
						.catch((error: unknown) => failed("The saved copy could not be discarded", error))
						.finally(() => setResuming(false));
				}}
			>
				Discard saved copy
			</button>
		</div>
	);
}

function DraftDetails({ machine, project, servers }: { readonly machine: string; readonly project: ReactNode; readonly servers: ReactNode }): ReactNode {
	return (
		<div className="divide-y divide-border text-ui">
			<section className="p-1.5">
				<DetailRow icon={<Laptop />} label={machine} />
				<div className="flex min-w-0 items-center gap-2 px-2 py-2">
					<Folder className="size-4 shrink-0 text-muted-foreground" />
					{project}
				</div>
			</section>
			{servers === null ? null : <section className="flex flex-col items-start gap-1 p-1.5">{servers}</section>}
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
			if (selected !== undefined && selected.path !== folder) throw new Error("Choose the work directory for this project.");
			const workspace: ThreadWorkspace = selected === undefined ? { kind: "current" } : { kind: "isolated", worktree: await planWorktree(client, selected) };
			const { key, thread } = await starting.send(config, workspace, draft);
			const open = (): void => go({ place: "thread", thread });
			if (workspace.kind === "isolated") {
				await prepareWorktree(client, thread, open).catch((error: unknown) => failed("The separate directory could not be prepared", error));
			} else open();
			await starting.clear(key).catch((error: unknown) => failed("The saved copy could not be cleared", error));
		} catch (error) {
			saved.retry();
			throw error;
		} finally {
			setSending(false);
		}
	};
}
