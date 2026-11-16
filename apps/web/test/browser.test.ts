/** The browser as the interface shows it. */
import { describe, expect, it } from "bun:test";
import { addressOf, editingOf, modifiersOf, PageInputGate, pageName, pickViewOf, shownAddress, typedBy } from "../src/core/browser.ts";

describe("the local element picker", () => {
	it("uses the drawn frame's CSS size and the node's scale, waiting until frame and view agree", () => {
		expect(pickViewOf({ width: 800, height: 600 }, { width: 800, height: 600, scale: 2 })).toEqual({ width: 800, height: 600, scale: 2 });
		expect(pickViewOf({ width: 800, height: 600 }, { width: 900, height: 600, scale: 2 })).toBeUndefined();
		expect(pickViewOf(undefined, { width: 800, height: 600 })).toBeUndefined();
		expect(pickViewOf({ width: 0, height: 0 }, { width: 0, height: 0 })).toBeUndefined();
	});

	it("swallows the entire selection gesture and selects only on its release", () => {
		const input = new PageInputGate();
		input.setMode("pick");
		expect(input.mouseDown(0)).toBe("block");
		expect(input.forwardsPointer()).toBe(false);
		expect(input.mouseUp(0)).toBe("select");
		input.setMode("capturing");
		expect(input.mouseDown(0)).toBe("block");
		expect(input.mouseUp(0)).toBe("block");
		expect(input.forwardsPointer()).toBe(false);
	});

	it("does not forward the rest of a swallowed gesture after Escape, cancellation or a new pick", () => {
		const input = new PageInputGate();
		input.setMode("pick");
		input.mouseDown(0);
		input.keyDown("Escape");
		input.setMode("browse");
		expect(input.forwardsPointer()).toBe(false);
		expect(input.keyUp("Escape")).toBe("block");
		input.setMode("pick");
		expect(input.mouseUp(0)).toBe("block");
		input.setMode("browse");
		expect(input.forwardsPointer()).toBe(true);
		expect(input.mouseDown(0)).toBe("forward");
		expect(input.mouseUp(0)).toBe("forward");
	});

	it("keeps keys local through capture completion, then resumes normal input", () => {
		const input = new PageInputGate();
		input.setMode("pick");
		expect(input.keyDown("KeyA")).toBe("block");
		input.setMode("capturing");
		expect(input.keyDown("KeyV")).toBe("block");
		input.setMode("browse");
		expect(input.keyUp("KeyA")).toBe("block");
		expect(input.keyUp("KeyV")).toBe("block");
		expect(input.keyDown("KeyB")).toBe("forward");
		expect(input.keyUp("KeyB")).toBe("forward");
	});
});

describe("the address bar", () => {
	it("takes an address as it is, gives a host its scheme, and searches for the rest", () => {
		expect(addressOf("https://example.com/a")).toBe("https://example.com/a");
		expect(addressOf("about:blank")).toBe("about:blank");
		expect(addressOf("example.com/docs")).toBe("https://example.com/docs");
		expect(addressOf("localhost:5173")).toBe("http://localhost:5173");
		expect(addressOf("127.0.0.1:8080/x")).toBe("http://127.0.0.1:8080/x");
		expect(addressOf("zig std io")).toBe("https://duckduckgo.com/?q=zig%20std%20io");
		expect(addressOf("octoii")).toBe("https://duckduckgo.com/?q=octoii");
	});

	it("shows no address for a blank page", () => {
		expect(shownAddress("about:blank")).toBe("");
		expect(shownAddress("https://a.dev/")).toBe("https://a.dev/");
	});
});

describe("a page on its tab", () => {
	const page = { id: "p", url: "https://docs.example.com/a", title: "", view: { width: 800, height: 600 }, loading: false, back: false, forward: false };
	it("is named by its title, else its host, else New tab", () => {
		expect(pageName({ ...page, title: "Docs" })).toBe("Docs");
		expect(pageName(page)).toBe("docs.example.com");
		expect(pageName({ ...page, url: "about:blank" })).toBe("New tab");
	});
});

describe("a key and its modifiers", () => {
	it("are told as the browser's protocol counts them", () => {
		expect(modifiersOf({ altKey: true, ctrlKey: false, metaKey: true, shiftKey: true })).toBe(13);
		expect(typedBy("a")).toBe("a");
		expect(typedBy("中")).toBe("中");
		expect(typedBy("Enter")).toBe("\r");
		expect(typedBy("ArrowLeft")).toBeUndefined();
	});
});

describe("a shortcut in a page", () => {
	const plain = { altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
	it("stands for the editing Cmd or Ctrl binds, and nothing else does", () => {
		expect(editingOf({ ...plain, metaKey: true, code: "KeyA" })).toEqual(["selectAll"]);
		expect(editingOf({ ...plain, ctrlKey: true, code: "KeyZ" })).toEqual(["undo"]);
		expect(editingOf({ ...plain, metaKey: true, shiftKey: true, code: "KeyZ" })).toEqual(["redo"]);
		expect(editingOf({ ...plain, code: "KeyA" })).toBeUndefined();
		expect(editingOf({ ...plain, metaKey: true, altKey: true, code: "KeyA" })).toBeUndefined();
		expect(editingOf({ ...plain, metaKey: true, code: "KeyQ" })).toBeUndefined();
	});
});
