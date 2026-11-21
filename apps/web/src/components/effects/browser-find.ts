/** A search in the visible Electron guest. Hiding the tab ends the search. */
import { type KeyboardEvent, useCallback, useLayoutEffect, useRef, useState } from "react";
import { pressed, shortcutOf } from "../../core/shortcuts.ts";
import { failed } from "../state/notices.ts";

export const FIND_SHORTCUT = "mod+f";
const findShortcut = shortcutOf(FIND_SHORTCUT);

export type FindablePage = HTMLElement & {
	findInPage(text: string, options: { readonly findNext: boolean; readonly forward: boolean }): number;
	stopFindInPage(action: "clearSelection"): void;
};

type Match = { readonly requestId: number; readonly activeMatchOrdinal: number; readonly matches: number; readonly finalUpdate: boolean };
type Search = { readonly query: string; readonly match?: Match };

export function useBrowserFind(guest: FindablePage | undefined, address: string) {
	const [search, setSearch] = useState<Search>();
	const request = useRef<number | undefined>(undefined);
	const field = useRef<HTMLInputElement>(null);
	const clear = useCallback((): void => {
		if (request.current === undefined) return;
		request.current = undefined;
		try {
			if (guest?.isConnected) guest.stopFindInPage("clearSelection");
		} catch {
			// The guest can disappear while the panel is being hidden.
		}
	}, [guest]);
	useLayoutEffect(() => {
		if (guest === undefined) return;
		const found = (event: Event): void => {
			// SAFETY: Electron dispatches the found-in-page event with a result on this webview element.
			const { result } = event as Event & { readonly result: Match };
			if (!result.finalUpdate || result.requestId !== request.current) return;
			setSearch((active) => (active === undefined ? undefined : { ...active, match: result }));
		};
		guest.addEventListener("found-in-page", found);
		return () => {
			guest.removeEventListener("found-in-page", found);
			clear();
			setSearch(undefined);
		};
	}, [guest, address, clear]);
	const locate = (query: string, direction: "start" | "forward" | "backward"): void => {
		if (guest === undefined) return;
		try {
			request.current = guest.findInPage(query, { findNext: direction === "start", forward: direction !== "backward" });
		} catch (error) {
			failed("The page could not be searched", error);
		}
	};
	const open =
		guest === undefined
			? undefined
			: (): void => {
					setSearch((active) => active ?? { query: "" });
					field.current?.focus();
					field.current?.select();
				};
	return {
		search: guest === undefined ? undefined : search,
		field,
		open,
		close: () => {
			clear();
			setSearch(undefined);
		},
		edit: (query: string) => {
			setSearch({ query });
			if (query === "") clear();
			else locate(query, "start");
		},
		move: (direction: "forward" | "backward") => {
			if (search?.match && search.match.matches > 0) locate(search.query, direction);
		},
		shortcut: (event: KeyboardEvent) => {
			if (open === undefined || event.nativeEvent.isComposing || !pressed(findShortcut, event)) return;
			event.preventDefault();
			event.stopPropagation();
			open();
		},
	};
}
