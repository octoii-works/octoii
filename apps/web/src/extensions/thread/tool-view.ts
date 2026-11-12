/** The `thread` tool as the page shows a call of it: a thread started, messaged or stopped, by its title or its message. */
import { toolView, type ToolView, type Verbs } from "../../core/tools.ts";

/** The `thread` tool's verbs, by its action. */
const THREAD_VERBS: Readonly<Record<string, Verbs>> = {
	start: ["Start a thread", "Starting a thread", "Started a thread"],
	send: ["Message a thread", "Messaging a thread", "Messaged a thread"],
	stop: ["Stop a thread", "Stopping a thread", "Stopped a thread"],
};

/** The threads extension's tools. */
export const THREAD_TOOLS: Readonly<Record<string, ToolView>> = {
	thread: toolView({
		work: "thread",
		verbs: (given) => THREAD_VERBS[given.action ?? ""] ?? ["Work with a thread", "Working with a thread", "Worked with a thread"],
		subject: (given) => given.title ?? given.message?.split("\n")[0],
	}),
};
