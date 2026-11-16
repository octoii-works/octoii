/** Browser evidence chosen by the person: bounded text and the saved screenshot, using normal message parts. */
import type { ElementFeedback, InputContent } from "@octoii-works/octoii-client";

/** A short label for a saved selection, independent of the page's current contents. */
export function elementSummary(feedback: ElementFeedback): string {
	const element = feedback.element;
	const kind = element.role || element.tag;
	const words = element.label || element.text || element.selector;
	return bounded(words ? `${kind}: ${words}` : kind, 160);
}

/** Captured facts describe a past document; they never act as a locator on a live page. */
export function elementEvidence(feedback: ElementFeedback): string {
	const element = feedback.element;
	return `Historical browser context captured from the element the user selected; its saved screenshot follows. Re-read the live page before using any selector or coordinates.\n${JSON.stringify(
		{
			thread: bounded(feedback.thread, 128),
			page: bounded(feedback.page, 128),
			url: bounded(feedback.url, 2048),
			document: bounded(feedback.document, 256),
			captured_at: feedback.captured_at,
			viewport: {
				width: feedback.viewport.width,
				height: feedback.viewport.height,
				scale: feedback.viewport.scale,
				scroll_x: feedback.viewport.scroll_x,
				scroll_y: feedback.viewport.scroll_y,
			},
			point: { x: feedback.point.x, y: feedback.point.y },
			element: {
				tag: bounded(element.tag, 64),
				...(element.role === undefined ? {} : { role: bounded(element.role, 128) }),
				...(element.label === undefined ? {} : { label: bounded(element.label, 256) }),
				...(element.text === undefined ? {} : { text: bounded(element.text, 512) }),
				...(element.selector === undefined ? {} : { selector: bounded(element.selector, 512) }),
				rect: { x: element.rect.x, y: element.rect.y, width: element.rect.width, height: element.rect.height },
			},
		},
	)}`;
}

/** A feedback item's two adjacent parts keep the screenshot with its own captured context. */
export function elementContent(feedback: ElementFeedback): readonly InputContent[] {
	return [
		{ type: "input_text", text: elementEvidence(feedback) },
		{ type: "input_image", file: feedback.screenshot, detail: "auto" },
	];
}

function bounded(text: string, most: number): string {
	const chars = [...text];
	return chars.length <= most ? text : `${chars.slice(0, most - 1).join("")}…`;
}
