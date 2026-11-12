/**
 * What the person sent before, called back into the composer as a shell's history is: earlier, one message further back
 * each time, from the latest; later, one nearer, back to what they were writing past the latest.
 */

/** Where the composer stands in what was sent: the message it shows, by its place, and what was being written before. */
export type Recalled = { readonly at: number; readonly draft: string };

/** What the composer shows after a step through what was sent, and where it then stands; none where it stands still. */
export type Step = { readonly text: string; readonly recalled: Recalled | undefined };

/** One message further back than `recalled` — from the latest where it stands nowhere — `text` kept as what was being written. */
export function earlier(sent: readonly string[], recalled: Recalled | undefined, text: string): Step | undefined {
	const at = recalled === undefined ? sent.length - 1 : recalled.at - 1;
	const message = sent[at];
	if (message === undefined) return undefined;
	return { text: message, recalled: { at, draft: recalled?.draft ?? text } };
}

/** One message nearer than `recalled`; past the latest, what was being written; none where it stands nowhere. */
export function later(sent: readonly string[], recalled: Recalled | undefined): Step | undefined {
	if (recalled === undefined) return undefined;
	const message = sent[recalled.at + 1];
	return message === undefined ? { text: recalled.draft, recalled: undefined } : { text: message, recalled: { at: recalled.at + 1, draft: recalled.draft } };
}
