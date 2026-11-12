/**
 * The browser in the panel: a tab for each page of the thread's, named by its title, the page below, closing it as it
 * is closed; and `+`'s way to open one.
 */
import type { PageListed } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { Globe } from "lucide-react";
import { pageName } from "../../core/browser.ts";
import type { Opening, Surface } from "../../components/panel/right-panel.tsx";
import { failed } from "../../components/state/notices.ts";
import { pageKey, showSurface } from "../../components/state/panel.ts";
import { BrowserPane } from "./browser-pane.tsx";

/** The size a page opens at, before the view that opens it lays it out at its room. */
const OPENED_PAGE = { width: 1280, height: 800, scale: 1 } as const;

/** The panel's part of the pages `pages` of the thread `thread`; nothing where the browser is switched off (`on`). */
export function useBrowserPanel(
	thread: string,
	pages: readonly PageListed[],
	on: boolean,
): { readonly surfaces: readonly Surface[]; readonly openings: readonly Opening[] } {
	const client = useClient();
	if (!on) return { surfaces: [], openings: [] };
	const surfaces = pages.map((page): Surface => ({
		key: pageKey(page.id),
		icon: <Globe />,
		label: pageName(page),
		body: <BrowserPane page={page} />,
		close: () => {
			client.closePage(page.id).catch((error: unknown) => failed("The page could not be closed", error));
		},
	}));
	const opening: Opening = {
		label: "Browser",
		icon: Globe,
		letter: "B",
		open: () => {
			client.openPage(thread, undefined, OPENED_PAGE).then(
				(id) => showSurface(thread, pageKey(id)),
				(error: unknown) => failed("A page could not be opened", error),
			);
		},
	};
	return { surfaces, openings: [opening] };
}
