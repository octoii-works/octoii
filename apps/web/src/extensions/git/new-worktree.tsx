/** Shared workspace controls for the draft's context row and details. Selection never checks out a branch. */
import { useState, type ReactNode } from "react";
import type { Branches } from "@octoii-works/octoii-client";
import { ChevronDown, Folder, GitBranch } from "lucide-react";
import { Menu, MenuTrigger, MenuPopup, MenuRadioGroup, MenuRadioItem } from "../../components/ui/menu.tsx";
import { Popover, PopoverTrigger, PopoverPopup } from "../../components/ui/popover.tsx";
import { Command, CommandSearch, CommandList, CommandGroup, CommandItem } from "../../components/ui/command.tsx";

export type WorktreeBase = { readonly path: string; readonly base: string; readonly label: string };

const CONTROL =
	"inline-flex h-control-sm min-w-0 max-w-full items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";

export function WorkspaceChoice(props: {
	readonly folder: string;
	readonly branches: Branches;
	readonly value: WorktreeBase | undefined;
	readonly onChange: (value: WorktreeBase | undefined) => void;
}): ReactNode {
	const repository = props.branches.repository;
	return (
		<Menu>
			<MenuTrigger aria-label="Workspace" className={CONTROL}>
				<Folder className="size-3.5 shrink-0" />
				<span className="truncate">{props.value === undefined ? "Current checkout" : "New worktree"}</span>
				<ChevronDown className="size-3 shrink-0" />
			</MenuTrigger>
			<MenuPopup>
				<MenuRadioGroup
					value={props.value === undefined ? "current" : "new"}
					onValueChange={(value: string) => {
						if (value === "current") props.onChange(undefined);
						else if (repository.head !== undefined)
							props.onChange({ path: props.folder, base: repository.head, label: repository.branch ?? repository.head.slice(0, 7) });
					}}
				>
					<MenuRadioItem value="current">Current checkout</MenuRadioItem>
					<MenuRadioItem value="new" disabled={repository.head === undefined}>
						New worktree
					</MenuRadioItem>
				</MenuRadioGroup>
			</MenuPopup>
		</Menu>
	);
}

export function BaseBranchChoice(props: {
	readonly folder: string;
	readonly branches: Branches;
	readonly value: WorktreeBase | undefined;
	readonly onChange: (value: WorktreeBase) => void;
}): ReactNode {
	const [open, setOpen] = useState(false);
	const repository = props.branches.repository;
	const label = props.value?.label ?? repository.branch ?? repository.head?.slice(0, 7) ?? "No commits";
	const items = props.branches.branches.map((branch) => ({ ...branch, label: branch.reference.replace(/^refs\/(heads|remotes)\//, "") }));
	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger
				aria-label={props.value === undefined ? "Current branch" : "Base branch"}
				title={props.value === undefined ? "Select New worktree to choose a base branch" : "Base branch for the new worktree"}
				disabled={props.value === undefined}
				className={CONTROL}
			>
				<GitBranch className="size-3.5 shrink-0" />
				<span className="truncate">{label}</span>
				<ChevronDown className="size-3 shrink-0" />
			</PopoverTrigger>
			<PopoverPopup className="w-72 max-w-[calc(100vw-2rem)]">
				<Command items={[{ items }]} itemToStringValue={(item: { label: string }) => item.label}>
					<CommandSearch placeholder="Search branches…" aria-label="Search branches" />
					<CommandList empty="No matching branches">
						<CommandGroup title="Base branch" items={items}>
							{(branch) => (
								<CommandItem
									key={branch.reference}
									value={branch}
									className="h-control"
									onClick={() => {
										props.onChange({ path: props.folder, base: branch.commit, label: branch.label });
										setOpen(false);
									}}
								>
									<GitBranch className="size-3.5 shrink-0" />
									<span className="truncate">{branch.label}</span>
								</CommandItem>
							)}
						</CommandGroup>
					</CommandList>
				</Command>
			</PopoverPopup>
		</Popover>
	);
}
