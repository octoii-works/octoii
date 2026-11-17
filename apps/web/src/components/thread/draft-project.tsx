/** The draft headline's project picker; the same picker also opens from its details. */
import { useState, type ReactNode } from "react";
import { ChevronDown, FolderPlus } from "lucide-react";
import type { Project } from "../../core/threads.ts";
import { folderName } from "../../core/threads.ts";
import { Command, CommandSearch, CommandList, CommandItem, CommandGroup } from "../ui/command.tsx";
import { Popover, PopoverTrigger, PopoverPopup } from "../ui/popover.tsx";

export function DraftProject(props: {
	readonly folder: string | undefined;
	readonly projects: readonly Project[];
	readonly onChoose: (folder: string) => void;
	readonly onAnother: () => void;
}): ReactNode {
	const [open, setOpen] = useState(false);
	const projects =
		props.folder !== undefined && !props.projects.some((project) => project.folder === props.folder)
			? [{ folder: props.folder, name: folderName(props.folder) }, ...props.projects]
			: props.projects;
	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger
				aria-label="Choose project"
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
								<CommandItem
									key={project.folder}
									value={project}
									onClick={() => {
										props.onChoose(project.folder);
										setOpen(false);
									}}
									className="h-control"
								>
									<span className="truncate" title={project.folder}>
										{project.name}
									</span>
								</CommandItem>
							)}
						</CommandGroup>
					</CommandList>
				</Command>
				<button
					type="button"
					className="flex w-full items-center gap-2 border-t border-border px-4 py-3 text-ui hover:bg-accent"
					onClick={() => {
						setOpen(false);
						props.onAnother();
					}}
				>
					<FolderPlus className="size-4" />
					Add project…
				</button>
			</PopoverPopup>
		</Popover>
	);
}
