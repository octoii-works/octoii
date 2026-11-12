/**
 * What Octoii says to a thread's model within what it reads, apart from what is read: its words in `<system-reminder>`
 * tags — a note after a tool's result, how another thread's turn ended. A text read as the parts it is made of, the
 * words of outside and Octoii's, in their order, so the page can show either as it needs.
 */

/** A part of a text: words of outside, or Octoii's. */
export type Part = { readonly kind: "text"; readonly text: string } | { readonly kind: "reminder"; readonly text: string };

/** Each of Octoii's words in a text, whole tags alone. */
const REMINDERS = /<system-reminder>([\s\S]*?)<\/system-reminder>/g;

/** `text` as its parts, in their order; an empty text none. */
export function partsOf(text: string): readonly Part[] {
	const parts: Part[] = [];
	let at = 0;
	for (const found of text.matchAll(REMINDERS)) {
		if (found.index > at) parts.push({ kind: "text", text: text.slice(at, found.index) });
		parts.push({ kind: "reminder", text: found[1] ?? "" });
		at = found.index + found[0].length;
	}
	if (at < text.length) parts.push({ kind: "text", text: text.slice(at) });
	return parts;
}

/** The first of Octoii's words in `text`, where it opens with them; none where it opens with words of outside. */
export function openingReminder(text: string): string | undefined {
	const [first] = partsOf(text);
	return first?.kind === "reminder" ? first.text : undefined;
}
