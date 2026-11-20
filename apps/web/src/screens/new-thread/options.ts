/** Unsent draft options share the same page lifetime as composer drafts, isolated by node session and project entry. */
import { useSyncExternalStore } from "react";
import type { Mode, ReasoningEffort } from "@octoii-works/octoii-client";
import type { WorktreeBase } from "../../extensions/git/new-worktree.tsx";

type Options = {
	readonly model?: string | undefined;
	readonly mode?: Mode | undefined;
	readonly effort?: ReasoningEffort | undefined;
	readonly worktree?: { readonly base?: WorktreeBase | undefined } | undefined;
};
const empty: Options = {};
const options = new Map<string, Options>();
const listeners = new Set<() => void>();
function subscribe(changed: () => void): () => void {
	listeners.add(changed);
	return () => listeners.delete(changed);
}
export function useNewThreadOptions(session: string | undefined, project: string | undefined) {
	const key = JSON.stringify([session, project]);
	const value = useSyncExternalStore(subscribe, () => options.get(key) ?? empty);
	return {
		value,
		change: (change: Partial<Options>) => {
			options.set(key, { ...options.get(key), ...change });
			for (const changed of listeners) changed();
		},
	};
}
