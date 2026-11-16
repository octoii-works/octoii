/** A saved selection remains reviewable and becomes bounded text plus a normal persisted input image. */
import { describe, expect, it } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { ElementFeedbackCards } from "../src/components/composer/element-feedback.tsx";
import { contentOf } from "../src/core/attachments.ts";
import { elementEvidence, elementSummary } from "../src/core/element-feedback.ts";
import { capturedElement } from "./fixtures.ts";

describe("browser element feedback", () => {
	it("sends the user's description followed by captured context and its own saved image", () => {
		const capture = capturedElement();
		const content = contentOf("Fix the button", [], [capture]);
		expect(content).toEqual([
			{ type: "input_text", text: "Fix the button" },
			{ type: "input_text", text: elementEvidence(capture) },
			{ type: "input_image", file: capture.screenshot, detail: "auto" },
		]);
		expect(elementEvidence(capture)).toContain("Historical browser context");
		expect(elementEvidence(capture)).toContain("Re-read the live page before using any selector or coordinates");
		expect(elementSummary(capture)).toBe("button: Submit");
	});

	it("bounds every text field and sends only the captured facts, never extra DOM or form data", () => {
		const capture = capturedElement();
		const long = "界".repeat(10_000);
		const data = {
			...capture,
			thread: long,
			page: long,
			url: long,
			document: long,
			viewport: { ...capture.viewport, cookie: "private" },
			point: { ...capture.point, secret: "private" },
			element: {
				...capture.element,
				tag: long,
				role: long,
				label: long,
				text: long,
				selector: long,
				html: "private",
				value: "private",
				rect: { ...capture.element.rect, secret: "private" },
			},
		};
		const evidence = elementEvidence(data);
		const json = JSON.parse(evidence.slice(evidence.indexOf("\n") + 1));
		expect(json.url.length).toBe(2048);
		expect(json.document.length).toBe(256);
		expect(json.thread.length).toBe(128);
		expect(json.element.text.length).toBe(512);
		expect(json.element.selector.length).toBe(512);
		expect(evidence).not.toContain("private");
		expect(evidence.length).toBeLessThan(5000);
	});

	it("shows an accessible removable card, reading the screenshot from its original thread with the existing image viewer", () => {
		const html = renderToStaticMarkup(<ElementFeedbackCards feedback={[capturedElement()]} onRemove={() => undefined} />);
		expect(html).toContain("Selected element");
		expect(html).toContain("button: Submit");
		expect(html).toContain("https://example.com/form");
		expect(html).toContain("/threads/thread-A/files/captured");
		expect(html).toContain('aria-label="Remove selected button: Submit"');
		expect(html).toContain("object-contain");
		expect(html).not.toContain("/machine");
	});
});
