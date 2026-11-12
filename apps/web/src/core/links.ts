/**
 * Links in a message, as markdown writes them, the person's and the model's alike: what one points at — a path of the
 * machine, from the thread's folder or whole, with the lines named after it, or a page of the web; the link a mention writes to
 * a path; and a line with each link by its words, as a title shows it.
 */
import type { FileKind } from "@octoii-works/octoii-client";
import { nameOf } from "./files.ts";

/** Lines of a file, from `start` to `end`, the first numbered 1. */
export type Lines = { readonly start: number; readonly end: number };

/** What a link points at: a path of the machine — a folder's ending in a slash — and the lines of it named, where any are; or a page, by its address. */
export type LinkTarget = { readonly path: string; readonly lines?: Lines } | { readonly url: string };

/** A scheme, as `https:` begins an address: two letters at least, which a drive's, as `C:` begins a path, is not. */
const SCHEME = /^[a-z][a-z0-9+.-]+:/i;

/** A line named after a path, as `#L12`, `#L12-L20` or `:12:4`. */
const LINE = /(#L\d+(-L?\d+)?|:\d+(:\d+)?)$/;

/** What `href` points at. */
export function targetOf(href: string): LinkTarget {
	if (href.startsWith("file:")) return pathOfFileUrl(href);
	if (SCHEME.test(href) || href.startsWith("#")) return { url: href };
	return pathTarget(decoded(href));
}

/** The path `named` names, and the lines after it, where it names any: `#L12`, `#L12-L20`, `:12` or `:12:4`. */
function pathTarget(named: string): LinkTarget {
	const found = LINE.exec(named);
	if (found === null) return { path: named };
	const [start = 0, end] = (found[0].match(/\d+/g) ?? []).map(Number);
	const lines = found[0].startsWith("#") ? { start, end: end ?? start } : { start, end: start };
	return { path: named.slice(0, found.index), lines };
}

/** The path a `file:` address names; the address itself where it reads as none. */
function pathOfFileUrl(href: string): LinkTarget {
	try {
		const url = new URL(href);
		return pathTarget(decoded(url.pathname) + url.hash);
	} catch {
		return { url: href };
	}
}

/** `href` with its escaped characters as they are, where it holds any. */
function decoded(href: string): string {
	try {
		return decodeURI(href);
	} catch {
		return href;
	}
}

/**
 * The markdown link a mention writes to a path of the thread's folder: its last name, a folder's with its slash, and
 * its path, in angle brackets where a space or a bracket would end it.
 */
export function linkTo(path: string, kind: FileKind): string {
	const slash = kind === "directory" ? "/" : "";
	const words = `${nameOf(path)}${slash}`.replace(/[[\]\\]/g, "\\$&");
	const written = `${path}${slash}`;
	const destination = /[\s()<>]/.test(written) ? `<${written.replace(/[<>]/g, "\\$&")}>` : written;
	return `[${words}](${destination})`;
}

/** Each markdown link in a line: its words, escapes and all, then where it points. */
const LINKS = /\[((?:\\.|[^\]\\])*)\]\((?:<(?:\\.|[^>\\])*>|[^)\s]*)\)/g;

/** `text` with each link by its words, as a line that names the message shows it. */
export function linksAsWords(text: string): string {
	return text.replace(LINKS, (_link, words: string) => words.replace(/\\(.)/g, "$1"));
}
