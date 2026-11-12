/**
 * The terminal as the interface shows it: the sixteen colours programs pick by number, one set for a light page and one
 * for a dark, its font's size, how a terminal is named on its tab, and a command line's end as its thread is told it.
 */
import type { TerminalListed } from "@octoii-works/octoii-client";
import { openingReminder } from "./reminders.ts";

/** The sixteen ANSI colours on a light page: black, red, green, yellow, blue, magenta, cyan, white, then their bright forms. */
export const ANSI_LIGHT: readonly number[] = [
	0x24292f, 0xc4362e, 0x2a7f3a, 0x8f6400, 0x2f5fc4, 0x8a3fb4, 0x137a83, 0x8c959f, 0x57606a, 0xdc4a40, 0x36994a, 0xae7c00, 0x3d78e6, 0xa452d4, 0x1894a0,
	0xb4bcc6,
];

/** The sixteen ANSI colours on a dark page. */
export const ANSI_DARK: readonly number[] = [
	0x3a3f47, 0xf0767a, 0x8bd08a, 0xf2c56b, 0x78a6f5, 0xc595ea, 0x6acdd4, 0xc9ced6, 0x5f6773, 0xff8d90, 0xa6e3a2, 0xffd88a, 0x9bc0ff, 0xdbb0ff, 0x8de3e8,
	0xf3f5f7,
];

/** The terminal's font size, in CSS pixels. */
export const TERMINAL_FONT_SIZE = 12;

/** A terminal's name on its tab: the title its program set, the command line the model runs in it, or its number in the thread. */
export function terminalName(terminal: TerminalListed): string {
	const title = terminal.title.trim();
	if (title !== "") return title;
	return terminal.line ?? `Terminal ${String(terminal.number)}`;
}

/** A command line the model ran in a terminal, ended of itself: the terminal's number, the line, how it ended, whether it went well, and its last screen. */
export type Ended = { readonly number: number; readonly line: string; readonly how: string; readonly ok: boolean; readonly screen: string };

/**
 * The end the thread was told in `text`, as the terminals write it — Octoii's words `terminal 2, which ran `npm run dev`,
 * ended: exit 1` opening it, then the last screen; none where `text` tells no end.
 */
export function endedOf(text: string): Ended | undefined {
	const note = openingReminder(text);
	const found = note === undefined ? null : /^terminal (\d+), which ran `(.*)`, ended: (.+)$/.exec(note);
	if (found === null) return undefined;
	const screen = text
		.slice(text.indexOf("</system-reminder>") + "</system-reminder>".length)
		.replace(/^\n/, "")
		.split("\n");
	const [, number = "0", line = "", how = ""] = found;
	return { number: Number(number), line, how: how.replace(/^ended by /, ""), ok: how === "exit 0", screen: screen.join("\n") };
}
