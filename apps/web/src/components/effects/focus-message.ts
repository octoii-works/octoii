import { placePanel } from "../state/panel.ts";

/** Goes to the draft after adding something from a workspace tab. */
export function focusThreadMessage(thread: string): void {
	const panel = document.querySelector<HTMLElement>("[data-workspace-panel]");
	if (panel !== null && (panel.hasAttribute("data-fills") || window.getComputedStyle(panel).position === "absolute")) placePanel("hidden");
	window.requestAnimationFrame(() => {
		document.querySelector<HTMLTextAreaElement>(`textarea[data-thread="${CSS.escape(thread)}"]`)?.focus({ preventScroll: true });
	});
}
