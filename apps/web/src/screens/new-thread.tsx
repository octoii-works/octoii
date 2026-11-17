/**
 * A new thread: "What should we build in <project>?" over the composer, in the middle of the page, and under the
 * composer, joined to it, the folder the thread will work in — none, or the project the page was opened for, till the
 * person chooses from a menu, opening upward, of the folders threads work in, none, or another chosen as a file manager
 * chooses one — and on its right the MCP servers a thread there would use.
 * Sending makes the thread there, with the model chosen — the one the newest thread chose where none is — the mode
 * chosen, else the one the environment's settings name, and the message's first line as its title, delivers the
 * message, and goes to the thread.
 */
import { type ReactNode, useState } from "react";
import type { ReasoningEffort as Effort, Mode } from "@octoii-works/octoii-client";
import { useClient, useSettings, useThreads } from "@octoii-works/octoii-client/react";
import { ChevronDown, FolderOpen, FolderPlus, FolderX } from "lucide-react";
import type { Device } from "../core/device.ts";
import { recentModelsOf } from "../core/models.ts";
import type { NewThread, PendingThread } from "../core/new-thread.ts";
import { usePendingThread } from "../components/effects/pending-thread.ts";
import { failed } from "../components/state/notices.ts";
import { clearPendingDraft } from "../components/state/draft.ts";
import { thingsOf } from "../core/mentions.ts";
import { useExtension } from "../components/state/extensions.ts";
import { folderName, type Project, projectsOf } from "../core/threads.ts";
import { Composer } from "../components/composer/composer.tsx";
import { FolderChooser } from "../components/files/folder-chooser.tsx";
import { Pane } from "../components/frame/frame.tsx";
import { go } from "../components/state/navigation.ts";
import { ChatHeader } from "../components/thread/header.tsx";
import { Menu, MenuItem, MenuItemLabel, MenuPopup, MenuRadioGroup, MenuRadioItem, MenuSeparator, MenuTrigger } from "../components/ui/menu.tsx";
import { ProjectMark } from "../components/files/project-mark.tsx";
import { ServersMenu } from "../extensions/mcp/servers-menu.tsx";

/** What the new thread's page is given: the project's folder asked for, and the device's settings. */
export type NewThreadScreenProps = {
	readonly project: string | undefined;
	readonly device: Device;
};

/** The folder the person chose to work in, or that they chose none. */
type Chosen = { readonly folder: string | undefined };

