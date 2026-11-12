/**
 * The `browser` tool as the page shows a call of it, by its action: a page opened or gone to, read or looked at, acted
 * on, run in or waited for.
 */
import { type Given, toolView, type ToolView, type Verbs } from "../../core/tools.ts";

/** Each kind of the browser's actions, its verbs, and what it works on. */
const ACTIONS: Readonly<Record<string, { readonly verbs: Verbs; readonly subject?: (given: Given) => string | undefined }>> = {
	open: { verbs: ["Open", "Opening", "Opened"], subject: (given) => given.url },
	navigate: { verbs: ["Go to", "Going to", "Went to"], subject: (given) => given.url },
	read: { verbs: ["Read the page", "Reading the page", "Read the page"] },
	look: { verbs: ["Look at the page", "Looking at the page", "Looked at the page"] },
	evaluate: { verbs: ["Run in the page", "Running in the page", "Ran in the page"], subject: (given) => given.expression },
	wait: { verbs: ["Wait for the page", "Waiting for the page", "Waited for the page"], subject: (given) => given.text ?? given.url },
	tabs: { verbs: ["List the pages", "Listing the pages", "Listed the pages"] },
	switch: { verbs: ["Switch pages", "Switching pages", "Switched pages"] },
	close: { verbs: ["Close a page", "Closing a page", "Closed a page"] },
	back: { verbs: ["Go back", "Going back", "Went back"] },
	forward: { verbs: ["Go forward", "Going forward", "Went forward"] },
	reload: { verbs: ["Reload the page", "Reloading the page", "Reloaded the page"] },
	click: { verbs: ["Click on the page", "Clicking on the page", "Clicked on the page"] },
	type: { verbs: ["Type on the page", "Typing on the page", "Typed on the page"], subject: (given) => given.text },
	press: { verbs: ["Press a key", "Pressing a key", "Pressed a key"] },
	scroll: { verbs: ["Scroll the page", "Scrolling the page", "Scrolled the page"] },
};

/** Any other action on the page: a hover, a selection, a drag, an upload, a dialog answered, a console read. */
const ACT: Verbs = ["Act on the page", "Acting on the page", "Acted on the page"];

/** The browser extension's tools. */
export const BROWSER_TOOLS: Readonly<Record<string, ToolView>> = {
	browser: toolView({
		work: "browse",
		verbs: (given) => ACTIONS[given.action ?? ""]?.verbs ?? ACT,
		subject: (given) => ACTIONS[given.action ?? ""]?.subject?.(given),
	}),
};
