/** A mention as the person writes it, what it may name, and the text with the one chosen in its place. */
import { describe, expect, it } from "bun:test";
import { matchingThings, mentionAt, thingsOf, withMention } from "../src/core/mentions.ts";
import type { Listed } from "@octoii-works/octoii-client";

const listed = (id: string, title: string, began: number): Listed => ({
	id,
	config: { mode: "default", title },
	marks: { pinned: false, archived: false },
	status: { turn: "replied", inbox: [], asks: [], compacting: false, began_at: began },
});

describe("a mention", () => {
	it("is the `@` the caret ends, at the start or after a space, with what follows it", () => {
		expect(mentionAt("@src/ma", 7)).toEqual({ query: "src/ma", start: 0 });
		expect(mentionAt("look at @", 9)).toEqual({ query: "", start: 8 });
		expect(mentionAt("look at @main and", 13)).toEqual({ query: "main", start: 8 });
		expect(mentionAt("mail me@home", 12)).toBeUndefined();
		expect(mentionAt("look at @main and", 17)).toBeUndefined();
	});

	it("is replaced by a link to the path chosen, and a space after it", () => {
		const text = "read @ma please";
		const mention = mentionAt(text, 8);
		if (mention === undefined) throw new Error("no mention");
		const file = withMention(text, mention, 8, { path: { path: "src/main.zig", kind: "file" } });
		expect(file).toEqual({ text: "read [main.zig](src/main.zig) please", caret: 30 });
		expect(withMention("@sr", { query: "sr", start: 0 }, 3, { path: { path: "src", kind: "directory" } })).toEqual({ text: "[src/](src/) ", caret: 13 });
	});

	it("is replaced by the directive that names the thing chosen", () => {
		expect(withMention("see @dev", { query: "dev", start: 4 }, 8, { thing: { kind: "terminal", id: "2", name: "npm run dev" } })).toEqual({
			text: "see :terminal{id=2} ",
			caret: 20,
		});
	});

	it("may name the thread's terminals and pages, then the person's other threads at rest or at work, the latest first", () => {
		const own = listed("t0", "This one", 3);
		const older = listed("t1", "Older", 1);
		const newer = listed("t2", "Newer", 2);
		const things = thingsOf([own, older, newer], [], [], "t0");
		expect(things.map((thing) => thing.id)).toEqual(["t2", "t1"]);
		expect(matchingThings(things, "OLD").map((thing) => thing.name)).toEqual(["Older"]);
	});
});
