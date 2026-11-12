/** Links in a message: what one points at, the link a mention writes, and a line with links by their words. */
import { describe, expect, it } from "bun:test";
import { namesAsWords, linkTo, targetOf } from "../src/core/links.ts";

describe("a link", () => {
	it("points at a path of the machine — from the thread's folder, whole, or a file address — its line let go", () => {
		expect(targetOf("src/main.zig")).toEqual({ path: "src/main.zig" });
		expect(targetOf("src/main.zig#L12-L20")).toEqual({ path: "src/main.zig", lines: { start: 12, end: 20 } });
		expect(targetOf("src/main.zig:12:4")).toEqual({ path: "src/main.zig", lines: { start: 12, end: 12 } });
		expect(targetOf("src/main.zig#L7")).toEqual({ path: "src/main.zig", lines: { start: 7, end: 7 } });
		expect(targetOf("/Users/me/app/README.md")).toEqual({ path: "/Users/me/app/README.md" });
		expect(targetOf("docs/my%20notes.md")).toEqual({ path: "docs/my notes.md" });
		expect(targetOf("file:///Users/me/app/main.zig#L3")).toEqual({ path: "/Users/me/app/main.zig", lines: { start: 3, end: 3 } });
		expect(targetOf("apps/web/")).toEqual({ path: "apps/web/" });
		expect(targetOf("C:\\work\\main.zig")).toEqual({ path: "C:\\work\\main.zig" });
	});

	it("points at a page where it has a scheme, and at the page itself where it is an anchor", () => {
		expect(targetOf("https://example.com/a")).toEqual({ url: "https://example.com/a" });
		expect(targetOf("mailto:me@example.com")).toEqual({ url: "mailto:me@example.com" });
		expect(targetOf("#notes")).toEqual({ url: "#notes" });
	});

	it("is written to a path by its last name, a folder's with its slash, in angle brackets where a space would end it", () => {
		expect(linkTo("packages/siphon/src/protocol/fuzzy.zig", "file")).toBe("[fuzzy.zig](packages/siphon/src/protocol/fuzzy.zig)");
		expect(linkTo("apps/web", "directory")).toBe("[web/](apps/web/)");
		expect(linkTo("docs/my notes.md", "file")).toBe("[my notes.md](<docs/my notes.md>)");
		expect(linkTo("lib/[id].ts", "file")).toBe("[\\[id\\].ts](lib/[id].ts)");
	});

	it("reads by its words in a line that names its message", () => {
		expect(namesAsWords("What does [fuzzy.zig](packages/siphon/src/protocol/fuzzy.zig) score?")).toBe("What does fuzzy.zig score?");
		expect(namesAsWords("see [my notes.md](<docs/my notes.md>) and [\\[id\\].ts](lib/[id].ts)")).toBe("see my notes.md and [id].ts");
		expect(namesAsWords("no links [here]")).toBe("no links [here]");
	});

	it("names a thing a directive names by its kind, a terminal by its number, in a line that names its message", () => {
		expect(namesAsWords("check :terminal{id=2} and :thread{id=01a1-t} on :page{id=p1}")).toBe("check terminal 2 and a thread on a page");
		expect(namesAsWords("10:30 Error:failed")).toBe("10:30 Error:failed");
	});
});
