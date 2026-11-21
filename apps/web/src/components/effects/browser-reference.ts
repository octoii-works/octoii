import { useEffect, useState } from "react";
import type { PageReference } from "@octoii-works/octoii-client";

/** Reads the thread's saved address after its live page is gone. */
export function useBrowserReference(
	thread: string | undefined,
	page: string,
	read: ((page: string) => Promise<PageReference | null>) | undefined,
): PageReference | undefined {
	const [kept, setKept] = useState<{ readonly thread: string; readonly page: string; readonly address: PageReference }>();
	useEffect(() => {
		if (thread === undefined || read === undefined) return;
		let current = true;
		void read(page).then(
			(address) => {
				if (current && address) setKept({ thread, page, address });
			},
			() => undefined,
		);
		return () => {
			current = false;
		};
	}, [thread, page, read]);
	return kept?.thread === thread && kept?.page === page ? kept.address : undefined;
}
