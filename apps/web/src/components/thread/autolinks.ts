/**
 * Addresses written bare in a message, as GFM links them, ended where a full-width punctuation mark stands after them —
 * a comma, a full stop, a closing bracket of Chinese or Japanese prose — which GFM, ending one only at a space or ASCII
 * punctuation, would take into the address with the words after it. The mark and those words are given back as text;
 * an address's own letters, a path of Chinese words among them, are kept.
 */
import type { Link, Nodes, Root } from "mdast";

/** The full-width marks prose ends an address with. */
const MARKS = /[，。、；：！？）】」』》〉（]/;

/** Ends the bare addresses of a message's tree at the full-width mark after each, read against the Markdown it was parsed from. */
export function remarkAutolinkEnds(): (tree: Root, file: { readonly value: unknown }) => void {
	return (tree, file) => {
		ended(tree, String(file.value));
	};
}

/** `node` and what it holds, each bare address in it ended at its mark. */
function ended(node: Nodes, source: string): void {
	if (!("children" in node)) return;
	for (let at = node.children.length - 1; at >= 0; at--) {
		const child = node.children[at];
		if (child === undefined) continue;
		ended(child, source);
		if (child.type !== "link") continue;
		const rest = restOf(child, source);
		if (rest === undefined) continue;
		// SAFETY: a link stands only among phrasing content, beside which a text node may stand.
		(node.children as Nodes[]).splice(at + 1, 0, { type: "text", value: rest });
	}
}

/** Where `link` is an address written bare holding a full-width mark: the link cut before the mark, and what it held from the mark on; none otherwise. */
function restOf(link: Link, source: string): string | undefined {
	const [only] = link.children;
	if (link.children.length !== 1 || only?.type !== "text") return undefined;
	const from = link.position?.start.offset;
	const to = link.position?.end.offset;
	if (from === undefined || to === undefined || source.slice(from, to) !== only.value) return undefined;
	const cut = only.value.search(MARKS);
	if (cut <= 0) return undefined;
	const rest = only.value.slice(cut);
	only.value = only.value.slice(0, cut);
	link.url = link.url.slice(0, link.url.length - rest.length);
	return rest;
}
