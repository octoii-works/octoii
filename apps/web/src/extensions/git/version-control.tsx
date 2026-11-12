/**
 * Git in the thread's details: the repository its folder is in — its branch and how it stands against its upstream —
 * and its changes, opening the Diff view; or that it is in none.
 */
import type { ReactNode } from "react";
import type { Repository } from "@octoii-works/octoii-client";
import { FileDiff, GitBranch } from "lucide-react";
import { defaultScope, scopeName, shortCount, totalsOf } from "../../core/diff.ts";
import { useOutcome } from "../../components/effects/read.ts";
import { showSurface, useThreadPanel } from "../../components/state/panel.ts";
import { DetailRow } from "../../components/thread/details.tsx";

/** The repository the thread's folder is in: its branch and how it stands against its upstream, and its changes; or that it is in none. */
export function VersionControl(props: { readonly thread: string; readonly folder: string; readonly revision: string }): ReactNode {
	const outcome = useOutcome("git.repository", { path: props.folder }, props.revision);
	if (outcome === undefined) return null;
	if ("failure" in outcome) {
		return (
			<section className="p-1.5">
				<DetailRow icon={<GitBranch />} label={outcome.failure} title={outcome.failure} />
			</section>
		);
	}
	const repository = outcome.result.repository;
	if (repository === undefined) {
		return (
			<section className="p-1.5">
				<DetailRow icon={<GitBranch />} label="Not a git repository" />
			</section>
		);
	}
	const tracking = [repository.ahead > 0 ? `↑${String(repository.ahead)}` : "", repository.behind > 0 ? `↓${String(repository.behind)}` : ""].join(" ").trim();
	return (
		<section className="p-1.5">
			<DetailRow
				icon={<GitBranch />}
				label={repository.branch ?? `Detached at ${repository.head?.slice(0, 7) ?? "no commit"}`}
				title={repository.upstream === undefined ? repository.branch : `${repository.branch ?? "HEAD"} → ${repository.upstream}`}
				{...(tracking === "" ? {} : { aside: tracking })}
			/>
			<Changes thread={props.thread} folder={props.folder} repository={repository} revision={props.revision} />
		</section>
	);
}

/** The changes the thread's Diff view shows — the branch's where the person chose none — by the lines they gained and lost, which open the view. */
function Changes(props: { readonly thread: string; readonly folder: string; readonly repository: Repository; readonly revision: string }): ReactNode {
	const scope = useThreadPanel(props.thread).scope ?? defaultScope(props.repository);
	const outcome = useOutcome("git.changes", { path: props.folder, scope }, props.revision);
	const totals = outcome !== undefined && "result" in outcome ? totalsOf(outcome.result.files) : undefined;
	return (
		<DetailRow
			icon={<FileDiff />}
			label={scopeName(scope, [])}
			onOpen={() => showSurface(props.thread, "diff")}
			{...(totals === undefined
				? {}
				: {
						aside: (
							<span className="font-mono">
								<span className="text-diff-add">+{shortCount(totals.additions)}</span> <span className="text-diff-remove">−{shortCount(totals.deletions)}</span>
							</span>
						),
					})}
		/>
	);
}
