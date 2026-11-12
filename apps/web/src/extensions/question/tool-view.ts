/** The `question` tool as the page shows a call of it: asking about what each question's header names. */
import { toolView, type ToolView } from "../../core/tools.ts";

/** The questions extension's tools. */
export const QUESTION_TOOLS: Readonly<Record<string, ToolView>> = {
	question: toolView({
		work: "ask",
		verbs: ["Ask about", "Asking about", "Asked about"],
		subject: (given) => given.questions?.map((question) => question.header).join(", "),
	}),
};
