/** Recovery of an accepted worktree task. Reading or reconnecting never starts Git. */
import { type ReactNode, useState } from "react";
import type { Listed, Start, ThreadStart, WorktreeEntry } from "@octoii-works/octoii-client";
import { useClient, useConnection } from "@octoii-works/octoii-client/react";
import { useWorktreePreparing } from "../../components/state/worktree-preparation.ts";
import { LoaderCircle } from "lucide-react";
import { failureOf } from "../../components/effects/read.ts";

export function WorktreeStart(props: { readonly thread: string; readonly start: Start; readonly inbox: Listed["status"]["inbox"] }): ReactNode {
	const client = useClient();
	const connection = useConnection();
	const preparing = useWorktreePreparing(client, props.thread);
	const [result, setResult] = useState<ThreadStart | undefined>(undefined);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | undefined>(undefined);
	const start = result?.start ?? props.start;
	const allowed = connection.status === "open" && connection.scopes.includes("git.operate") && connection.scopes.includes("threads.operate");
	const run = async (action: "inspect" | "prepare" | "retry"): Promise<void> => {
		if (busy || !allowed) return;
		setBusy(true);
		setError(undefined);
		try {
			const inspected = await client.call("git.start", { action: "inspect", thread: props.thread });
			setResult(inspected);
			if (action !== "inspect" && (inspected.start.state.state === "pending" || inspected.start.state.state === "failed"))
				setResult(
					await client.call("git.start", {
						action: inspected.start.state.state === "failed" ? "retry" : "prepare",
						thread: props.thread,
						token: inspected.start.token,
					}),
				);
		} catch (failure) {
			setError(failureOf(failure));
			setResult(undefined); // An uncertain write must be inspected again before another attempt.
		} finally {
			setBusy(false);
		}
	};
	if (preparing && start.state.state === "pending")
		return (
			<section aria-label="Worktree preparation" className="chat-column py-4">
				<div className="mb-4 whitespace-pre-wrap text-body">{savedInputs(props.inbox).join("\n\n")}</div>
				<div className="flex items-center gap-2 text-ui text-muted-foreground">
					<LoaderCircle className="size-4 animate-spin" />
					<span className="flex-1">Preparing worktree…</span>
					<button type="button" disabled={!allowed} onClick={() => void client.stop(props.thread).catch((failure: unknown) => setError(failureOf(failure)))}>
						Cancel task
					</button>
				</div>
				{error === undefined ? null : (
					<p role="alert" className="mt-2 text-xs text-error-foreground">
						{error}
					</p>
				)}
			</section>
		);
	return (
		<PreparationCard
			start={start}
			inputs={savedInputs(props.inbox)}
			worktrees={result?.worktrees}
			busy={busy}
			allowed={allowed}
			error={error}
			onInspect={() => void run("inspect")}
			onContinue={() => void run(start.state.state === "failed" ? "retry" : "prepare")}
			onCancel={() => {
				if (!allowed) return;
				void client.stop(props.thread).catch((failure: unknown) => setError(failureOf(failure)));
			}}
		/>
	);
}

/** The card stays small; detailed worktree paths are disclosed only when requested. */
export function PreparationCard(props: {
	readonly start: Start;
	readonly inputs: readonly string[];
	readonly worktrees: readonly WorktreeEntry[] | undefined;
	readonly busy: boolean;
	readonly allowed: boolean;
	readonly error: string | undefined;
	readonly onInspect: () => void;
	readonly onContinue: () => void;
	readonly onCancel: () => void;
}): ReactNode {
	const state = props.start.state;
	const cancelled = state.state === "cancelled";
	const ready = state.state === "ready";
	const payload = props.start.request.payload;
	const request = payload !== null && typeof payload === "object" && !Array.isArray(payload) ? payload : {};

	if (ready)
		return (
			<details className="chat-column py-2 text-xs text-muted-foreground">
				<summary className="cursor-pointer">Workspace ready</summary>
				<p className="mt-2 break-all">{String(request["destination"] ?? "")}</p>
			</details>
		);
	return (
		<section aria-label="Worktree preparation" className="chat-column py-4">
			<div className="rounded-lg border border-border bg-background p-4 text-body">
				<h2 className="font-medium">
					{cancelled
						? "Preparation cancelled"
						: ready
							? "Workspace ready"
							: state.state === "failed"
								? "Workspace preparation failed"
								: "Waiting for workspace preparation"}
				</h2>
				{typeof request["destination"] === "string" ? (
					<p className="mt-2 break-all text-xs text-muted-foreground">
						{request["destination"]}
						{typeof request["branch"] === "string" ? ` · ${request["branch"]}` : ""}
					</p>
				) : null}
				{props.inputs.length > 0 ? <div className="my-3 max-h-40 overflow-auto whitespace-pre-wrap break-words">{props.inputs.join("\n\n")}</div> : null}
				<p className="mt-2 text-muted-foreground">
					{ready
						? "The saved task can now run in its workspace."
						: cancelled
							? "This task will not run. Existing files have been left in place."
							: "Your message is saved. Continue preparation to check the workspace and resume this task."}
				</p>
				{state.state === "failed" ? <p className="mt-3 break-words text-error-foreground">{state.reason}</p> : null}
				{props.error === undefined ? null : (
					<p role="alert" className="mt-3 break-words text-error-foreground">
						{props.error}
					</p>
				)}
				{props.worktrees === undefined ? null : (
					<details className="mt-3 text-muted-foreground">
						<summary className="cursor-pointer">Registered worktrees ({props.worktrees.length})</summary>
						<ul className="mt-2 max-h-48 space-y-2 overflow-auto">
							{props.worktrees.map((tree) => (
								<li key={tree.path} className="break-all">
									<span className="text-foreground">{tree.branch ?? "Detached HEAD"}</span>
									<br />
									{tree.path}
								</li>
							))}
						</ul>
						<p className="mt-2">Existing worktrees are never adopted or removed automatically. Resolve conflicts before retrying.</p>
					</details>
				)}
				{ready || cancelled ? null : (
					<div className="mt-4 flex flex-wrap gap-3">
						<button
							type="button"
							disabled={props.busy || !props.allowed}
							onClick={props.onInspect}
							className="rounded-md border border-border px-3 py-2 disabled:opacity-50"
						>
							Check worktrees
						</button>
						<button
							type="button"
							disabled={props.busy || !props.allowed}
							onClick={props.onContinue}
							className="rounded-md bg-primary px-3 py-2 text-primary-foreground disabled:opacity-50"
						>
							{props.busy ? "Please wait…" : state.state === "failed" ? "Retry preparation" : "Continue preparation"}
						</button>
						<button type="button" disabled={!props.allowed} onClick={props.onCancel} className="px-2 py-2 text-muted-foreground disabled:opacity-50">
							Cancel task
						</button>
					</div>
				)}
				{!props.allowed && !ready && !cancelled ? (
					<p className="mt-3 text-xs text-muted-foreground">Connect with Git and thread operation permissions to continue.</p>
				) : null}
			</div>
		</section>
	);
}

function savedInputs(inbox: Listed["status"]["inbox"]): string[] {
	return inbox.flatMap(([, input]) =>
		input.type === "input"
			? input.content.flatMap((part) =>
					part.type === "input_text" ? [part.text] : part.type === "input_file" || part.type === "input_image" ? [part.file.filename ?? "Attachment"] : [],
				)
			: [],
	);
}
