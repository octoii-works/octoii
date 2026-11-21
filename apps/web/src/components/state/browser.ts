import type { Client, PagePlacement } from "@octoii-works/octoii-client";
import { failed } from "./notices.ts";
import { pageKey, showSurface } from "./panel.ts";

/** Opens a blank tab or reveals the page for an address in the thread's workspace. */
export function openThreadPage(client: Client, thread: string, address?: string, placement?: PagePlacement): void {
	void client.openPage(thread, address, { width: 1280, height: 800, scale: 1 }, placement).then(
		(id) => showSurface(thread, pageKey(id)),
		(error: unknown) => failed("A page could not be opened", error),
	);
}
