/**
 * The browser in the panel: a tab for each page of the thread's, named by its title, the page below, closing it as it
 * is closed; and `+`'s way to open one.
 */
import type { PageListed } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { Globe, Server } from "lucide-react";
import { pageName } from "../../core/browser.ts";
import type { Opening, OpeningGroup, Surface } from "../../components/panel/right-panel.tsx";
import { failed } from "../../components/state/notices.ts";
import { pageKey } from "../../components/state/panel.ts";
import { openThreadPage } from "../../components/state/browser.ts";
import { BrowserPane } from "./browser-pane.tsx";

/** The panel's part of the pages `pages` of the thread `thread`; nothing where the browser is switched off (`on`). */
export function useBrowserPanel(
	thread: string,
	options: {
		readonly pages: readonly PageListed[];
		readonly on: boolean;
		readonly onSent: (delivery: string) => void;
		readonly servers: readonly string[];
	},
): { readonly surfaces: readonly Surface[]; readonly openings: readonly Opening[]; readonly groups: readonly OpeningGroup[] } {
	const client = useClient();
	const { pages, on, onSent, servers } = options;
	if (!on) return { surfaces: [], openings: [], groups: [] };
	const surfaces = pages.map((page): Surface => ({
		key: pageKey(page.id),
		icon: <Globe />,
		label: pageName(page),
		body: <BrowserPane thread={thread} page={page} onSent={onSent} />,
		close: () => {
			client.closePage(page.id).catch((error: unknown) => failed("The page could not be closed", error));
		},
	}));
	const opening: Opening = {
		label: "Browser",
		icon: Globe,
		letter: "B",
		open: () => openThreadPage(client, thread),
	};
	const groups: readonly OpeningGroup[] =
		servers.length === 0
			? []
			: [
					{
						label: "Local servers",
						icon: Server,
						items: servers.map((address) => ({ label: address, open: () => openThreadPage(client, thread, address) })),
					},
				];
	return { surfaces, openings: [opening], groups };
}
