/** A task draft in the current project's route. Sending hands its first message to the node. */
import { type ReactNode, useState } from "react";
import type { Config } from "@octoii-works/octoii-client";
import { useConnection, useSettings, useThreads } from "@octoii-works/octoii-client/react";
import type { Device } from "../../core/device.ts";
import type { Draft } from "../../core/drafts.ts";
import { thingsOf } from "../../core/mentions.ts";
import { recentModelsOf } from "../../core/models.ts";
import { routeOf } from "../../core/route.ts";
import { projectsOf } from "../../core/threads.ts";
import { Composer } from "../../components/composer/composer.tsx";
import { FolderChooser } from "../../components/files/folder-chooser.tsx";
import { Pane } from "../../components/frame/frame.tsx";
import { clearPendingDraft } from "../../components/state/draft.ts";
import { useExtension } from "../../components/state/extensions.ts";
import { go } from "../../components/state/navigation.ts";
import { failed } from "../../components/state/notices.ts";
import { ChatHeader } from "../../components/thread/header.tsx";
import { SeparateDirectory } from "../../extensions/git/new-worktree.tsx";
import { ServersMenu } from "../../extensions/mcp/servers-menu.tsx";
import { DraftProject } from "./draft-project.tsx";
import { usePendingThread } from "./effects/pending-thread.ts";
import { useNewThreadOptions } from "./options.ts";
import { DraftLoading, SavedTask } from "./saved-task.tsx";
import type { ThreadWorkspace } from "./request.ts";

export function NewThreadScreen({ project, device }: { readonly project: string | undefined; readonly device: Device }): ReactNode {
	const connection = useConnection();
	const saved = usePendingThread(project);
	const { value: options, change } = useNewThreadOptions(saved.session, project);
	const [choosing, setChoosing] = useState(false);
	const [sending, setSending] = useState(false);
	const rows = useThreads() ?? [];
	const reachesThreads = useExtension("thread");
	const mcp = useExtension("mcp");
	const git = useExtension("git");
	const settings = useSettings();
	const model = options.model ?? recentModelsOf(rows)[0];
	const mode = options.mode ?? settings?.environment.thread_mode;
	const { worktree } = options;
	const canIsolate =
		git &&
		connection.status === "open" &&
		["files.read", "git.operate", "threads.operate"].every((scope) => connection.scopes.some((granted) => granted === scope));
	const locked = sending || saved.pending !== undefined;
	const choose = (next: string | undefined): void => {
		setChoosing(false);
		if (next !== project) go(next === undefined ? { place: "new" } : { place: "new", project: next });
	};
	const directoryAction =
		worktree !== undefined
			? { label: "Use project directory", select: () => change({ worktree: undefined }) }
			: canIsolate && project !== undefined
				? { label: "Work in a separate directory", repository: project, select: () => change({ worktree: {} }) }
				: undefined;
	const picker = (
		<DraftProject
			folder={project}
			projects={projectsOf(rows)}
			onChoose={choose}
			onAnother={() => setChoosing(true)}
			directoryAction={directoryAction}
			disabled={locked}
		/>
	);

	const starting = saved.starting;
	async function send(draft: Draft): Promise<void> {
		if (starting === undefined) throw new Error("Wait for the node connection.");
		const config: Config = {
			...(project === undefined ? {} : { workspace: project }),
			...(model === undefined ? {} : { model }),
			...(options.mode === undefined ? {} : { mode: options.mode }),
			...(options.effort === undefined ? {} : { effort: options.effort }),
		};
		const base = worktree?.base?.commit;
		const workspace: ThreadWorkspace =
			worktree === undefined || project === undefined ? { kind: "current" } : { kind: "isolated", path: project, ...(base === undefined ? {} : { base }) };
		setSending(true);
		try {
			const { key, thread } = await starting.send(config, workspace, draft);
			openAccepted(project, thread);
			await starting.clear(key).catch((error: unknown) => failed("The saved copy could not be cleared", error));
		} catch (error) {
			saved.retry();
			throw error;
		} finally {
			setSending(false);
		}
	}
	return (
		<Pane>
			<ChatHeader title="New thread" />
			<div className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-4 py-8">
				<div className="w-full max-w-(--chat-width)">
					<DraftHeadline project={project}>{picker}</DraftHeadline>
					{!saved.loaded ? (
						<DraftLoading error={saved.error} retry={saved.retry} />
					) : saved.pending !== undefined && starting !== undefined ? (
						<SavedTask
							pending={saved.pending}
							starting={starting}
							refresh={saved.retry}
							onAccepted={(pending, thread) => {
								clearPendingDraft({ project: pending.project }, pending.draft);
								openAccepted(project, thread);
							}}
						/>
					) : (
						<>
							<Composer
								model={model}
								onModel={(model) => change({ model })}
								mode={mode}
								onMode={(mode) => change({ mode })}
								effort={options.effort}
								onEffort={(effort) => change({ effort })}
								working={false}
								sendShortcut={device.send_shortcut}
								draft={{ project }}
								folder={project}
								things={thingsOf(reachesThreads ? rows : [], [], [], undefined)}
								onSend={send}
							/>
							<fieldset disabled={sending} className="mt-1 flex min-w-0 flex-wrap items-center justify-end gap-1 border-0 p-0 empty:hidden">
								{worktree === undefined || project === undefined ? null : (
									<SeparateDirectory
										path={project}
										base={worktree.base}
										disabled={sending}
										onBaseChange={canIsolate ? (base) => change({ worktree: { base } }) : undefined}
									/>
								)}
								{mcp ? <ServersMenu folder={project} /> : null}
							</fieldset>
						</>
					)}
				</div>
			</div>
			<FolderChooser open={choosing} onOpenChange={setChoosing} start={project} onChoose={choose} />
		</Pane>
	);
}

function DraftHeadline({ project, children }: { readonly project: string | undefined; readonly children: ReactNode }): ReactNode {
	return (
		<div className="mb-6 text-center">
			{project === undefined ? (
				<>
					<h1 className="text-title tracking-tight">What should we work on?</h1>
					<div className="mt-2 text-ui text-muted-foreground">{children}</div>
				</>
			) : (
				<h1 className="flex flex-wrap items-baseline justify-center gap-x-2 text-title tracking-tight">
					<span>What should we build in</span>
					<span className="max-w-full [&>button]:font-semibold [&>button]:text-muted-foreground [&>button]:underline [&>button]:decoration-border [&>button]:underline-offset-4 [&>button]:transition-colors [&>button:hover]:text-primary [&>button:hover]:decoration-primary">
						{children}
					</span>
					<span>?</span>
				</h1>
			)}
		</div>
	);
}

/** A slow acceptance opens its Thread only while the person is still looking at the same draft. */
function openAccepted(project: string | undefined, thread: string): void {
	const current = routeOf(window.location.hash);
	if (current.place === "new" && current.project === project) go({ place: "thread", thread });
}
