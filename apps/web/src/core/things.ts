/**
 * The things a message may name past paths — a thread of the person's, a page of the thread's browser, a terminal of
 * the thread's — by their kind and their id, a terminal's its number in the thread: the directive that names one,
 * `:thread{id=…}`, which the page draws as its chip and the model reads by its id; a thing as a mention offers it, by
 * its name; and a directive written as words, as a line that names a message shows it.
 */
import type { PageListed } from "@octoii-works/octoii-client";

/** The kinds of thing a directive names. */
export const KINDS = ["thread", "page", "terminal"] as const;

/** A kind of thing a directive names. */
export type Kind = (typeof KINDS)[number];

/** Whether `name` is a kind of thing a directive names. */
export function isKind(name: string): name is Kind {
	return KINDS.some((kind) => kind === name);
}

/** A thing a mention may name: its kind, its id, and the name it is offered by. */
export type Thing = { readonly kind: Kind; readonly id: string; readonly name: string };

/** The directive that names `thing` within a line. */
export function directiveOf(thing: Thing): string {
	return `:${thing.kind}{id=${thing.id}}`;
}

/** What a page is called: its title, or its address where it has none. */
export function pageName(page: PageListed): string {
	return page.title === "" ? page.url : page.title;
}

/** Each directive that names a thing within a line, its kind and its id. */
const DIRECTIVES = /:(thread|page|terminal)\{id=([^\s{}]+)\}/g;

/** `text` with each thing a directive names written as words: a thread, a page, a terminal by its number. */
export function thingsAsWords(text: string): string {
	return text.replace(DIRECTIVES, (_directive, kind: string, id: string) => (kind === "terminal" ? `terminal ${id}` : `a ${kind}`));
}
