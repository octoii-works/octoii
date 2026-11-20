/**
 * The terminals the client follows, as views of what the node draws, as tmux's clients attach to its panes: each
 * subscribed to from the offset its view stands at — the bytes it took of what the terminal wrote — its screen drawn
 * anew first where it stands nowhere, or where the node no longer holds what lay between; and so again as each
 * connection opens, from where it stood. The view types into the terminal, gives the size it is shown at, and lets it go.
 */
import type { Controls } from "./controls.ts";
import type { Cells } from "./protocol.ts";
import type { Requests } from "./requests.ts";
import type { Subscriptions } from "./subscriptions.ts";

/** What a terminal's view is told: its screen drawn anew; what it wrote after. */
export type TerminalHandlers = {
	readonly screen: (drawn: Uint8Array) => void;
	readonly output: (bytes: Uint8Array) => void;
};

/** A terminal as its view drives it: what the person types, the size the view shows it at, and letting it go. */
export type TerminalView = {
	write(data: string): void;
	resize(size: Cells): void;
	stop(): void;
};

/** Follows the terminal `id` for a view `size` big, told through `handlers`: its screen drawn first, then what it writes. */
export function followTerminal(
	controls: Controls,
	requests: Requests,
	subscriptions: Subscriptions,
	id: string,
	size: Cells,
	handlers: TerminalHandlers,
): TerminalView {
	const subscription = subscriptions.hold({ type: "terminal", terminal: id }, (event, bytes) => {
		const drawn = bytes ?? new Uint8Array();
		if (event.event === "screen") {
			subscription.topic = { type: "terminal", terminal: id, from: event.through };
			handlers.screen(drawn);
		} else if (event.event === "output") {
			subscription.topic = { type: "terminal", terminal: id, from: event.offset + drawn.length };
			handlers.output(drawn);
		}
	});
	controls.send(() => requests.call("terminals.resize", { terminal: id, size }), "resize");
	return {
		write: (data) => controls.send(() => requests.call("terminals.write", { terminal: id, data })),
		resize: (next) => controls.send(() => requests.call("terminals.resize", { terminal: id, size: next }), "resize"),
		stop: () => {
			controls.stop();
			subscription.stop();
		},
	};
}
