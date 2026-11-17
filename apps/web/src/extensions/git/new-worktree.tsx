/** A task may request a new worktree; preparation belongs to its accepted thread. */
import type { ReactNode } from "react";
import type { CreateWorktree } from "@octoii-works/octoii-client";
import { useConnection } from "@octoii-works/octoii-client/react";
import { useExtension } from "../../components/state/extensions.ts";
import { useOutcome } from "../../components/effects/read.ts";

export function NewWorktree(props: {
	readonly folder: string | undefined;
	readonly value: CreateWorktree | undefined;
	readonly onChange: (value: CreateWorktree | undefined) => void;
}): ReactNode {
	const connection = useConnection();
	const git = useExtension("git");
	const allowed =
		git &&
		connection.status === "open" &&
		["files.read", "git.operate", "threads.operate"].every((scope) => connection.scopes.some((given) => given === scope));
	const outcome = useOutcome("git.repository", allowed && props.folder !== undefined ? { path: props.folder } : undefined);
	if (!allowed || props.folder === undefined) return null;
	const head = outcome !== undefined && "result" in outcome ? outcome.result.repository?.head : undefined;
	return (
		<div className="mt-3 text-xs text-muted-foreground">
			<label className="inline-flex items-center gap-2">
				<input
					type="checkbox"
					checked={props.value !== undefined}
					disabled={head === undefined && props.value === undefined}
					onChange={(event) => {
						if (!event.target.checked) {
							props.onChange(undefined);
							return;
						}
						if (head !== undefined && props.folder !== undefined) props.onChange({ path: props.folder, destination: "", branch: "", base: head });
					}}
				/>
				Use a new worktree
			</label>
			{outcome === undefined ? (
				<p className="mt-2">Reading repository…</p>
			) : "failure" in outcome ? (
				<p role="alert" className="mt-2">
					{outcome.failure}
				</p>
			) : head === undefined ? (
				<p className="mt-2">Requires a Git repository with an existing commit.</p>
			) : null}
			{props.value === undefined ? null : <WorktreeFields value={props.value} onChange={props.onChange} />}
		</div>
	);
}

function WorktreeFields({ value, onChange }: { readonly value: CreateWorktree; readonly onChange: (value: CreateWorktree) => void }): ReactNode {
	return (
		<div className="mt-3 space-y-2 rounded-lg border border-border p-3">
			<label className="block">
				New branch
				<input
					aria-label="New branch"
					className="mt-1 block w-full rounded-md border border-border bg-background px-2 py-1.5 text-foreground"
					value={value.branch}
					onChange={(event) => onChange({ ...value, branch: event.target.value })}
					placeholder="task/my-change"
				/>
			</label>
			<label className="block">
				New directory
				<input
					aria-label="New directory"
					className="mt-1 block w-full rounded-md border border-border bg-background px-2 py-1.5 text-foreground"
					value={value.destination}
					onChange={(event) => onChange({ ...value, destination: event.target.value })}
					placeholder="Absolute path on this computer"
				/>
			</label>
			<p>From commit {value.base.slice(0, 12)}. After sending, review and start preparation in the thread.</p>
		</div>
	);
}
