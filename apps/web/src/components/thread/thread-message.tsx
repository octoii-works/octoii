/**
 * A message another thread gave this one — sent by its model, or what came of one this thread sent it — as the input
 * side shows it, folded: whose it is, a way into that thread, and its first line; opened in place, its words whole.
 */
import { type ReactNode, useState } from "react";
import { useThreads } from "@octoii-works/octoii-client/react";
import { ChevronDown, MessagesSquare } from "lucide-react";
import { plainLine } from "../../core/sub-threads.ts";
import { threadTitle } from "../../core/threads.ts";
import type { Said } from "../../core/timeline.ts";
import { go } from "../state/navigation.ts";
import { cn } from "../ui/cn.ts";
import { Prose } from "./markdown.tsx";

/** The message `said`, from the thread it names. */
export function ThreadMessage(props: { readonly said: Said }): ReactNode {
	const { said } = props;
	const [open, setOpen] = useState(false);
	const from = said.from ?? "";
	const row = useThreads()?.find((each) => each.id === from);
	return (
		<div className="flex flex-col items-end">
			<div className={cn("min-w-0 max-w-[80%] rounded-lg bg-muted", open && "w-full")}>
				<div className="flex min-w-0 items-center gap-2 px-3.5 py-2 text-ui">
					<MessagesSquare className="size-3.5 shrink-0 text-muted-foreground" />
					<button
						type="button"
						disabled={row === undefined}
						onClick={() => go({ place: "thread", thread: from })}
						className="max-w-1/2 shrink-0 cursor-pointer truncate text-muted-foreground hover:text-foreground hover:underline disabled:cursor-default disabled:no-underline"
					>
						{row === undefined ? "A thread" : threadTitle(row)}
					</button>
					<button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-left">
						<span className="min-w-0 flex-1 truncate text-foreground">{open ? "" : plainLine(said.text)}</span>
						<ChevronDown className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", open && "rotate-180")} />
					</button>
				</div>
				{open ? <Prose text={said.text} className="px-3.5 pb-2.5 text-body" /> : null}
			</div>
		</div>
	);
}
