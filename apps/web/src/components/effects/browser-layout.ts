import { useCallback, useState } from "react";
import type { Nothing, PageListed, PagePresentation, ViewportMode } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { VIEWPORT_MARGIN, type ViewportSize, type ViewportZoom, viewportScale } from "../../core/browser-viewport.ts";
import { failed } from "../state/notices.ts";

export type ViewportPreview = { readonly size: ViewportSize; readonly view: ViewportSize; readonly scale: number; readonly margin: number };
type Adjustment = { readonly viewport: ViewportMode; readonly size: ViewportSize } | { readonly zoom: number };

function applied(page: PageListed, adjustment: Adjustment): boolean {
	if ("zoom" in adjustment) return page.zoom === adjustment.zoom;
	if (page.viewport !== adjustment.viewport) return false;
	return adjustment.viewport === "fill" || (page.viewport_size.width === adjustment.size.width && page.viewport_size.height === adjustment.size.height);
}

/** The page's layout is shared through the node. Preview scaling belongs to this pane. */
export function useBrowserLayout(page: PageListed, presentation: PagePresentation, driving: boolean) {
	const client = useClient();
	const room = useViewportRoom();
	const [zoom, setZoom] = useState<ViewportZoom>("fit");
	const [pending, setPending] = useState<Adjustment>();
	const saving = pending !== undefined && !applied(page, pending) && page.control === "you";
	if (pending !== undefined && !saving) setPending(undefined);
	const resizable = page.host === "headless" || presentation === "native";
	const fixed = page.viewport === "fixed" || !resizable || page.control !== "you";
	const fit = viewportScale(page.viewport_size, room.size, "fit");
	const editable = resizable && driving && page.dialog === undefined;
	const preview: ViewportPreview | undefined = fixed
		? {
				size: page.viewport_size,
				view: page.view,
				scale: viewportScale(page.viewport_size, room.size, zoom),
				margin: VIEWPORT_MARGIN,
			}
		: page.zoom === 100
			? undefined
			: { size: page.viewport_size, view: page.view, scale: 1, margin: 0 };
	const change = (adjustment: Adjustment, send: () => Promise<Nothing>, note: string): void => {
		if (!editable || saving || applied(page, adjustment)) return;
		setPending(adjustment);
		void send().catch((error: unknown) => {
			setPending((current) => (current === adjustment ? undefined : current));
			failed(note, error);
		});
	};
	const resize = (viewport: ViewportMode, size: ViewportSize): void =>
		change(
			{ viewport, size },
			() =>
				client.call("browser.resize", {
					page: page.id,
					presentation,
					viewport,
					view: { width: size.width, height: size.height, scale: window.devicePixelRatio },
				}),
			"The viewport could not be changed",
		);
	const zoomPage = (percent: number): void =>
		change({ zoom: percent }, () => client.call("browser.zoom", { page: page.id, percent, presentation }), "Page zoom could not be changed");
	return { room, resizable, fixed, fit, preview, editable, saving, zoom, setZoom, resize, zoomPage };
}

/** Measures the room around a viewport. The viewport's own size can be larger and scroll within it. */
function useViewportRoom(): { readonly size: ViewportSize | undefined; readonly mount: (element: HTMLDivElement | null) => (() => void) | undefined } {
	const [size, setSize] = useState<ViewportSize>();
	const mount = useCallback((element: HTMLDivElement | null) => {
		if (element === null) return undefined;
		const measure = (): void => {
			const { clientWidth: width, clientHeight: height } = element;
			if (width > 0 && height > 0) setSize((current) => (current?.width === width && current.height === height ? current : { width, height }));
		};
		const observer = new ResizeObserver(measure);
		observer.observe(element);
		measure();
		return () => observer.disconnect();
	}, []);
	return { size, mount };
}
