/**
 * The model's todo list in a thread: the todos of the latest `todo` call the timeline holds, as its result told them —
 * each call gives the whole list, replacing the one before.
 */
import type { Todo } from "@octoii-works/octoii-client";
import type { TimelineItem, Work } from "./timeline.ts";

/** The todos of the latest todo call in `items`; none where no call gave a list. */
export function latestTodos(items: readonly TimelineItem[]): readonly Todo[] | undefined {
	for (let at = items.length - 1; at >= 0; at--) {
		const item = items[at];
		if (item?.kind !== "turn") continue;
		const todos = lastIn(item.turn.work);
		if (todos !== undefined) return todos;
	}
	return undefined;
}

/** The todos of the last todo call among `work` and the calls made under it. */
function lastIn(work: readonly Work[]): readonly Todo[] | undefined {
	for (let at = work.length - 1; at >= 0; at--) {
		const each = work[at];
		if (each?.kind !== "call") continue;
		const under = lastIn(each.calls);
		if (under !== undefined) return under;
		if (each.structured?.type === "todos") return each.structured.todos;
	}
	return undefined;
}

/** How many of `todos` are completed. */
export function completed(todos: readonly Todo[]): number {
	return todos.filter((todo) => todo.status === "completed").length;
}
