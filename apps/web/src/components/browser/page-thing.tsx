/**
 * A page of the thread's browser as a message names it, `:page{id=…}`: its chip, a globe and its title; its card, its
 * title and its address — shown in the panel as either is clicked; one closed said to be gone.
 */
import { type ReactNode, useContext } from "react";
import type { PageListed } from "@octoii-works/octoii-client";
import { Globe } from "lucide-react";
import { pageName } from "../../core/things.ts";
import { pageKey, showSurface } from "../state/panel.ts";
import { cn } from "../ui/cn.ts";
import { IconTile } from "../ui/icon-tile.tsx";
import { MessageThreadContext } from "../thread/message-thread.ts";
import { CARD, CardWords, CHIP, CHIP_ICON, GONE_CHIP, type ThingView } from "../thread/thing-look.tsx";

/** The page `page` of the thread shown, as its browser lists it, and the way to show it; none where it is closed. */
function usePage(page: string): { readonly listed: PageListed; readonly show: () => void } | undefined {
	const shown = useContext(MessageThreadContext);
	const listed = shown?.pages.find((each) => each.id === page);
	if (shown === undefined || listed === undefined) return undefined;
	return { listed, show: () => showSurface(shown.thread, pageKey(page)) };
}

/** A page named within a line: a globe and its title, shown in the panel as it is clicked. */
function PageChip(props: { readonly page: string }): ReactNode {
	const page = usePage(props.page);
	if (page === undefined) return <span className={GONE_CHIP}>a page no longer open</span>;
	return (
		<button type="button" data-link title={page.listed.url} onClick={page.show} className={cn(CHIP, "cursor-pointer")}>
			<Globe className={CHIP_ICON} />
			{pageName(page.listed)}
		</button>
	);
}

/** A page named on a line of its own: a globe, its title and its address. */
function PageCard(props: { readonly page: string }): ReactNode {
	const page = usePage(props.page);
	return (
		<button type="button" disabled={page === undefined} onClick={page?.show} className={CARD}>
			<IconTile>
				<Globe />
			</IconTile>
			{page === undefined ? <CardWords name="A page no longer open" /> : <CardWords name={pageName(page.listed)} detail={page.listed.url} />}
		</button>
	);
}

/** A page of the thread's browser, as a message names it. */
export const PAGE_VIEW: ThingView = { Chip: (props) => <PageChip page={props.id} />, Card: (props) => <PageCard page={props.id} /> };
