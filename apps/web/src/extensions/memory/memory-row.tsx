/**
 * Memory in the thread's details: a row of how many memories its project keeps, opening them to read and delete.
 */
import { type ReactNode, useState } from "react";
import { Brain } from "lucide-react";
import { useOutcome } from "../../components/effects/read.ts";
import { OPENING, ROW, RowContent } from "../../components/thread/details.tsx";
import { cn } from "../../components/ui/cn.ts";
import { Popover, PopoverPopup, PopoverTrigger } from "../../components/ui/popover.tsx";
import { MemoryCard } from "./memory-card.tsx";

/** What the models remember of the project the folder is in, by how many memories, opening them; nothing where there are none. */
export function MemoryRow(props: { readonly folder: string; readonly revision: string }): ReactNode {
	const [deleted, setDeleted] = useState(0);
	const outcome = useOutcome("memory.list", { path: props.folder }, `${props.revision}:${String(deleted)}`);
	if (outcome === undefined || "failure" in outcome || outcome.result.memories.length === 0) return null;
	const count = outcome.result.memories.length;
	return (
		<Popover>
			<PopoverTrigger render={<button type="button" title={outcome.result.folder ?? undefined} className={cn(ROW, OPENING)} />}>
				<RowContent icon={<Brain />} label="Memory" aside={count === 1 ? "1 memory" : `${String(count)} memories`} />
			</PopoverTrigger>
			<PopoverPopup side="left" align="start" sideOffset={12} className="w-[min(24rem,calc(100vw-2rem))] p-0">
				<MemoryCard path={props.folder} listed={outcome.result} onDeleted={() => setDeleted((count) => count + 1)} />
			</PopoverPopup>
		</Popover>
	);
}
