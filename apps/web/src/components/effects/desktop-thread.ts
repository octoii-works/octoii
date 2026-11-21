import { useLayoutEffect } from "react";

/** Allows the node's local desktop to render new pages for the thread on screen. */
export function useDesktopThread(thread: string | null, session: string | null): void {
	useLayoutEffect(() => {
		const desktop = window.octoiiBrowser;
		desktop?.thread(thread, session);
		return () => desktop?.thread(null, session);
	}, [thread, session]);
}
