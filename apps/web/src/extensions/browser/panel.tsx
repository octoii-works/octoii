/**
 * The browser in the panel: a tab for each page of the thread's, named by its title, the page below, closing it as it
 * is closed; and `+`'s way to open one.
 */
import type { PageListed, PagePlacement } from "@octoii-works/octoii-client";
import { useClient, useConnection, usePages } from "@octoii-works/octoii-client/react";
import { Globe, Server } from "lucide-react";
import { useCallback } from "react";
import { pageName } from "../../core/browser.ts";
import type { Opening, OpeningGroup, Surface } from "../../components/panel/right-panel.tsx";
import { failed } from "../../components/state/notices.ts";
import { pageKey } from "../../components/state/panel.ts";
import { openThreadPage } from "../../components/state/browser.ts";
import { useExtensionAccess } from "../../components/state/extensions.ts";
import { BrowserPane } from "./browser-pane.tsx";

const NO_PAGES: readonly PageListed[] = [];

/** Keeps the browser workspace visible while the node connection is being restored. */
export function useThreadBrowser(thread: string) {
	const client = useClient();
	const available = useExtensionAccess("browser", "browser.operate");
	const connection = useConnection();
	const reconnecting = connection.status === "waiting" || connection.status === "connecting";
	const pages = usePages(available || reconnecting ? thread : undefined) ?? NO_PAGES;
	const openBrowser = useCallback((address: string, placement?: PagePlacement) => openThreadPage(client, thread, address, placement), [client, thread]);
	const pageReference = useCallback(async (page: string) => (await client.call("browser.reference", { thread, page })).reference, [client, thread]);
	return { available, pages, openBrowser, pageReference };
}

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
	const surfaces = pages.map((page): Surface => ({
		key: pageKey(page.id),
		icon: <Globe />,
		label: pageName(page),
		body: <BrowserPane thread={thread} page={page} onSent={onSent} />,
		...(on ? { close: () => void client.closePage(page.id).catch((error: unknown) => failed("The page could not be closed", error)) } : {}),
	}));
	if (!on) return { surfaces, openings: [], groups: [] };
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
