/**
 * A page of the thread's browser as a message names it, `:page{id=…}`: its chip, a globe and its title; its card, its
 * title and its address. A closed page can open its last address from the thread's record.
 */
import { type ReactNode, useContext } from "react";
import type { PageReference } from "@octoii-works/octoii-client";
import { Globe } from "lucide-react";
import { pageName } from "../../core/things.ts";
import { pageKey, showSurface } from "../../components/state/panel.ts";
import { cn } from "../../components/ui/cn.ts";
import { IconTile } from "../../components/ui/icon-tile.tsx";
import { MessageThreadContext } from "../../components/thread/message-thread.ts";
import { useBrowserReference } from "../../components/effects/browser-reference.ts";
import { CARD, CardWords, CHIP, CHIP_ICON, GONE_CHIP, type ThingView } from "../../components/thread/thing-look.tsx";

type PageLink = { readonly listed: PageReference; readonly label?: string; readonly show: (() => void) | undefined };

/** A live tab, or the last address the thread kept for the page. */
function usePage(page: string): PageLink | undefined {
	const shown = useContext(MessageThreadContext);
	const listed = shown?.pages.find((each) => each.id === page);
	const thread = shown?.thread;
	const kept = useBrowserReference(thread, page, listed === undefined ? shown?.pageReference : undefined);
	if (thread === undefined) return undefined;
	if (listed !== undefined) return { listed, show: () => showSurface(thread, pageKey(page)) };
	if (kept === undefined) return undefined;
	const open = shown?.openBrowser;
	return { listed: kept, label: `Reopen ${pageName(kept)}`, show: open && (() => open(kept.url, kept.host === "desktop" ? "desktop" : "background")) };
}

/** A page named within a line: a globe and its title, shown in the panel as it is clicked. */
function PageChip(props: { readonly page: string }): ReactNode {
	const page = usePage(props.page);
	if (page === undefined) return <span className={GONE_CHIP}>a page no longer open</span>;
	if (page.show === undefined) return <span className={GONE_CHIP}>{pageName(page.listed)}</span>;
	return (
		<button type="button" data-link aria-label={page.label} title={page.listed.url} onClick={page.show} className={cn(CHIP, "cursor-pointer")}>
			<Globe className={CHIP_ICON} />
			{pageName(page.listed)}
		</button>
	);
}

/** A page named on a line of its own: a globe, its title and its address. */
function PageCard(props: { readonly page: string }): ReactNode {
	const page = usePage(props.page);
	return (
		<button type="button" disabled={page?.show === undefined} aria-label={page?.label} onClick={page?.show} className={CARD}>
			<IconTile>
				<Globe />
			</IconTile>
			{page === undefined ? <CardWords name="A page no longer open" /> : <CardWords name={pageName(page.listed)} detail={page.listed.url} />}
		</button>
	);
}

/** A page of the thread's browser, as a message names it. */
export const PAGE_VIEW: ThingView = { Chip: (props) => <PageChip page={props.id} />, Card: (props) => <PageCard page={props.id} /> };
