/**
 * The panel's tabs, in the place the frame gives the panel: a bar of them, which the arrow keys walk — the thread's
 * Files view where it is open, then what the extensions show, as the page gives it: the changes, the terminals, the
 * pages — each as wide as its name, narrower where the bar runs out of room; `+` to open what the extensions offer or
 * the Files view, by its letter while the menu is open; the tab chosen shown
 * below, each other kept as it was while it is hidden, its terminal or page no longer followed; and the frame's own
 * buttons — at the bar's left the one that shows the sidebar, where the panel stands at the page's left edge, at its
 * right those that have the panel fill the page's column and that hide it. A view's tab is named by what it
 * shows, the Files view's by its file and that file's icon. A tab closes from its cross, taken out, and what its
 * extension ends with it ended: a terminal's closes the terminal, a page's the page.
 */
import { Activity, type KeyboardEvent, type ReactNode, useState } from "react";
import { Tabs } from "@base-ui/react/tabs";
import { Files, Plus, X } from "lucide-react";
import { nameOf } from "../../core/files.ts";
import { closeSurface, type SurfaceKey, showSurface, type ThreadPanel, useThreadPanel, type View } from "../state/panel.ts";
import { PathIcon } from "../files/path-icon.tsx";
import { FillPanel, HidePanel, ShowSidebarFromPanel } from "../frame/frame.tsx";
import { Button, IconButton } from "../ui/button.tsx";
import { Menu, MenuItem, MenuItemLabel, MenuPopup, MenuShortcut, MenuTrigger } from "../ui/menu.tsx";
import { FilesSurface } from "../files/files-surface.tsx";

/** What the panel is given: its thread, the folder it works in — none where it works in none — where its turns stand,
 * which its views are read again at, and what the extensions show in it and offer to open. */
export type RightPanelProps = {
	readonly thread: string;
	readonly folder: string | undefined;
	readonly revision: string;
	readonly surfaces: readonly Surface[];
	readonly openings: readonly Opening[];
};

/** What an extension shows in the panel: its tab — its key, its icon, its name — what it shows, and what closing it ends beside the tab. */
export type Surface = {
	readonly key: SurfaceKey;
	readonly icon: ReactNode;
	readonly label: string;
	readonly body: ReactNode;
	readonly close?: () => void;
};

/** What `+` opens: its name, icon and letter, and how. */
export type Opening = { readonly label: string; readonly icon: typeof Files; readonly letter: string; readonly open: () => void };

/** The panel. */
export function RightPanel(props: RightPanelProps): ReactNode {
	const place = useThreadPanel(props.thread);
	const tabs: readonly Surface[] = [...place.views.filter((view) => view === "files").map((view) => filesTab(view, place, props)), ...props.surfaces];
	const shown = tabs.find((tab) => tab.key === place.shown) ?? tabs[0];
	const openings: readonly Opening[] = [...props.openings, { label: "Files", icon: Files, letter: "F", open: () => showSurface(props.thread, "files") }];
	const close = (tab: Surface, index: number): void => {
		closeSurface(props.thread, tab.key, (tabs[index - 1] ?? tabs[index + 1])?.key);
		tab.close?.();
	};
	return (
		<Tabs.Root value={shown?.key ?? null} onValueChange={(key: SurfaceKey) => showSurface(props.thread, key)} className="flex min-h-0 flex-1 flex-col">
			<div className="flex h-[var(--topbar-height)] shrink-0 items-center gap-1 px-2">
				<ShowSidebarFromPanel />
				{/* Positioned, as the list measures where a tab stands in it to keep the one chosen in view. */}
				<Tabs.List aria-label="The panel's tabs" className="relative flex min-w-0 items-center gap-1 overflow-x-auto scrollbar-none">
					{tabs.map((tab, index) => (
						<TabButton key={tab.key} tab={tab} onClose={() => close(tab, index)} />
					))}
				</Tabs.List>
				<OpenMenu openings={openings} />
				<span className="flex-1" />
				<FillPanel />
				<HidePanel />
			</div>
			{shown === undefined ? <Empty openings={openings} /> : null}
			{tabs.map((tab) => (
				<Tabs.Panel key={tab.key} value={tab.key} keepMounted className="flex min-h-0 flex-1 flex-col">
					<Activity mode={tab === shown ? "visible" : "hidden"}>{tab.body}</Activity>
				</Tabs.Panel>
			))}
		</Tabs.Root>
	);
}

