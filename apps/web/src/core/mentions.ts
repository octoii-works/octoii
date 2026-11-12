/**
 * A mention as the person writes it: `@` at the start or after a space, and what follows it up to the caret, which the
 * things the thread may name — its terminals, its pages, the person's other threads — and the files and folders under
 * its folder are searched for; and the text with the one chosen in its place — a path as a markdown link to it, a thing
 * as the directive that names it — which the message shows by its name and the model reads by its path or its id.
 */
import type { FoundPath, Listed, PageListed, TerminalListed } from "@octoii-works/octoii-client";
import { linkTo } from "./links.ts";
import { terminalName } from "./terminal.ts";
import { directiveOf, pageName, type Thing } from "./things.ts";
import { activeAt, isArchived, threadTitle } from "./threads.ts";

/** What a mention may name: a path under the thread's folder, or a thing. */
export type Choice = { readonly path: FoundPath } | { readonly thing: Thing };

/** A mention being written: what is typed of it, and where its `@` stands in the text. */
export type Mention = { readonly query: string; readonly start: number };

/** The mention the caret ends, where there is one. */
export function mentionAt(text: string, caret: number): Mention | undefined {
	const found = /(?:^|\s)@([^\s@]*)$/.exec(text.slice(0, caret));
	if (found === null) return undefined;
	const query = found[1] ?? "";
	return { query, start: caret - query.length - 1 };
}

/**
 * The text with `chosen` named in the place of the mention the caret ends, and a space after it, a space already there
 * taken as that one; and where the caret goes, after it.
 */
export function withMention(text: string, mention: Mention, caret: number, chosen: Choice): { readonly text: string; readonly caret: number } {
	const inserted = `${"path" in chosen ? linkTo(chosen.path.path, chosen.path.kind) : directiveOf(chosen.thing)} `;
	const after = text.slice(caret).replace(/^ /, "");
	return { text: `${text.slice(0, mention.start)}${inserted}${after}`, caret: mention.start + inserted.length };
}

/** The most threads a mention offers. */
const OFFERED_THREADS = 8;

/**
 * The things a mention in the thread `own` may name: its terminals and its pages, then the person's other threads —
 * not a sub-thread, nor one archived — the latest at work first, as many as a menu offers.
 */
export function thingsOf(
	rows: readonly Listed[],
	terminals: readonly TerminalListed[],
	pages: readonly PageListed[],
	own: string | undefined,
): readonly Thing[] {
	const threads = rows
		.filter((row) => row.id !== own && row.parent == null && !isArchived(row))
		.toSorted((a, b) => activeAt(b) - activeAt(a))
		.slice(0, OFFERED_THREADS);
	return [
		...terminals.map((terminal): Thing => ({ kind: "terminal", id: String(terminal.number), name: terminalName(terminal) })),
		...pages.map((page): Thing => ({ kind: "page", id: page.id, name: pageName(page) })),
		...threads.map((row): Thing => ({ kind: "thread", id: row.id, name: threadTitle(row) })),
	];
}

/** The things of `things` whose names hold `query`, whatever their case. */
export function matchingThings(things: readonly Thing[], query: string): readonly Thing[] {
	const sought = query.toLowerCase();
	return things.filter((thing) => thing.name.toLowerCase().includes(sought));
}
