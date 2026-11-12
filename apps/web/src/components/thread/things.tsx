/**
 * The things a message names by a directive, as the page draws them, each kind by its own view — a thread of the
 * person's, a page of the thread's browser, a terminal of the thread's: a chip within a line, a card on a line of its
 * own, opening the thing as it is clicked.
 */
import type { ReactNode } from "react";
import type { Kind } from "../../core/things.ts";
import { PAGE_VIEW } from "../browser/page-thing.tsx";
import { TERMINAL_VIEW } from "../terminal/terminal-thing.tsx";
import type { ThingView } from "./thing-look.tsx";
import { THREAD_VIEW } from "./thread-thing.tsx";

/** Each kind's view. */
const VIEWS: Readonly<Record<Kind, ThingView>> = { thread: THREAD_VIEW, page: PAGE_VIEW, terminal: TERMINAL_VIEW };

/** The chip of the thing of kind `kind` whose id is `id`. */
export function ThingChip(props: { readonly kind: Kind; readonly id: string }): ReactNode {
	const { Chip } = VIEWS[props.kind];
	return <Chip id={props.id} />;
}

/** The card of the thing of kind `kind` whose id is `id`. */
export function ThingCard(props: { readonly kind: Kind; readonly id: string }): ReactNode {
	const { Card } = VIEWS[props.kind];
	return <Card id={props.id} />;
}
