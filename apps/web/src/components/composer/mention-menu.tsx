/**
 * The menu of what a mention may be, over the composer — floating above the page, as a popover does, so nothing the
 * composer stands in cuts it off; the focus kept in the box, Escape and a click elsewhere dismissing it: the thread's terminals and pages and the person's other threads,
 * each by its icon, its name bold and its kind after it; then each file or folder by its path, its name bold and the
 * folder it is in after it; the highlighted one shaded — the box naming it to assistive technology as the arrow keys
 * move it; the list scrolls, its last rows fading out where more follow.
 */
import type { ReactNode, RefObject } from "react";
import { Popover } from "@base-ui/react/popover";
import type { FoundPath } from "@octoii-works/octoii-client";
import { Globe, MessageSquare, SquareTerminal } from "lucide-react";
import { folderOf, nameOf } from "../../core/files.ts";
import type { Choice } from "../../core/mentions.ts";
import type { Thing } from "../../core/things.ts";
import { PathIcon } from "../files/path-icon.tsx";
import type { Mentions } from "./mentions.ts";

/** The id of the menu `menu`'s option at `index`, which the box names as the one highlighted. */
export function optionId(menu: string, index: number): string {
	return `${menu}-${String(index)}`;
}

/** The menu `id` of `mentions`, over `anchor` and as wide, `onChoose` told of what is clicked. */
export function MentionMenu(props: {
	readonly id: string;
	readonly anchor: RefObject<HTMLElement | null>;
	readonly mentions: Mentions;
	readonly onChoose: (chosen: Choice) => void;
}): ReactNode {
	const { choices, searching, highlighted } = props.mentions;
	return (
		<Popover.Root
			open
			modal={false}
			onOpenChange={(open) => {
				if (!open) props.mentions.dismiss();
			}}
		>
			<Popover.Portal>
				<Popover.Positioner
					anchor={props.anchor}
					side="top"
					align="center"
					sideOffset={4}
					collisionPadding={8}
					className="z-[120] w-[calc(var(--anchor-width)-1.5rem)]"
				>
					<Popover.Popup initialFocus={false} finalFocus={false} className="outline-none">
						<div id={props.id} role="listbox" aria-label="What to mention" className="max-h-72 overflow-y-auto floating rounded-lg p-1.5 mask-b-from-85%">
							{choices.map((chosen, index) => (
								<div
									key={"path" in chosen ? `path ${chosen.path.path}` : `${chosen.thing.kind} ${chosen.thing.id}`}
									id={optionId(props.id, index)}
									role="option"
									aria-selected={index === highlighted}
									tabIndex={-1}
									// Chosen as it is pressed, before the box would lose its caret.
									onMouseDown={(event) => {
										event.preventDefault();
										props.onChoose(chosen);
									}}
									onMouseEnter={() => props.mentions.highlight(index)}
									className="flex h-control-sm cursor-default items-center gap-2 rounded-md px-2.5 text-ui aria-selected:bg-accent [&>svg]:size-3.5 [&>svg]:shrink-0 [&>svg]:text-muted-foreground"
								>
									{"path" in chosen ? <PathRow found={chosen.path} /> : <ThingRow thing={chosen.thing} />}
								</div>
							))}
							{searching && choices.length === 0 ? <p className="px-2.5 py-1.5 text-ui text-muted-foreground">Searching…</p> : null}
							{!searching && choices.length === 0 ? <p className="px-2.5 py-1.5 text-ui text-muted-foreground">Nothing found</p> : null}
						</div>
					</Popover.Popup>
				</Popover.Positioner>
			</Popover.Portal>
		</Popover.Root>
	);
}

/** A path's row: its icon, its name, and the folder it is in. */
function PathRow(props: { readonly found: FoundPath }): ReactNode {
	const { path, kind } = props.found;
	const folder = folderOf(path);
	return (
		<>
			<PathIcon path={path} kind={kind} />
			<span className="shrink-0 font-medium text-foreground">{nameOf(path)}</span>
			{folder === "" ? null : <span className="min-w-0 truncate text-muted-foreground">{folder}</span>}
		</>
	);
}

/** Each kind of thing's icon in the menu. */
const THING_ICONS = { thread: MessageSquare, page: Globe, terminal: SquareTerminal } as const;

/** A thing's row: its kind's icon, its name, and its kind. */
function ThingRow(props: { readonly thing: Thing }): ReactNode {
	const { thing } = props;
	const Icon = THING_ICONS[thing.kind];
	return (
		<>
			<Icon />
			<span className="min-w-0 truncate font-medium text-foreground">{thing.name}</span>
			<span className="shrink-0 text-muted-foreground">{thing.kind === "terminal" ? `Terminal ${thing.id}` : thing.kind === "page" ? "Page" : "Thread"}</span>
		</>
	);
}
