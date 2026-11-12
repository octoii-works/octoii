/**
 * Shortcuts the window answers while the component is drawn: one held with Cmd or Ctrl wherever the focus is, text
 * fields too; a key alone, such as `/` or a digit, only while no text field has the focus, which takes it as typing;
 * and one a text field keeps as its own, as ⌘Z its undo, only while none has the focus. The latest of what they do is
 * done, the window listening once.
 */
import { useEffect, useEffectEvent } from "react";
import { pressed, shortcutOf } from "../../core/shortcuts.ts";

/** Whether the person types in `element`: a text field, a box or what is edited in place. */
export function isTypedIn(element: EventTarget | null): boolean {
	return element instanceof HTMLElement && (element.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(element.tagName));
}

/** Answers each shortcut of `specs` with `act`, told which was pressed. */
export function useShortcuts(specs: readonly string[], act: (spec: string) => void): void {
	useKeys(specs, act, (mod, typedIn) => mod || !typedIn);
}

/** Answers the shortcut `spec` with `act` only while no text field has the focus, which keeps the keys as its own. */
export function useShortcutOutsideText(spec: string, act: () => void): void {
	useKeys([spec], act, (_, typedIn) => !typedIn);
}

/** Answers each shortcut of `specs` with `act` where `answers` says so, told whether it is held with Cmd or Ctrl and whether a text field has the focus. */
function useKeys(specs: readonly string[], act: (spec: string) => void, answers: (mod: boolean, typedIn: boolean) => boolean): void {
	const answer = useEffectEvent(act);
	const asked = JSON.stringify(specs);
	const answered = useEffectEvent(answers);
	useEffect(() => {
		// SAFETY: `asked` is the JSON of the specs the caller gave, so it parses back to them.
		const shortcuts = (JSON.parse(asked) as readonly string[]).map((spec) => ({ spec, shortcut: shortcutOf(spec) }));
		const down = (event: KeyboardEvent): void => {
			if (event.isComposing) return;
			const hit = shortcuts.find(({ shortcut }) => pressed(shortcut, event));
			if (hit === undefined || !answered(hit.shortcut.mod, isTypedIn(event.target))) return;
			event.preventDefault();
			answer(hit.spec);
		};
		document.addEventListener("keydown", down);
		return () => document.removeEventListener("keydown", down);
	}, [asked]);
}

/** Answers the shortcut `spec` with `act`. */
export function useShortcut(spec: string, act: () => void): void {
	useShortcuts([spec], act);
}
