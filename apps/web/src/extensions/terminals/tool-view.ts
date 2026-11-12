/** The `terminal` tool as the page shows a call of it: a command run in a terminal, or what was done to one. */
import { toolView, type ToolView } from "../../core/tools.ts";

/** The terminals extension's tools. */
export const TERMINAL_TOOLS: Readonly<Record<string, ToolView>> = {
	terminal: toolView({
		work: "command",
		verbs: ["Use a terminal", "Using a terminal", "Used a terminal"],
		subject: (given) => given.command ?? (given.terminal === undefined ? undefined : `${given.action ?? ""} terminal ${String(given.terminal)}`.trim()),
		code: true,
	}),
};
