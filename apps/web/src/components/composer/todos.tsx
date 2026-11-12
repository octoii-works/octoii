/**
 * The model's todo list, docked over the composer: a line of how many of its steps are done and the one in progress,
 * which opens to the whole list — each step marked pending, in progress or done, a done one struck through.
 */
import type { ReactNode } from "react";
import type { Todo } from "@octoii-works/octoii-client";
import { Circle, CircleCheck, CircleDot, ListChecks } from "lucide-react";
import { completed } from "../../core/todos.ts";
import { cn } from "../ui/cn.ts";
import { Collapsible, CollapsiblePanel, CollapsibleTrigger, FoldChevron } from "../ui/collapsible.tsx";

/** The list `todos`; nothing where it has none. */
export function TodoStrip(props: { readonly todos: readonly Todo[] }): ReactNode {
	const { todos } = props;
	if (todos.length === 0) return null;
	const doing = todos.find((todo) => todo.status === "in_progress");
	return (
		<Collapsible className="mx-3 mb-2 rounded-lg border border-border bg-card">
			<CollapsibleTrigger className="flex h-control-sm w-full items-center gap-2 rounded-lg px-2.5 text-ui hover:bg-accent">
				<ListChecks className="size-3.5 shrink-0 text-muted-foreground" />
				<span className="shrink-0 tabular-nums text-muted-foreground">
					{completed(todos)}/{todos.length}
				</span>
				<span className="min-w-0 flex-1 truncate text-left text-foreground">{doing?.content ?? (completed(todos) === todos.length ? "All done" : "")}</span>
				<FoldChevron className="text-muted-foreground" />
			</CollapsibleTrigger>
			<CollapsiblePanel>
				<ol className="flex flex-col gap-0.5 px-2.5 pb-2">
					{todos.map((todo) => (
						<li key={todo.content} className="flex items-start gap-2 text-ui">
							<TodoMark status={todo.status} />
							<span className={cn("min-w-0 flex-1", todo.status === "completed" ? "text-muted-foreground line-through" : "text-foreground")}>
								{todo.content}
							</span>
						</li>
					))}
				</ol>
			</CollapsiblePanel>
		</Collapsible>
	);
}

/** A step's mark: an empty circle while pending, a dot in progress, a check once done. */
function TodoMark(props: { readonly status: Todo["status"] }): ReactNode {
	const className = "mt-[3px] size-3.5 shrink-0";
	switch (props.status) {
		case "completed":
			return <CircleCheck className={cn(className, "text-muted-foreground")} />;
		case "in_progress":
			return <CircleDot className={cn(className, "text-foreground")} />;
		case "pending":
			return <Circle className={cn(className, "text-muted-foreground")} />;
	}
}
