/**
 * What is mentioned at the composer's caret, and what it may be: the things the thread may name whose names hold what is
 * typed after `@`, then the files and folders under the thread's folder whose paths hold it, best first, searched once
 * the person pauses; which of them is highlighted, moved by the arrow keys; and whether the person dismissed the menu
 * with Escape, till they begin another mention.
 */
import { useState } from "react";
import { type Choice, type Mention, matchingThings, mentionAt } from "../../core/mentions.ts";
import type { Thing } from "../../core/things.ts";
import { useDebounced } from "../effects/debounced.ts";
import { useOutcome } from "../effects/read.ts";

/** The most paths a mention's menu offers. */
const MENTION_CHOICES = 20;

/** A key the menu took: whether it did, and what was chosen where it was Enter or Tab. */
export type MentionKey = { readonly handled: boolean; readonly chosen?: Choice };

/** The mention being written and its menu: what it may be; whether paths are still searched; the one highlighted; and what a key does. */
export type Mentions = {
	readonly mention: Mention | undefined;
	readonly choices: readonly Choice[];
	readonly searching: boolean;
	readonly highlighted: number;
	readonly highlight: (index: number) => void;
	readonly key: (key: string) => MentionKey;
	/** Dismisses the menu till another mention begins. */
	readonly dismiss: () => void;
};

/** The mention the caret ends in `text`, where the composer mentions `things` or the files of `folder`. */
export function useMentions(folder: string | undefined, things: readonly Thing[], text: string, caret: number): Mentions {
	const written = folder === undefined && things.length === 0 ? undefined : mentionAt(text, caret);
	const [dismissed, setDismissed] = useState<number | undefined>(undefined);
	const mention = written?.start === dismissed ? undefined : written;
	const query = useDebounced(mention?.query ?? "", 80);
	const outcome = useOutcome("files.search", mention === undefined || folder === undefined ? undefined : { path: folder, query, limit: MENTION_CHOICES });
	const found = outcome === undefined ? undefined : "result" in outcome ? outcome.result.found : [];
	const choices: readonly Choice[] = [...matchingThings(things, mention?.query ?? "").map((thing) => ({ thing })), ...(found ?? []).map((path) => ({ path }))];
	const [highlight, setHighlight] = useState({ query: "", index: 0 });
	const count = choices.length;
	const highlighted = highlight.query === query ? Math.min(highlight.index, Math.max(count - 1, 0)) : 0;
	const key = (pressed: string): MentionKey => {
		if (mention === undefined) return { handled: false };
		if (pressed === "Escape") {
			setDismissed(mention.start);
			return { handled: true };
		}
		const chosen = choices[highlighted];
		if (chosen === undefined) return { handled: false };
		if (pressed === "ArrowDown" || pressed === "ArrowUp") {
			setHighlight({ query, index: (highlighted + (pressed === "ArrowDown" ? 1 : count - 1)) % count });
			return { handled: true };
		}
		return pressed === "Enter" || pressed === "Tab" ? { handled: true, chosen } : { handled: false };
	};
	return {
		mention,
		choices,
		searching: folder !== undefined && found === undefined,
		highlighted,
		highlight: (index) => setHighlight({ query, index }),
		key,
		dismiss: () => setDismissed(mention?.start),
	};
}
