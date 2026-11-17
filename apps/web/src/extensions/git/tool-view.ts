/** Repository and linked-worktree operations, shown by their action. */
import { toolView, type ToolView, type Verbs } from "../../core/tools.ts";

const ACTIONS: Readonly<Record<string, Verbs>> = {
	status: ["Inspect Git status", "Inspecting Git status", "Inspected Git status"],
	list_worktrees: ["List worktrees", "Listing worktrees", "Listed worktrees"],
	create_worktree: ["Create a worktree", "Creating a worktree", "Created a worktree"],
	remove_worktree: ["Remove a worktree", "Removing a worktree", "Removed a worktree"],
};

export const GIT_TOOLS: Readonly<Record<string, ToolView>> = {
	git: toolView({
		work: "other",
		verbs: (given) => ACTIONS[given.action ?? ""] ?? ["Use Git", "Using Git", "Used Git"],
		subject: (given) => given.path,
	}),
};
