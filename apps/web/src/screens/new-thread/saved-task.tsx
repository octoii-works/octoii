/** Recover a submission whose request was saved before it was sent. */
import { type ReactNode, useTransition } from "react";
import { failed } from "../../components/state/notices.ts";
import { Button } from "../../components/ui/button.tsx";
import type { NewThread, PendingThread } from "./submission.ts";

export function DraftLoading({ error, retry }: { readonly error: string | undefined; readonly retry: () => void }): ReactNode {
	return (
		<p className="p-4 text-body text-muted-foreground">
			{error === undefined ? (
				"Loading saved task…"
			) : (
				<>
					Saved tasks could not be read. {error}{" "}
					<button type="button" className="underline" onClick={retry}>
						Retry
					</button>
				</>
			)}
		</p>
	);
}

export function SavedTask({
	pending,
	starting,
	refresh,
	onAccepted,
}: {
	readonly pending: PendingThread;
	readonly starting: NewThread;
	readonly refresh: () => void;
	readonly onAccepted: (pending: PendingThread, thread: string) => void;
}): ReactNode {
	const [busy, startAction] = useTransition();
	const { workspace } = pending;
	async function resume(): Promise<void> {
		try {
			const { key, thread } = await starting.resume(pending.key);
			onAccepted(pending, thread);
			await starting.clear(key).catch((error: unknown) => failed("The saved copy could not be cleared", error));
		} catch (error) {
			failed("The task could not be resumed", error);
			refresh();
		}
	}
	async function discard(): Promise<void> {
		try {
			await starting.clear(pending.key);
			refresh();
		} catch (error) {
			failed("The saved copy could not be discarded", error);
		}
	}
	return (
		<section aria-label="Saved task" className="rounded-lg border border-border p-4 text-body">
			<p className="font-medium">{pending.thread === undefined ? "Unfinished submission" : "Task accepted"}</p>
			<p className="my-3 max-h-48 overflow-auto whitespace-pre-wrap break-words">{pending.draft.text}</p>
			<p className="break-all text-xs text-muted-foreground">
				{pending.project ?? "No project"} · {pending.draft.attachments.length} attachments
			</p>
			{workspace.kind === "isolated" ? (
				<p className="mt-2 break-all text-xs text-muted-foreground">Separate directory{workspace.base === undefined ? "" : ` · ${workspace.base}`}</p>
			) : null}
			<div className="mt-4 flex flex-wrap gap-2">
				<Button size="sm" disabled={busy} onClick={() => startAction(resume)}>
					{pending.thread === undefined ? "Continue task" : "Open thread"}
				</Button>
				<Button size="sm" variant="ghost-muted" disabled={busy} onClick={() => startAction(discard)}>
					Discard saved copy
				</Button>
			</div>
		</section>
	);
}
