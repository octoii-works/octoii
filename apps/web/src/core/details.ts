/**
 * A thread's details as its card tells them: how full its window is — the tokens its model's latest request held
 * against the window its model takes — and what its requests cost and how much of what they read was cached.
 */
import type { ThreadUsage as Usage, Turn } from "@octoii-works/octoii-client";

/** How full a window is: the tokens it holds now, and the most its model takes. */
export type Context = { readonly used: number; readonly window: number };

/**
 * How full the window is that the thread's next request asks on, its model taking `window` tokens: what its latest
 * request read and wrote, which the next reads again; none before any request told what it used, or where the window
 * is not known.
 */
export function contextOf(turns: readonly Turn[], window: number | undefined): Context | undefined {
	if (window === undefined || window <= 0) return undefined;
	for (const turn of turns.toReversed()) {
		for (const step of turn.steps.toReversed()) {
			const response = step.reply?.response;
			const used = response?.usage;
			if (response !== undefined && used !== undefined && used !== null) {
				return { used: (response.context_tokens ?? used.input_tokens) + used.output_tokens, window };
			}
		}
	}
	return undefined;
}

/** The share of what a thread read that came from the prompt cache, in whole percent; none before it read anything. */
export function cachedShare(usage: Usage): number | undefined {
	return usage.input_tokens === 0 ? undefined : Math.round((usage.cached_tokens / usage.input_tokens) * 100);
}

/** How full a window stands: with room to spare, filling, or near its end. */
export type Fullness = "room" | "filling" | "full";

/** How full `context` stands: filling past seven tenths of its window, near its end past nine. */
export function fullnessOf(context: Context): Fullness {
	const share = context.used / context.window;
	return share >= 0.9 ? "full" : share >= 0.7 ? "filling" : "room";
}
