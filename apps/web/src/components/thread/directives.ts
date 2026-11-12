/**
 * The directives a message's Markdown may hold, as CommonMark's proposal for generic directives writes them, given their
 * meaning on the page — a thing the person can open, by its kind and its id: `:thread{id=…}`, `:page{id=…}` or
 * `:terminal{id=…}` within a line, drawn as the thing's chip; `::thread{id=…}`, `::page{id=…}` or `::terminal{id=…}` on a
 * line of its own, drawn as its card. Any other directive — and one that names no thing, `:name`, which ordinary words
 * hold, as `Error:failed` or `10:30` do — is given back as the words it was written in, so nothing written is lost.
 */
import type { Nodes, Root } from "mdast";
import { isKind } from "../../core/things.ts";

/** The element a thing named within a line is drawn as, its kind and its id its properties. */
export const CHIP_ELEMENT = "thing-chip";
/** The element a thing named on a line of its own is drawn as, its kind and its id its properties. */
export const CARD_ELEMENT = "thing-card";

/** Gives the directives of a message's tree their meaning, read against the Markdown it was parsed from. */
export function remarkDirectives(): (tree: Root, file: { readonly value: unknown }) => void {
	return (tree, file) => {
		meant(tree, String(file.value));
	};
}

/** `node` and what it holds, each directive given its meaning or its words. */
function meant(node: Nodes, source: string): void {
	if ("children" in node) for (const child of node.children) meant(child, source);
	if ((node.type === "textDirective" || node.type === "leafDirective") && isKind(node.name)) {
		const id = node.attributes?.["id"];
		if (typeof id === "string" && id !== "") {
			const hName = node.type === "textDirective" ? CHIP_ELEMENT : CARD_ELEMENT;
			Object.assign(node, { data: { hName, hProperties: { kind: node.name, id }, hChildren: [] } });
			return;
		}
	}
	if (node.type === "textDirective") Object.assign(node, { value: writtenOf(node, source) });
	if (node.type === "leafDirective" || node.type === "containerDirective") {
		Object.assign(node, { data: { hName: "p", hChildren: [{ type: "text", value: writtenOf(node, source) }] } });
	}
}

/**
 * The words `node` was written in, as `source` holds them — which, as what makes the page's elements of a tree reads a
 * node with a value, a directive within a line is given as, text; one of its own, as a paragraph of that text.
 */
function writtenOf(node: Nodes, source: string): string {
	const from = node.position?.start.offset;
	const to = node.position?.end.offset;
	return from === undefined || to === undefined ? "" : source.slice(from, to);
}
