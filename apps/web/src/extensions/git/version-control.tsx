/** Read-only repository details and the changes shown in the thread's Diff view. */
import { type ReactNode, useState } from "react";
import type { Repository } from "@octoii-works/octoii-client";
import { ChevronDown, FileDiff, GitBranch, LoaderCircle, RefreshCw } from "lucide-react";
import { defaultScope, shortCount, totalsOf } from "../../core/diff.ts";
import { useOutcome } from "../../components/effects/read.ts";
import { showSurface, useThreadPanel } from "../../components/state/panel.ts";
import { DetailRow, OPENING, ROW, RowContent } from "../../components/thread/details.tsx";
import { cn } from "../../components/ui/cn.ts";
import { CopyButton } from "../../components/ui/copy-button.tsx";
import { Popover, PopoverPopup, PopoverTrigger } from "../../components/ui/popover.tsx";

/** Repository reads retry independently of the conversation's revision. */
export function VersionControl(props: { readonly thread: string; readonly folder: string; readonly revision: string }): ReactNode {
	const [retry, setRetry] = useState(0);
	const outcome = useOutcome("git.repository", { path: props.folder }, `${props.revision}:${String(retry)}`);
	return (
		<section className="p-1.5">
			{outcome === undefined ? (
				<DetailRow icon={<LoaderCircle className="animate-spin" />} label="Loading repository…" />
			) : "failure" in outcome ? (
				<DetailRow icon={<RefreshCw />} label="Retry repository" title={outcome.failure} onOpen={() => setRetry(retry + 1)} />
			) : outcome.result.repository === undefined ? (
				<DetailRow icon={<GitBranch />} label="Not a Git repository" />
			) : (
				<>
					<Branch repository={outcome.result.repository} />
					<Changes {...props} repository={outcome.result.repository} />
				</>
			)}
		</section>
	);
}

/** Branch information is inspectable and copyable, without changing the checkout. */
function Branch({ repository }: { readonly repository: Repository }): ReactNode {
	const name = repository.branch ?? `Detached at ${repository.head?.slice(0, 7) ?? "no commit"}`;
	const tracking = [repository.ahead > 0 ? `↑${String(repository.ahead)}` : "", repository.behind > 0 ? `↓${String(repository.behind)}` : ""].join(" ").trim();
	return (
		<Popover>
			<PopoverTrigger render={<button type="button" aria-label="Branch details" className={cn(ROW, OPENING)} />}>
				<RowContent
					icon={<GitBranch />}
					label={name}
					aside={
						<>
							{tracking}
							<ChevronDown className="size-3.5" />
						</>
					}
				/>
			</PopoverTrigger>
			<PopoverPopup side="bottom" align="end" className="w-72 max-w-[calc(100vw-1rem)] p-3 text-ui">
				<div className="flex items-start gap-2">
					<p className="min-w-0 flex-1 break-all font-mono">{name}</p>
					<CopyButton text={repository.branch ?? repository.head ?? name} label="Copy branch or commit" />
				</div>
				<dl className="mt-3 space-y-1 text-xs">
					<dt className="text-muted-foreground">Upstream</dt>
					<dd className="break-all font-mono">{repository.upstream ?? "No upstream configured"}</dd>
					{repository.upstream === undefined ? null : (
						<dd className="text-muted-foreground">
							{repository.ahead} ahead · {repository.behind} behind
						</dd>
					)}
				</dl>
			</PopoverPopup>
		</Popover>
	);
}

/** The scope stays in sync with the Diff panel; failures have an explicit retry. */
function Changes(props: { readonly thread: string; readonly folder: string; readonly repository: Repository; readonly revision: string }): ReactNode {
	const [retry, setRetry] = useState(0);
	const scope = useThreadPanel(props.thread).scope ?? defaultScope(props.repository);
	const outcome = useOutcome("git.changes", { path: props.folder, scope }, `${props.revision}:${String(retry)}`);
	const scopeLabel =
		scope.type === "branch" ? `vs ${props.repository.base ?? "base"}` : scope.type === "uncommitted" ? "Uncommitted" : scope.commit.slice(0, 7);
	if (outcome !== undefined && "failure" in outcome) {
		return <DetailRow icon={<RefreshCw />} label="Retry changes" title={outcome.failure} onOpen={() => setRetry(retry + 1)} />;
	}
	const totals = outcome === undefined ? undefined : totalsOf(outcome.result.files);
	return (
		<DetailRow
			icon={<FileDiff />}
			label={
				<span className="flex min-w-0 items-baseline gap-2">
					<span>Changes</span>
					<span className="truncate text-xs text-muted-foreground">{scopeLabel}</span>
				</span>
			}
			title={`Changes · ${scopeLabel}`}
			onOpen={() => showSurface(props.thread, "diff")}
			aside={
				totals === undefined ? (
					<LoaderCircle aria-label="Loading changes" className="size-3.5 animate-spin" />
				) : (
					<span className="font-mono">
						<span className="text-diff-add">+{shortCount(totals.additions)}</span> <span className="text-diff-remove">−{shortCount(totals.deletions)}</span>
					</span>
				)
			}
		/>
	);
}
