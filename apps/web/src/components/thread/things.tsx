/**
 * The things a message names by a directive, as the page draws them, each kind by the view its extension gives — a
 * thread of the person's, a page of the thread's browser, a terminal of the thread's: a chip within a line, a card on
 * a line of its own, opening the thing as it is clicked; a kind no extension draws, by its kind and its id, quiet.
 */
import { type ReactNode, use } from "react";
import type { Kind } from "../../core/things.ts";
import { ContributionsContext } from "../state/contributions.ts";
import { CARD, CardWords, GONE_CHIP } from "./thing-look.tsx";

/** The chip of the thing of kind `kind` whose id is `id`. */
export function ThingChip(props: { readonly kind: Kind; readonly id: string }): ReactNode {
	const view = use(ContributionsContext).things[props.kind];
	if (view === undefined) return <span className={GONE_CHIP}>{`${props.kind} ${props.id}`}</span>;
	const { Chip } = view;
	return <Chip id={props.id} />;
}

/** The card of the thing of kind `kind` whose id is `id`. */
export function ThingCard(props: { readonly kind: Kind; readonly id: string }): ReactNode {
	const view = use(ContributionsContext).things[props.kind];
	if (view === undefined)
		return (
			<button type="button" disabled className={CARD}>
				<CardWords name={`${props.kind} ${props.id}`} />
			</button>
		);
	const { Card } = view;
	return <Card id={props.id} />;
}
