/**
 * The composer's way back through what the person sent in the thread: ↑ at the box's start shows the message before,
 * from the latest, its caret at the start for the next; ↓ at its end the one after, and past the latest what they were
 * writing; typing keeps what the box shows as their own.
 */
import { type KeyboardEvent, useState } from "react";
import { earlier, later, type Recalled } from "../../core/recall.ts";
import type { Placement } from "../effects/caret.ts";

/** How the box's keys call back what was sent, and how typing ends it. */
export type Recall = { readonly key: (event: KeyboardEvent<HTMLTextAreaElement>) => boolean; readonly typed: () => void };

/** The way back through `sent`, the person's messages oldest first, for a box showing `text`, set by `setText`, its caret placed by `place`. */
export function useRecall(sent: readonly string[], text: string, setText: (text: string) => void, place: (placement: Placement) => void): Recall {
	const [recalled, setRecalled] = useState<Recalled | undefined>(undefined);
	const key = (event: KeyboardEvent<HTMLTextAreaElement>): boolean => {
		if (event.shiftKey || event.altKey || event.metaKey || event.ctrlKey) return false;
		const box = event.currentTarget;
		const collapsed = box.selectionStart === box.selectionEnd;
		const step =
			event.key === "ArrowUp" && collapsed && box.selectionStart === 0
				? earlier(sent, recalled, text)
				: event.key === "ArrowDown" && collapsed && box.selectionEnd === text.length
					? later(sent, recalled)
					: undefined;
		if (step === undefined) return false;
		setRecalled(step.recalled);
		setText(step.text);
		place({ caret: event.key === "ArrowUp" ? 0 : step.text.length });
		return true;
	};
	return { key, typed: () => setRecalled(undefined) };
}
