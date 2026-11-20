import { useEffect, useState } from "react";

/** Room for the floating card and its margins alongside at least 32.5rem of conversation. */
const DETAILS_MIN_REM = 52;

export function useDetailsRoom() {
	const [pane, setPane] = useState<HTMLDivElement | null>(null);
	const [room, setRoom] = useState(false);
	useEffect(() => {
		if (pane === null) return;
		const measure = (): void => {
			const rem = Number.parseFloat(window.getComputedStyle(pane.ownerDocument.documentElement).fontSize) || 16;
			setRoom(pane.clientWidth >= DETAILS_MIN_REM * rem);
		};
		const observer = new ResizeObserver(measure);
		observer.observe(pane);
		measure();
		return () => observer.disconnect();
	}, [pane]);
	return { room, pane: setPane };
}
