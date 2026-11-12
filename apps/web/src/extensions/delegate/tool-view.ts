/** The `delegate` tool as the page shows a call of it: a task delegated, by its sub-thread's title or its first line. */
import { toolView, type ToolView } from "../../core/tools.ts";

/** The sub-threads extension's tools. */
export const DELEGATE_TOOLS: Readonly<Record<string, ToolView>> = {
	delegate: toolView({ work: "delegate", verbs: ["Delegate", "Delegating", "Delegated"], subject: (given) => given.title ?? given.task?.split("\n")[0] }),
};
