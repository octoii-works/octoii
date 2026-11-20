/** The node's durable worktree start state and the actions a person can take on its current attempt. */
import { type ReactNode, useState, useTransition } from "react";
import type { Start, StartState, Status } from "@octoii-works/octoii-client";
import { useClient, useConnection } from "@octoii-works/octoii-client/react";
import { LoaderCircle } from "lucide-react";
import { isPreparingStart, stopTargetOf, THREAD_STATUS_LABELS } from "../../core/threads.ts";
import { failureOf } from "../../components/effects/read.ts";
import { failed } from "../../components/state/notices.ts";
import { Button } from "../../components/ui/button.tsx";

const START_TITLES: Readonly<Record<StartState["state"], string>> = {
	pending: "Waiting to prepare the worktree…",
	preparing: "Preparing worktree…",
	checking: "Checking worktree…",
	failed: "Workspace preparation failed",
	interrupted: "Workspace preparation interrupted",
	cancelled: "Preparation cancelled",
	ready: "Workspace ready",
	unavailable: THREAD_STATUS_LABELS.unavailable,
};

export function WorktreeStart({ thread, status }: { readonly thread: string; readonly status: Status }): ReactNode {
	const start = status.start;
	if (start?.request.owner !== "git.worktree") return null;
	const { state } = start;
	const title = START_TITLES[state.state];
	if (state.state === "ready")
		return (
			<details className="chat-column py-2 text-xs text-muted-foreground">
				<summary className="cursor-pointer">{title}</summary>
				<p className="mt-2 break-all">{state.directory.workspace}</p>
			</details>
		);
	const { payload } = start.request;
	const request = payload !== null && typeof payload === "object" && !Array.isArray(payload) ? payload : {};
	const unavailable = state.state === "unavailable";
	const directory = unavailable ? state.directory.workspace : request["destination"];
	const stop = unavailable ? stopTargetOf(status) : undefined;
	const inputs = savedInputs(status.inbox);
	return (
		<section aria-label={unavailable ? "Workspace status" : "Worktree preparation"} className="chat-column py-4">
			<div className="rounded-lg border border-border bg-background p-4 text-body">
				<h2 aria-live="polite" className="flex items-center gap-2 font-medium">
					{isPreparingStart(state) ? <LoaderCircle aria-hidden className="size-4 animate-spin motion-reduce:animate-none" /> : null}
					{title}
				</h2>
				{typeof directory === "string" ? (
					<p className="mt-2 break-all text-xs text-muted-foreground">
						{directory}
						{typeof request["branch"] === "string" ? ` · ${request["branch"]}` : ""}
					</p>
				) : null}
				{inputs.length > 0 ? <div className="my-3 max-h-40 overflow-auto whitespace-pre-wrap break-words">{inputs.join("\n\n")}</div> : null}
				{state.state === "failed" || state.state === "interrupted" ? <p className="mt-3 break-words text-error-foreground">{state.reason}</p> : null}
				{unavailable ? <p className="mt-3 text-xs text-muted-foreground">Restore the original worktree at this path to continue.</p> : null}
				{stop?.kind === "turn" ? <StopTurn key={stop.id} thread={thread} turn={stop.id} /> : null}
				{unavailable || state.state === "cancelled" ? null : <PreparationActions key={`${thread}:${start.token}`} thread={thread} start={start} />}
			</div>
		</section>
	);
}

function StopTurn({ thread, turn }: { readonly thread: string; readonly turn: string }): ReactNode {
	const client = useClient();
	const connection = useConnection();
	const [stopping, startStopping] = useTransition();
	const canStop = connection.status === "open" && connection.scopes.includes("threads.operate");
	return (
		<Button
			size="sm"
			variant="ghost-muted"
			className="mt-4"
			disabled={stopping || !canStop}
			onClick={() => {
				startStopping(async () => {
					await client.stop(thread, { kind: "turn", id: turn }).catch((error: unknown) => failed("The turn could not be stopped", error));
				});
			}}
		>
			Stop turn
		</Button>
	);
}

/** Local feedback for actions addressed to the displayed token. The subscription supplies all preparation state. */
function PreparationActions({ thread, start }: { readonly thread: string; readonly start: Start }): ReactNode {
	const client = useClient();
	const connection = useConnection();
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string>();
	const canCancel = connection.status === "open" && connection.scopes.includes("threads.operate");
	const canRetry = canCancel && connection.scopes.includes("git.operate");
	const retry = start.state.state === "failed";
	const unavailable =
		connection.status !== "open"
			? "Reconnect to manage this task."
			: !canCancel
				? "Thread operation permission is required to manage this task."
				: retry && !canRetry
					? "Git operation permission is required to retry preparation."
					: undefined;
	async function perform<Result>(action: () => Promise<Result>): Promise<void> {
		if (busy) return;
		setBusy(true);
		setError(undefined);
		try {
			await action();
		} catch (failure) {
			setError(failureOf(failure));
		} finally {
			setBusy(false);
		}
	}
	return (
		<>
			<div className="mt-4 flex flex-wrap gap-3">
				{retry ? (
					<Button size="sm" disabled={busy || !canRetry} onClick={() => void perform(() => client.call("git.retry-start", { thread, token: start.token }))}>
						Retry preparation
					</Button>
				) : null}
				<Button
					size="sm"
					variant="ghost-muted"
					disabled={busy || !canCancel}
					onClick={() => void perform(() => client.stop(thread, { kind: "preparation", token: start.token }))}
				>
					Cancel task
				</Button>
			</div>
			{unavailable === undefined ? null : <p className="mt-3 text-xs text-muted-foreground">{unavailable}</p>}
			{error === undefined ? null : (
				<p role="alert" className="mt-3 break-words text-error-foreground">
					{error}
				</p>
			)}
		</>
	);
}

function savedInputs(inbox: Status["inbox"]): string[] {
	return inbox.flatMap(([, input]) =>
		input.type === "input"
			? input.content.flatMap((part) =>
					part.type === "input_text" ? [part.text] : part.type === "input_file" || part.type === "input_image" ? [part.file.filename ?? "Attachment"] : [],
				)
			: [],
	);
}
