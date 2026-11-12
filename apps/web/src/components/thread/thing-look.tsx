/**
 * How a thing a message names looks, whatever its kind: its chip within a line, its mark and its name kept together, as
 * a link to a path looks; a chip of a thing not there, quiet; and its card on a line of its own, as wide as it needs up
 * to a measure — its name, and under it, quieter, how the thing stands. What a kind draws, by its id, as either.
 */
import type { ReactNode } from "react";
import { cn } from "../ui/cn.ts";

/** What a kind of thing draws, by its id: its chip, and its card. */
export type ThingView = { readonly Chip: (props: { readonly id: string }) => ReactNode; readonly Card: (props: { readonly id: string }) => ReactNode };

/** A chip's look: its mark and its words, kept together on the line. */
export const CHIP = "inline-flex items-baseline gap-1 whitespace-nowrap";
/** The look of a chip of a thing that is not there. */
export const GONE_CHIP = cn(CHIP, "text-muted-foreground");
/** A chip's icon, set on the words' line. */
export const CHIP_ICON = "size-[1em] shrink-0 self-center";

/** A card's look: a line of its own, as wide as it needs to be up to a measure, opening as it is clicked. */
export const CARD =
	"card my-2 flex w-full max-w-md items-center gap-3 px-3 py-2.5 text-left outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-60";

/** A card's words: its name, and under it, quieter, how the thing stands. */
export function CardWords(props: { readonly name: ReactNode; readonly detail?: ReactNode }): ReactNode {
	return (
		<span className="flex min-w-0 flex-1 flex-col">
			<span className="truncate text-ui font-medium">{props.name}</span>
			{props.detail === undefined ? null : <span className="truncate text-xs text-muted-foreground">{props.detail}</span>}
		</span>
	);
}