/** The new thread's page. */
export function NewThreadScreen(props: NewThreadScreenProps): ReactNode {
	const client = useClient();
	const { starting, pending, loaded, setPending, error, retry } = usePendingThread(client, props.project);
	const rows = useThreads() ?? [];
	const reachesThreads = useExtension("thread");
	const projects = projectsOf(rows);
	const [chosen, setChosen] = useState<Chosen | undefined>(undefined);
	const [choosing, setChoosing] = useState(false);
	const [model, setModel] = useState<string | undefined>(undefined);
	const [chosenMode, setMode] = useState<Mode | undefined>(undefined);
	const [effort, setEffort] = useState<Effort | undefined>(undefined);
	const settings = useSettings();
	const mode = chosenMode ?? settings?.environment.thread_mode;
	// None till the person chooses one, unless the page was opened for a project: a thread of no folder is no project's.
	const folder = chosen === undefined ? props.project : chosen.folder;
	const running = model ?? recentModelsOf(rows)[0];
	const choose = (next: string | undefined): void => {
		setChosen({ folder: next });
		setChoosing(false);
	};
	return (
		<Pane>
			<ChatHeader title="New thread" {...(folder === undefined ? {} : { project: { folder, name: folderName(folder) } })} />
			<div className="flex flex-1 items-center justify-center px-4 pb-[12vh]">
				<div className="w-full max-w-[var(--chat-width)]">
					<h1 className="mb-6 text-center text-title font-normal tracking-tight">
						{folder === undefined ? "What should we work on?" : `What should we build in ${folderName(folder)}?`}
					</h1>
					{/* The composer stands over the folder's strip, which shows from under its foot. */}
					<div className="relative z-10">
						{!loaded ? (
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
						) : pending !== undefined ? (
							<PendingTask pending={pending} starting={starting} onForgotten={() => setPending(undefined)} />
						) : (
							<Composer
								model={running}
								onModel={setModel}
								mode={mode}
								onMode={setMode}
								effort={effort}
								onEffort={setEffort}
								working={false}
								sendShortcut={props.device.send_shortcut}
								draft={{ project: folder }}
								folder={folder}
								things={thingsOf(reachesThreads ? rows : [], [], [], undefined)}
								onSend={async (draft) => {
									if (starting === undefined) throw new Error("Wait for the node connection.");
									try {
										const thread = await starting.send(
											{
												...(folder === undefined ? {} : { workspace: folder }),
												...(running === undefined ? {} : { model: running }),
												...(chosenMode === undefined ? {} : { mode: chosenMode }),
												...(effort === undefined ? {} : { effort }),
											},
											draft,
										);
										go({ place: "thread", thread });
										await starting.acknowledge();
									} catch (error) {
										setPending(await starting.read());
										throw error;
									}
								}}
								onStop={() => undefined}
							/>
						)}
					</div>
					<FolderStrip folder={folder} projects={projects} onChoose={choose} onAnother={() => setChoosing(true)} />
					<FolderChooser open={choosing} onOpenChange={setChoosing} start={folder} projects={projects} onChoose={choose} />
				</div>
			</div>
		</Pane>
	);
}

/**
 * Under the composer, joined to it: the folder the thread will work in, by its name, and a menu of the folders threads
 * work in to choose instead — the one chosen checked — none, and another folder; on its right, the servers. Both open
 * upward, as what stands under the composer has the page's room above it.
 */
function FolderStrip(props: {
	readonly folder: string | undefined;
	readonly projects: readonly Project[];
	readonly onChoose: (folder: string | undefined) => void;
	readonly onAnother: () => void;
}): ReactNode {
	const mcp = useExtension("mcp");
	return (
		<div className="-mt-5 flex items-center justify-between gap-2 rounded-b-lg border border-t-0 border-border bg-muted px-2 pt-6 pb-1.5">
			<Menu>
				<MenuTrigger
					title={props.folder}
					className="inline-flex h-control-sm max-w-full items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring data-popup-open:bg-accent data-popup-open:text-foreground"
				>
					<FolderOpen className="size-3.5 shrink-0" />
					<span className="truncate">{props.folder === undefined ? "No project" : folderName(props.folder)}</span>
					<ChevronDown className="size-3.5 shrink-0 opacity-70" />
				</MenuTrigger>
				<MenuPopup side="top" align="start" sideOffset={8} className="w-64">
					<MenuRadioGroup value={props.folder ?? ""} onValueChange={(folder: string) => props.onChoose(folder === "" ? undefined : folder)}>
						{props.projects.map((project) => (
							<MenuRadioItem key={project.folder} value={project.folder}>
								<ProjectMark project={project} />
								<MenuItemLabel title={project.folder}>{project.name}</MenuItemLabel>
							</MenuRadioItem>
						))}
						<MenuRadioItem value="">
							<FolderX />
							<MenuItemLabel>No project</MenuItemLabel>
						</MenuRadioItem>
					</MenuRadioGroup>
					<MenuSeparator />
					<MenuItem onClick={props.onAnother}>
						<FolderPlus />
						<MenuItemLabel>Choose a folder…</MenuItemLabel>
					</MenuItem>
				</MenuPopup>
			</Menu>
			{mcp ? <ServersMenu folder={props.folder} /> : null}
		</div>
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
				Recovery is available for 23 hours. Discarding this saved copy does not stop or delete an existing thread.
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
