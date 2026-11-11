/**
 * A file mentioned as the person writes: `@` at the start or after a space, and what follows it up to the caret, which
 * the files and folders under the thread's folder are searched for; and the text with the one chosen in its place, as a
 * markdown link to it — which the message shows by its name and the model reads by its path.
 */
import { linkTo } from "./links.ts";

/** A file or a folder found under the thread's folder, by its path from it. */
export type FoundPath = { readonly path: string; readonly kind: "file" | "directory" };

/** What a search found, each path as the node gives it — a folder's ending in `/` — as a file or a folder. */
export function foundPathsOf(found: readonly string[]): readonly FoundPath[] {
	return found.map((path) => (path.endsWith("/") ? { path: path.slice(0, -1), kind: "directory" } : { path, kind: "file" }));
}

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
 * The text with a link to `found` in the place of the mention the caret ends, and a space after it, a space already
 * there taken as that one; and where the caret goes, after it.
 */
export function withMention(text: string, mention: Mention, caret: number, found: FoundPath): { readonly text: string; readonly caret: number } {
	const inserted = `${linkTo(found.path, found.kind)} `;
	const after = text.slice(caret).replace(/^ /, "");
	return { text: `${text.slice(0, mention.start)}${inserted}${after}`, caret: mention.start + inserted.length };
}
