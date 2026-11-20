/** The draft headline's project picker; the same picker also opens from its details. */
import { useState, type ReactNode } from "react";
import { ChevronDown, FolderPlus, Folders } from "lucide-react";
import type { Project } from "../../core/threads.ts";
import { folderName } from "../../core/threads.ts";
import { Command, CommandSearch, CommandList, CommandItem, CommandGroup } from "../../components/ui/command.tsx";
import { Popover, PopoverTrigger, PopoverPopup } from "../../components/ui/popover.tsx";

export function DraftProject(props: {
	readonly folder: string | undefined;
	readonly projects: readonly Project[];
	readonly onChoose: (folder: string) => void;
	readonly onAnother: () => void;
	readonly directoryAction: { readonly label: string; readonly select: () => void } | undefined;
	readonly disabled: boolean;
}): ReactNode {
	const [open, setOpen] = useState(false);
	const { directoryAction } = props;
	const select = (action: () => void): void => {
		setOpen(false);
		action();
	};
	const projects =
		props.folder !== undefined && !props.projects.some((project) => project.folder === props.folder)
			? [{ folder: props.folder, name: folderName(props.folder) }, ...props.projects]
			: props.projects;
	return (
		<Popover open={open && !props.disabled} onOpenChange={setOpen}>
			<PopoverTrigger
				aria-label="Choose project"
				disabled={props.disabled}
				className="inline-flex max-w-full items-baseline gap-1 rounded-md outline-none hover:text-primary focus-visible:ring-2 focus-visible:ring-ring"
			>
				<span className="truncate">{props.folder === undefined ? "a project" : folderName(props.folder)}</span>
				<ChevronDown className="size-4 shrink-0 self-center text-muted-foreground" />
			</PopoverTrigger>
			<PopoverPopup align="center" className="w-72 max-w-[calc(100vw-2rem)]">
				<Command items={[{ items: projects }]} itemToStringValue={(project: { folder: string; name: string }) => `${project.name} ${project.folder}`}>
					<CommandSearch placeholder="Search projects…" aria-label="Search projects" />
					<CommandList empty="No matching projects">
						<CommandGroup title="Projects" items={projects}>
							{(project) => (
								<CommandItem key={project.folder} value={project} onClick={() => select(() => props.onChoose(project.folder))} className="h-control">
									<span className="truncate" title={project.folder}>
										{project.name}
									</span>
								</CommandItem>
							)}
						</CommandGroup>
					</CommandList>
				</Command>
				<div className="border-t border-border">
					<ProjectAction icon={<FolderPlus />} label="Add project…" onClick={() => select(props.onAnother)} />
					{directoryAction === undefined ? null : (
						<ProjectAction icon={<Folders />} label={directoryAction.label} onClick={() => select(directoryAction.select)} />
					)}
				</div>
			</PopoverPopup>
		</Popover>
	);
}

function ProjectAction(props: { readonly icon: ReactNode; readonly label: string; readonly onClick: () => void }): ReactNode {
	return (
		<button
			type="button"
			className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-ui hover:bg-accent focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring focus-visible:outline-none [&>svg]:size-4"
			onClick={props.onClick}
		>
			{props.icon}
			{props.label}
		</button>
	);
}
