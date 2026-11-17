/** Route-backed settings in a modal; the underlying conversation remains mounted. */
import { type ReactNode, useRef, useState } from "react";
import { Keyboard, MessagesSquare, Palette, Search, SlidersHorizontal, Sparkles, X } from "lucide-react";
import { SETTINGS_SECTIONS, type SettingsSection } from "../../core/route.ts";
import { type SettingsItem, settingsFound } from "../../core/settings-search.ts";
import { useIntoView } from "../effects/into-view.ts";
import { useShortcut } from "../effects/shortcut.ts";
import { useDevice } from "../state/device.ts";
import { IconButton } from "../ui/button.tsx";
import { Input } from "../ui/input.tsx";
import { Dialog, DialogClose, DialogPopup, DialogTitle } from "../ui/dialog.tsx";
import { Kbd } from "../ui/kbd.tsx";
import { GeneralPage } from "./general-page.tsx";
import { KeybindingsPage } from "./keybindings-page.tsx";
import { ModelsPage } from "./models-page.tsx";
import { ThreadsPage } from "./threads-page.tsx";

/** Each section's icon. */
const SECTION_ICONS: Readonly<Record<SettingsSection, typeof Palette>> = {
	general: SlidersHorizontal,
	models: Sparkles,
	threads: MessagesSquare,
	keybindings: Keyboard,
};

/**
 * What settings are given: the section open and the setting brought into view on it, where one is; and who is told of
 * another section — with a setting to bring into view — or of none as settings close.
 */
export type SettingsProps = {
	readonly section: SettingsSection;
	readonly at: string | undefined;
	readonly onSection: (section: SettingsSection | undefined, at?: string) => void;
};

/** How a line of the sections' list reads: muted, the one shown marked. */
const LINE =
	"group/line flex h-control-sm w-full items-center gap-2 rounded-md px-2 text-left text-ui text-muted-foreground outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring aria-[current=page]:bg-selected aria-[current=page]:text-foreground";

/** The modal's navigation: search and sections, stacked above content on narrow screens. */
function SettingsNav(props: SettingsProps): ReactNode {
	const [search, setSearch] = useState("");
	const box = useRef<HTMLInputElement>(null);
	useShortcut("/", () => box.current?.focus());
	const searching = search.trim() !== "";
	return (
		<aside className="flex min-h-0 shrink-0 flex-col border-b border-border bg-sidebar p-2 sm:w-52 sm:border-r sm:border-b-0">
			<label className="mb-1 flex h-control shrink-0 items-center gap-2 rounded-md px-2 focus-within:ring-1 focus-within:ring-ring hover:bg-accent">
				<Search className="size-4 shrink-0 text-muted-foreground" />
				<Input
					ref={box}
					type="search"
					variant="bare"
					aria-label="Search settings"
					placeholder="Search settings"
					value={search}
					onValueChange={setSearch}
					className="font-medium placeholder:text-muted-foreground"
				/>
				{searching ? null : <Kbd className="pointer-coarse:hidden">/</Kbd>}
			</label>
			<nav aria-label="Settings" className="flex gap-1 overflow-auto sm:flex-col max-sm:max-h-40">
				{searching ? (
					<Found items={settingsFound(search)} onSection={props.onSection} />
				) : (
					SETTINGS_SECTIONS.map((section) => {
						const Icon = SECTION_ICONS[section.key];
						return (
							<button
								key={section.key}
								type="button"
								aria-current={section.key === props.section ? "page" : undefined}
								onClick={() => props.onSection(section.key)}
								className={LINE + " shrink-0 max-sm:w-auto"}
							>
								<Icon className="size-4 text-muted-foreground group-aria-[current=page]/line:text-foreground" />
								{section.label}
							</button>
						);
					})
				)}
			</nav>
		</aside>
	);
}

/** The settings a search found, each its title over its section's name, opening its section with its row brought into view; or that it found none. */
function Found(props: { readonly items: readonly SettingsItem[]; readonly onSection: SettingsProps["onSection"] }): ReactNode {
	if (props.items.length === 0) return <p className="px-2 py-4 text-center text-xs text-faint-foreground">No settings found</p>;
	return props.items.map((item) => (
		<button
			key={item.anchor}
			type="button"
			onClick={() => props.onSection(item.section, item.anchor)}
			className={`${LINE} h-auto flex-col items-start gap-0 py-1.5`}
		>
			<span className="text-foreground">{item.title}</span>
			<span className="text-xs text-faint-foreground">{SETTINGS_SECTIONS.find((section) => section.key === item.section)?.label}</span>
		</button>
	));
}

/** Closing commits focused fields before returning to the underlying route. */
export function SettingsDialog(props: SettingsProps): ReactNode {
	const surface = useRef<HTMLDivElement>(null);
	useIntoView(props.at);
	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (open) return;
				const focused = document.activeElement;
				if (focused instanceof HTMLElement && surface.current?.contains(focused)) focused.blur();
				props.onSection(undefined);
			}}
		>
			<DialogPopup
				ref={surface}
				initialFocus={surface}
				className="h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] overflow-hidden rounded-lg sm:h-[calc(100dvh-3rem)] sm:w-[calc(100vw-3rem)] sm:max-h-[60rem] sm:max-w-[80rem] lg:h-[calc(100dvh-5rem)] lg:w-[calc(100vw-5rem)]"
			>
				<DialogTitle className="sr-only">Settings</DialogTitle>
				<header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
					<span className="text-ui text-muted-foreground">Settings /</span>
					<span className="flex-1 text-ui font-medium">{SETTINGS_SECTIONS.find((section) => section.key === props.section)?.label}</span>
					<DialogClose render={<IconButton label="Close settings" />}>
						<X className="size-4" />
					</DialogClose>
				</header>
				<div className="flex min-h-0 flex-1 flex-col sm:flex-row">
					<SettingsNav {...props} />
					<div className="@container/pane min-h-0 min-w-0 flex-1 overflow-y-auto bg-background py-5 scrollbar-even">
						<SectionPage section={props.section} />
					</div>
				</div>
			</DialogPopup>
		</Dialog>
	);
}

/** The page of a section. */
function SectionPage(props: { readonly section: SettingsSection }): ReactNode {
	const { device, set, restore } = useDevice();
	switch (props.section) {
		case "general":
			return <GeneralPage device={device} set={set} restore={restore} />;
		case "models":
			return <ModelsPage />;
		case "threads":
			return <ThreadsPage />;
		case "keybindings":
			return <KeybindingsPage device={device} />;
	}
}
