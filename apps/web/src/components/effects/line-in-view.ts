/**
 * A line of a file's view brought into view as the view opens on it: read on to it where it lies past the lines read,
 * then scrolled to the middle of the view, once — as the person scrolls on, the view stays where they put it.
 */
import { type RefCallback, useEffect, useRef } from "react";

/** The ref of the view in which line `line` is brought into view, `read` lines of it read so far, `readMore` reading the next. */
export function useLineInView(line: number | undefined, read: number, readMore: (() => void) | undefined): RefCallback<HTMLDivElement> {
	const view = useRef<HTMLDivElement | null>(null);
	const shown = useRef<number | undefined>(undefined);
	useEffect(() => {
		if (line === undefined || shown.current === line) return;
		if (line > read && readMore !== undefined) {
			readMore();
			return;
		}
		// The code is drawn in a shadow tree of the view's, a frame after the lines arrive.
		const frame = requestAnimationFrame(() => {
			const host = view.current;
			const root = host?.querySelector("*")?.shadowRoot ?? host;
			const row = root?.querySelector(`[data-line="${String(line)}"]`);
			if (row === null || row === undefined) return;
			row.scrollIntoView({ block: "center" });
			shown.current = line;
		});
		return () => cancelAnimationFrame(frame);
	}, [line, read, readMore]);
	return (element) => {
		view.current = element;
	};
}
