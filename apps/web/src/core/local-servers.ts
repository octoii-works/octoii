import type { PageListed, TerminalListed, Turn } from "@octoii-works/octoii-client";
import { developmentAddress } from "./links.ts";

const LINK = /https?:\/\/[^\s<>"'`]+/gi;
const SERVER_LIMIT = 12;

/** Recent development addresses reported by this thread and its own terminals. */
export function localServersOf(pages: readonly PageListed[], terminals: readonly TerminalListed[], turns: readonly Turn[]): readonly string[] {
	const servers = new Set<string>();
	const add = (value: string): void => {
		const url = developmentAddress(value);
		if (url !== undefined) servers.add(url.origin);
	};
	for (const terminal of terminals) for (const address of terminal.local_servers) add(address);
	for (const page of pages) add(page.url);
	for (const text of reportsOf(turns)) {
		if (servers.size >= SERVER_LIMIT) break;
		for (const match of text.matchAll(LINK)) add(match[0].replace(/[.,;!)}]+$/, ""));
	}
	return [...servers].slice(0, SERVER_LIMIT);
}

function* reportsOf(turns: readonly Turn[]): Generator<string> {
	for (const turn of turns.slice(-12).reverse()) {
		for (const step of turn.steps.toReversed()) {
			for (const item of step.reply?.response.output ?? []) {
				if (item.type !== "message" || item.role !== "assistant") continue;
				for (const part of item.content) if (part.type === "output_text") yield part.text;
			}
			for (const call of step.calls) {
				for (const part of call.result?.output.output ?? []) if (part.type === "input_text") yield part.text;
			}
		}
	}
}