/** The tab of the Files view, named by the file read. */
function filesTab(view: View, place: ThreadPanel, props: RightPanelProps): Surface {
	return {
		key: view,
		icon: place.file === undefined ? <Files /> : <PathIcon path={place.file} kind="file" />,
		label: place.file === undefined ? "Files" : nameOf(place.file),
		body: <FilesSurface thread={props.thread} folder={props.folder} />,
	};
}

/**
 * A tab: its icon and name, shaded while it is the one chosen, as wide as its name and its cross and narrower, its name
 * cut short, where the bar runs out of room — never past its icon and a few letters; its cross shown as it is pointed at or focused, and always, larger, where a finger
 * points.
 */
function TabButton(props: { readonly tab: Surface; readonly onClose: () => void }): ReactNode {
	const { icon, label } = props.tab;
	return (
		<span className="group/tab flex h-control-sm max-w-48 min-w-16 items-center rounded-md text-ui text-muted-foreground hover:bg-accent has-data-active:bg-accent has-data-active:text-foreground">
			<Tabs.Tab
				value={props.tab.key}
				title={label}
				className="flex h-full min-w-0 cursor-pointer items-center gap-1.5 rounded-md pr-1 pl-2 outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset [&>svg]:size-3.5 [&>svg]:shrink-0"
			>
				{icon}
				<span className="truncate">{label}</span>
			</Tabs.Tab>
			<button
				type="button"
				aria-label={`Close ${label}`}
				onClick={props.onClose}
				className="mr-1 flex size-control-2xs shrink-0 cursor-pointer items-center justify-center rounded-sm opacity-0 group-focus-within/tab:opacity-100 group-hover/tab:opacity-100 hover:bg-accent pointer-coarse:size-control-xs pointer-coarse:opacity-100"
			>
				<X className="size-3" />
			</button>
		</span>
	);
}

/** `+`: a menu of what opens, each also opened by its letter while the menu is open. */
function OpenMenu(props: { readonly openings: readonly Opening[] }): ReactNode {
	const [open, setOpen] = useState(false);
	const onKeyDown = (event: KeyboardEvent): void => {
		const opening = openingOf(props.openings, event);
		if (opening === undefined) return;
		event.preventDefault();
		event.stopPropagation();
		setOpen(false);
		opening.open();
	};
	return (
		<Menu open={open} onOpenChange={setOpen}>
			<MenuTrigger render={<IconButton label="New tab" />}>
				<Plus />
			</MenuTrigger>
			<MenuPopup className="w-48" onKeyDownCapture={onKeyDown}>
				{props.openings.map((opening) => (
					<MenuItem key={opening.label} onClick={opening.open} aria-keyshortcuts={opening.letter}>
						<opening.icon />
						<MenuItemLabel>{opening.label}</MenuItemLabel>
						<MenuShortcut>{opening.letter}</MenuShortcut>
					</MenuItem>
				))}
			</MenuPopup>
		</Menu>
	);
}

/** The opening a key pressed alone names by its letter, where one does. */
function openingOf(openings: readonly Opening[], event: KeyboardEvent): Opening | undefined {
	if (event.metaKey || event.ctrlKey || event.altKey || event.nativeEvent.isComposing) return undefined;
	return openings.find((opening) => opening.letter.toLowerCase() === event.key.toLowerCase());
}

/** A panel with no tab: what `+` opens, offered at once. */
function Empty(props: { readonly openings: readonly Opening[] }): ReactNode {
	return (
		<div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 text-ui text-muted-foreground">
			Nothing is open.
			<div className="flex flex-wrap justify-center gap-2">
				{props.openings.map((opening) => (
					<Button key={opening.label} variant="outline" size="sm" onClick={opening.open}>
						<opening.icon />
						{opening.label}
					</Button>
				))}
			</div>
		</div>
	);
}
