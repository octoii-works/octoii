/**
 * A message's Markdown as the page draws it: its directives — a thing it names, a chip within a line and a card on a line
 * of its own, any other as it was written, and the words a line holds whole however many colons they hold — an image of the web as a link, never
 * fetched, and a bare address ended at the full-width mark after it.
 */
import { describe, expect, it } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { Prose } from "../src/components/thread/markdown.tsx";
import { ContributionsContext } from "../src/components/state/contributions.ts";
import { MessageThreadContext } from "../src/components/thread/message-thread.ts";
import { CONTRIBUTIONS } from "../src/extensions/contributions.ts";

/** `text` drawn as a message's Markdown, its things by the views the extensions give. */
const drawn = (text: string): string =>
	renderToStaticMarkup(
		<ContributionsContext value={CONTRIBUTIONS}>
			<Prose text={text} />
		</ContributionsContext>,
	);

describe("a message's Markdown", () => {
	it("keeps the words a line holds whole, however many colons they hold", () => {
		const html = drawn("Error:failed at 10:30, mode:auto on example.com/a:b");
		expect(html).toContain("Error:failed at 10:30, mode:auto on example.com/a:b");
	});

	it("draws a page the reply points at as its card, and gives any other directive back as it was written", () => {
		expect(drawn('::page{id="p1"}')).toContain("A page no longer open");
		expect(drawn("::page{id=01a1-p1}")).toContain("A page no longer open");
		expect(drawn('::video{src="a.mp4"}')).toContain("<p>::video{src=&quot;a.mp4&quot;}</p>");
		expect(drawn(":::note\nsome words\n:::")).toContain("<p>:::note\nsome words\n:::</p>");
		expect(drawn("::page")).toContain("<p>::page</p>");
	});

	it("draws a thing named within a line as its chip, one on a line of its own as its card, a closed one said to be gone", () => {
		const pages = [
			{ id: "p1", title: "Docs", url: "https://example.com/docs", view: { width: 1280, height: 800 }, loading: false, back: false, forward: false },
		];
		const html = renderToStaticMarkup(
			<ContributionsContext value={CONTRIBUTIONS}>
				<MessageThreadContext value={{ thread: "t1", workspace: undefined, pages, terminals: [] }}>
					<Prose text={"see :page{id=p1} here\n\n::page{id=p1}"} />
				</MessageThreadContext>
			</ContributionsContext>,
		);
		expect(html).toContain("<p>see <button");
		expect(html).toContain("Docs</button> here</p>");
		expect(html).toContain("https://example.com/docs</span>");
		expect(drawn("see :page{id=p2} here")).toContain("a page no longer open</span> here");
	});

	it("gives a name of no thread back as it was written, the chip of a thread drawn where the threads are read", () => {
		expect(drawn("see :thread here")).toContain("see :thread here");
		expect(drawn("a :thread{} of none")).toContain("a :thread{} of none");
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
