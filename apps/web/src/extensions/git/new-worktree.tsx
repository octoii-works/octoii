/** The optional base for a draft using a separate directory. */
import { useState, type ReactNode } from "react";
import type { Branch } from "@octoii-works/octoii-client";
import { ChevronDown, Folders, GitBranch } from "lucide-react";
import { useOutcome } from "../../components/effects/read.ts";
import { Button } from "../../components/ui/button.tsx";
import { Popover, PopoverTrigger, PopoverPopup } from "../../components/ui/popover.tsx";
import { Command, CommandSearch, CommandList, CommandGroup, CommandItem } from "../../components/ui/command.tsx";

type WorktreeBase = { readonly commit: string; readonly label: string };
export type DraftWorktree = { readonly path: string; readonly base?: WorktreeBase | undefined };

export function SeparateDirectory(props: {
	readonly worktree: DraftWorktree;
	readonly disabled: boolean;
	readonly onBaseChange: ((base: WorktreeBase | undefined) => void) | undefined;
}): ReactNode {
	return (
		<div className="flex min-w-0 items-center gap-1.5 px-2 text-xs text-muted-foreground">
			<Folders className="size-3.5 shrink-0" />
			<span className="shrink-0">Separate directory</span>
			{props.onBaseChange === undefined ? null : (
				<BaseBranchChoice path={props.worktree.path} base={props.worktree.base} disabled={props.disabled} onChange={props.onBaseChange} />
			)}
		</div>
	);
}

function BaseBranchChoice(props: {
	readonly path: string;
	readonly base: WorktreeBase | undefined;
	readonly disabled: boolean;
	readonly onChange: (base: WorktreeBase | undefined) => void;
}): ReactNode {
	const [open, setOpen] = useState(false);
	const showing = open && !props.disabled;
	const branches = useOutcome("git.branches", showing ? { path: props.path } : undefined);
	const label = props.base?.label ?? "Current commit";
	const choose = (base: WorktreeBase | undefined): void => {
		props.onChange(base);
		setOpen(false);
	};
	return (
		<Popover open={showing} onOpenChange={setOpen}>
			<PopoverTrigger
				aria-label={`Change base branch (${label})`}
				disabled={props.disabled}
				render={<Button variant="ghost-muted" size="compact" className="min-w-0 shrink font-normal" />}
			>
				<GitBranch />
				<span className="truncate">{label}</span>
				<ChevronDown className="size-3" />
			</PopoverTrigger>
			<PopoverPopup className="w-72 max-w-[calc(100vw-2rem)]">
				{branches === undefined ? (
					<p role="status" className="px-4 py-3 text-ui text-muted-foreground">
						Loading branches…
					</p>
				) : "failure" in branches ? (
					<p role="alert" className="px-4 py-3 text-ui text-muted-foreground">
						{branches.failure}
					</p>
				) : (
					<BranchOptions branches={branches.result.branches} onChoose={choose} />
				)}
				{props.base === undefined ? null : (
					<div className="border-t border-border p-1.5">
						<Button variant="ghost" size="sm" className="w-full justify-start" onClick={() => choose(undefined)}>
							Use current commit
						</Button>
					</div>
				)}
			</PopoverPopup>
		</Popover>
	);
}

function BranchOptions(props: { readonly branches: readonly Branch[]; readonly onChoose: (base: WorktreeBase) => void }): ReactNode {
	const items = props.branches.map((branch) => ({ ...branch, label: branch.reference.replace(/^refs\/(heads|remotes)\//, "") }));
	return (
		<Command items={[{ items }]} itemToStringValue={(branch) => branch.label}>
			<CommandSearch placeholder="Search branches…" aria-label="Search branches" />
			<CommandList empty="No matching branches">
				<CommandGroup title="Base branch" items={items}>
					{(branch) => (
						<CommandItem
							key={branch.reference}
							value={branch}
							className="h-control"
							onClick={() => props.onChoose({ commit: branch.commit, label: branch.label })}
						>
							<GitBranch className="size-3.5 shrink-0" />
							<span className="truncate">{branch.label}</span>
						</CommandItem>
					)}
				</CommandGroup>
			</CommandList>
		</Command>
	);
}
