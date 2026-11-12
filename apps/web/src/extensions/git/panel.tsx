/**
 * Git in the panel: the Diff view of the thread's changes, where it is open; and `+`'s way to open it.
 */
import { FileDiff } from "lucide-react";
import type { Opening, Surface } from "../../components/panel/right-panel.tsx";
import { showSurface, useThreadPanel } from "../../components/state/panel.ts";
import { DiffSurface } from "./diff-surface.tsx";

/**
 * The panel's part of git for the thread `thread`, working in `folder`, its turns standing at `revision`; nothing where
 * git is switched off (`on`).
 */
export function useGitPanel(
	thread: string,
	folder: string | undefined,
	revision: string,
	on: boolean,
): { readonly surfaces: readonly Surface[]; readonly openings: readonly Opening[] } {
	const place = useThreadPanel(thread);
	if (!on) return { surfaces: [], openings: [] };
	const surfaces: readonly Surface[] = place.views.includes("diff")
		? [{ key: "diff", icon: <FileDiff />, label: "Diff", body: <DiffSurface thread={thread} folder={folder} revision={revision} /> }]
		: [];
	return { surfaces, openings: [{ label: "Diff", icon: FileDiff, letter: "D", open: () => showSurface(thread, "diff") }] };
}
