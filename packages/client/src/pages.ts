/**
 * The browser's pages the client follows, as a VNC viewer follows a framebuffer: each handed every frame the browser
 * paints of it, the newest, those painted while it could not take them passed over. The view clicks, types into and
 * scrolls the page, gives the size it is shown at, and lets it go.
 */
import type { Controls } from "./controls.ts";
import type { PageKey, PageMouse, View } from "./protocol.ts";
import type { Requests } from "./requests.ts";
import type { Subscriptions } from "./subscriptions.ts";

/** A frame of a page: its JPEG, its size in the page's CSS pixels, and its number among the page's. */
export type PageFrame = { readonly jpeg: Uint8Array; readonly width: number; readonly height: number; readonly number: number };

/** What a page's view is told: each frame painted of it, the newest. */
export type PageHandlers = { readonly frame: (frame: PageFrame) => void };

/** A page as its view drives it: the person's mouse, keys and pasted text, the size the view shows it at, and letting it go. */
export type PageView = {
	mouse(event: PageMouse): void;
	key(event: PageKey): void;
	text(text: string): void;
	resize(view: View): void;
	stop(): void;
};

/** Follows the page `id` for a view `view` big, told through `handlers`: its newest frame first, then each it paints. */
export function followPage(controls: Controls, requests: Requests, subscriptions: Subscriptions, id: string, view: View, handlers: PageHandlers): PageView {
	const subscription = subscriptions.hold({ type: "page", page: id }, (event, bytes) => {
		if (event.event !== "frame") return;
		handlers.frame({ jpeg: bytes ?? new Uint8Array(), width: event.width, height: event.height, number: event.number });
	});
	controls.send(() => requests.call("browser.resize", { page: id, view }), "resize");
	return {
		mouse: (event) => controls.send(() => requests.call("browser.mouse", { page: id, event }), event.motion === "move" ? "move" : undefined),
		key: (event) => controls.send(() => requests.call("browser.key", { page: id, event })),
		text: (text) => controls.send(() => requests.call("browser.text", { page: id, text })),
		resize: (next) => controls.send(() => requests.call("browser.resize", { page: id, view: next }), "resize"),
		stop: () => {
			controls.stop();
			subscription.stop();
		},
	};
}
