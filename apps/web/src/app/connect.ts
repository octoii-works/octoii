/** The page's HTTP and watch client, using the same platform session for both. */
import { type Client, type Clock, createClient } from "@octoii-works/octoii-client";
import { failed } from "../components/state/notices.ts";
import type { Reach } from "../components/state/reach.ts";

const clock: Clock = {
	now: () => Date.now(),
	setTimer: (callback, ms) => {
		const timer = setTimeout(callback, ms);
		return () => clearTimeout(timer);
	},
};

/** A client of the node `reach` reaches. */
export function connect(reach: Reach): Client {
	return createClient({
		open: reach.open,
		send: reach.send,
		randomUUID: () => crypto.randomUUID(),
		clock,
		report: (error) => failed("Something asked of the node failed", error),
	});
}
