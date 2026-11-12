/**
 * The page's ways of telling the person what happened while they look elsewhere: a value watched as it changes, told
 * from where it stood to where it stands, from its second reading on; and the page's title, as its tab shows it.
 */
import { useEffect, useEffectEvent, useRef } from "react";

/** Tells `act` each change of `value`, from where it stood before; nothing of where it first stands. */
export function useChanges<Value>(value: Value, act: (before: Value, after: Value) => void): void {
	const before = useRef<{ readonly value: Value } | undefined>(undefined);
	const answer = useEffectEvent(act);
	useEffect(() => {
		if (before.current !== undefined && before.current.value !== value) answer(before.current.value, value);
		before.current = { value };
	}, [value]);
}

/** The page's title, as its tab and the system show it. */
export function useTitle(title: string): void {
	useEffect(() => {
		document.title = title;
	}, [title]);
}
