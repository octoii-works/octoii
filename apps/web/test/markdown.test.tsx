/**
 * A message's Markdown as the page draws it: its directives — a page of the thread's as its card, any other as it was
 * written, and the words a line holds whole however many colons they hold — an image of the web as a link, never
 * fetched, and a bare address ended at the full-width mark after it.
 */
import { describe, expect, it } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { Prose } from "../src/components/thread/markdown.tsx";

const drawn = (text: string): string => renderToStaticMarkup(<Prose text={text} />);

describe("a message's Markdown", () => {
	it("keeps the words a line holds whole, however many colons they hold", () => {
		const html = drawn("Error:failed at 10:30, mode:auto on example.com/a:b");
		expect(html).toContain("Error:failed at 10:30, mode:auto on example.com/a:b");
	});

	it("draws a page the reply points at as its card, and gives any other directive back as it was written", () => {
		expect(drawn('::page{id="p1"}')).toContain("A page no longer open");
		expect(drawn('::video{src="a.mp4"}')).toContain("<p>::video{src=&quot;a.mp4&quot;}</p>");
		expect(drawn(":::note\nsome words\n:::")).toContain("<p>:::note\nsome words\n:::</p>");
		expect(drawn("::page")).toContain("<p>::page</p>");
	});

	it("ends a bare address at the full-width mark after it, keeping a path of Chinese words whole", () => {
		const html = drawn("监听 http://localhost:4317/，可用 PORT 改端口；见 https://zh.wikipedia.org/wiki/章鱼。");
		expect(html).toContain('href="http://localhost:4317/"');
		expect(html).toContain("</a>，可用 PORT 改端口");
		expect(html).toContain('href="https://zh.wikipedia.org/wiki/%E7%AB%A0%E9%B1%BC"');
		expect(html).toContain("</a>。");
	});

	it("draws an image of the web as a link to it, never fetching it", () => {
		const html = drawn("![the logo](https://example.com/logo.png)");
		expect(html).toContain('href="https://example.com/logo.png"');
		expect(html).toContain("the logo");
		expect(html).not.toContain("<img");
	});
});
