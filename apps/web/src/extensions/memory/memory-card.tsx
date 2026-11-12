/**
 * What the thread's models remember of its project, as the details card opens it: each memory by its name and what it
 * holds, which the person deletes where they want it gone.
 */
import type { ReactNode } from "react";
import { useTransition } from "react";
import type { MemoryListed } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { Trash2 } from "lucide-react";
import { failed } from "../../components/state/notices.ts";
import { IconButton } from "../../components/ui/button.tsx";

/** The project's memories, under where they are kept; `onDeleted` told once one is gone, for them to be read again. */
export function MemoryCard(props: { readonly path: string; readonly listed: MemoryListed; readonly onDeleted: () => void }): ReactNode {
	const client = useClient();
	const [deleting, startDeleting] = useTransition();
	const forget = (file: string): void =>
		startDeleting(async () => {
			try {
				await client.deleteMemory(props.path, file);
				props.onDeleted();
			} catch (error) {
				failed("The memory could not be deleted", error);
			}
		});
	return (
		<div className="flex flex-col text-ui">
			<p className="truncate border-b border-border px-3.5 py-2.5 text-xs text-muted-foreground" title={props.listed.folder ?? undefined}>
				What the model keeps of this project across threads
			</p>
			<ul className="flex max-h-80 flex-col overflow-y-auto p-1.5">
				{props.listed.memories.map((memory) => (
					<li key={memory.file} className="flex items-start gap-2 rounded-md py-1.5 ps-2">
						<div className="flex min-w-0 flex-1 flex-col gap-0.5">
							<span className="flex items-baseline gap-2">
								<span className="truncate font-medium">{memory.name}</span>
								{memory.type === "" ? null : <span className="shrink-0 text-xs text-muted-foreground">{memory.type}</span>}
							</span>
							{memory.description === "" ? null : <span className="text-xs text-muted-foreground">{memory.description}</span>}
						</div>
						<IconButton label={`Delete ${memory.name}`} size="icon-xs" disabled={deleting} onClick={() => forget(memory.file)}>
							<Trash2 />
						</IconButton>
					</li>
				))}
			</ul>
		</div>
	);
}
